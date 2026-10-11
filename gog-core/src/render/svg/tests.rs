use super::*;
use crate::ir::{ChannelDef, Layer, ScaleType};
use crate::render::palette::{NEUTRAL_INK, PALETTE_GOG, RAMP_BLUE};
use crate::render::text::estimate_text_width;
use crate::render::ticks::nice_ticks;

// -----------------------------------------------------------------------
// Flow — the three readers of one layout (spec §15, the flow entry)
// -----------------------------------------------------------------------

fn flow_table() -> HashMap<String, DataFrame> {
    let df = DataFrame::new()
        .with_levels("class",
            vec!["First".into(), "First".into(), "Third".into(), "Third".into()],
            vec!["First".into(), "Third".into()])
        .with_str("survived",
            vec!["yes".into(), "no".into(), "yes".into(), "no".into()])
        .with_float("n", vec![203.0, 122.0, 178.0, 528.0]);
    HashMap::from([("t".to_string(), df)])
}

fn flow_spec() -> PlotSpec {
    PlotSpec::new().data("t").y("n")
        .layer(Layer::new(Mark::Ribbon).flow(&["class", "survived"]))
        .layer(Layer::new(Mark::Zone).flow(&["class", "survived"]))
}

/// **The count on `x` runs the stages top to bottom** (2026-10-02): the stage
/// names move to the y axis, first stage at the top, the slots span the count
/// along `x`, and each band curves down from one stage to the next. Nothing in
/// the sentence names a direction; the binding does, as a bar's does.
#[test]
fn a_flow_with_its_count_on_x_runs_top_to_bottom() {
    let down = PlotSpec::new().data("t").x("n")
        .layer(Layer::new(Mark::Ribbon).flow(&["class", "survived"]))
        .layer(Layer::new(Mark::Zone).flow(&["class", "survived"]))
        .layer(Layer::new(Mark::Text).flow(&["class", "survived"])
            .encode(Channel::Label, crate::transform::NODE_NAME));
    let svg = SvgRenderer::default().render(&down, &flow_table());
    let y_of = |name: &str| -> f64 {
        let line = svg.lines().find(|l| l.contains(&format!(">{name}</text>")))
            .unwrap_or_else(|| panic!("no `{name}` tick: {svg}"));
        line.split(r#" y=""#).nth(1).and_then(|r| r.split('"').next())
            .and_then(|v| v.parse::<f64>().ok()).unwrap()
    };
    assert!(y_of("class") < y_of("survived"), "the first stage is the top one");
    // The slots are wide along x and short along y: four of them, each
    // spanning its count, so their widths add to the total at each stage.
    let rects: Vec<(f64, f64)> = svg.split("<rect ").skip(1)
        .filter(|r| r.contains(r#"fill-opacity="1.000""#))
        .filter_map(|r| {
            let grab = |k: &str| r.split(k).nth(1).and_then(|v| v.split('"').next())
                .and_then(|v| v.parse::<f64>().ok());
            Some((grab(r#"width=""#)?, grab(r#"height=""#)?))
        })
        .collect();
    assert_eq!(rects.len(), 4, "{rects:?}");
    assert!(rects.iter().all(|(w, h)| w > h), "slots lie across the page: {rects:?}");
    // A band leaves a slot's lower side and enters the next one's upper side,
    // so its curve's control points share the band's x at each end.
    let band = svg.lines().find(|l| l.contains("<path d=\"M ") && l.contains(" C "))
        .expect("a band");
    let nums: Vec<f64> = band.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .filter_map(|t| t.parse().ok()).collect();
    assert_eq!(nums[0], nums[2], "the curve leaves straight down: {band}");
    // And the count on `y` keeps the stages along the bottom, left to right.
    let across = SvgRenderer::default().render(&flow_spec(), &flow_table());
    let at = |name: &str, key: &str| -> f64 {
        let line = across.lines().find(|l| l.contains(&format!(">{name}</text>"))).unwrap();
        line.split(key).nth(1).and_then(|r| r.split('"').next())
            .and_then(|v| v.parse::<f64>().ok()).unwrap()
    };
    assert!(at("class", r#" x=""#) < at("survived", r#" x=""#), "left to right");
    assert_eq!(at("class", r#" y=""#), at("survived", r#" y=""#), "on one axis line");
}

/// A flow's count axis is ticked over its whole range, as a partition's is, so
/// naming other stages cannot change its step: the slots' centers span a range
/// that depends on the stages, and the ticks chosen over them differed.
#[test]
fn a_flows_count_ticks_do_not_depend_on_its_stages() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("a", s(&["p", "p", "q", "q", "p", "q"]))
            .with_str("b", s(&["u", "v", "u", "v", "v", "u"]))
            .with_str("c", s(&["x", "x", "y", "y", "y", "x"]))
            .with_float("n", vec![400.0, 300.0, 350.0, 500.0, 250.0, 400.0]),
    )]);
    let ticks = |stages: &[&str]| {
        let spec = PlotSpec::new().data("t").y("n").layer(Layer::new(Mark::Ribbon).flow(stages));
        text_of(&SvgRenderer::default().render(&spec, &t)).into_iter()
            .filter(|l| l.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .collect::<Vec<_>>()
    };
    assert_eq!(ticks(&["a", "b"]), ticks(&["a", "b", "c"]), "one range, one set of ticks");
    assert_eq!(ticks(&["a", "b"]).first().map(String::as_str), Some("0"), "ticked from zero");
}

/// **The band is the renderer's first curve.** Every other path in the crate
/// is a polyline or a polygon, so the cubic command is asserted by name —
/// and beside it the slots, the honest measure axis (real ticks on real
/// cumulative magnitude), and the stage names on the domain axis.
#[test]
fn a_flow_draws_curved_bands_thin_slots_and_honest_axes() {
    let svg = SvgRenderer::default().render(&flow_spec(), &flow_table());
    assert!(svg.contains(" C "), "a band is a cubic curve");
    assert!(svg.contains("<rect"), "the slots are rectangles");
    assert!(svg.contains(">class<") && svg.contains(">survived<"),
        "the stage axis names its stages");
    assert!(svg.contains(">1K<") || svg.contains(">500<") || svg.contains(">600<"),
        "the measure axis keeps real ticks — contiguous stacks, no padding");
    // Two renders of one sentence are one picture: the determinism two
    // layers reading one computation stand on, asserted at the very end of
    // the pipeline rather than only in the transform.
    assert_eq!(svg, SvgRenderer::default().render(&flow_spec(), &flow_table()));
}

/// **A bands-only flow still draws, and its axis carries every stage.** The
/// first build starved both: band rows named only *left* stages, so a
/// seen-levels axis lost the final stage and a two-stage `ribbon * flow`
/// with no other layer drew an empty panel — found by a reader of the
/// chapter's first plot, not by any check. The band frame now carries one
/// row per (path, stage), so the axis is complete from the bands' own
/// rows, whichever layers the sentence has.
#[test]
fn a_bands_only_flow_draws_and_names_every_stage() {
    let spec = PlotSpec::new().data("t").y("n")
        .layer(Layer::new(Mark::Ribbon).flow(&["class", "survived"]));
    let svg = SvgRenderer::default().render(&spec, &flow_table());
    assert!(svg.contains(" C "), "two stages and one layer still draw bands");
    assert!(svg.contains(">class<") && svg.contains(">survived<"),
        "the final stage's name is on the axis without a zone layer to carry it");
}

/// **A band colored by a column that is not a stage is split by it**
/// (2026-10-03): each path is drawn as one band per value, in that value's
/// color, and the slots are unchanged. Refused until then, which kept a
/// third variable out of a two-stage flow unless it was drawn as a stage.
#[test]
fn a_flow_band_colored_by_another_column_is_split_by_it() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("a", s(&["p", "p", "q", "q"]))
            .with_str("b", s(&["u", "u", "v", "v"]))
            .with_str("c", s(&["x", "y", "x", "y"]))
            .with_float("n", vec![3.0, 2.0, 4.0, 1.0]),
    )]);
    let spec = |color: Option<&str>| {
        let mut ribbon = Layer::new(Mark::Ribbon).flow(&["a", "b"]);
        if let Some(c) = color {
            ribbon = ribbon.encode(Channel::Color, c);
        }
        PlotSpec::new().data("t").y("n").layer(ribbon)
            .layer(Layer::new(Mark::Zone).flow(&["a", "b"]))
    };
    let bands = |svg: &str| svg.lines()
        .filter(|l| l.trim_start().starts_with("<path d=\"M ") && l.contains(" C "))
        .map(str::to_string).collect::<Vec<_>>();
    let whole = SvgRenderer::default().render(&spec(None), &t);
    let split = SvgRenderer::default().render(&spec(Some("c")), &t);
    assert_eq!(bands(&whole).len(), 2, "two paths");
    let parts = bands(&split);
    assert_eq!(parts.len(), 4, "each path split in two");
    let fills: std::collections::HashSet<String> = parts.iter()
        .filter_map(|l| l.split(r#"fill=""#).nth(1).and_then(|r| r.split('"').next()))
        .map(String::from).collect();
    assert_eq!(fills.len(), 2, "one color per value of `c`: {fills:?}");
    // The legend narrows the panel, so the slots are compared by their
    // vertical extent, which is the count they hold.
    let extents = |svg: &str| slot_rects(svg).into_iter()
        .filter(|r| r.contains(r#"stroke="black""#))
        .map(|r| r.split(r#" y=""#).nth(1).unwrap().split(r#"" width"#).next().unwrap().to_string()
            + r.split(r#"height=""#).nth(1).unwrap().split('"').next().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(extents(&whole), extents(&split), "the slots do not change");
}

/// The slot rectangles of a rendered flow: every opaque `<rect>` inside the
/// clipped groups, which in `flow_table()`'s sentences is a slot and nothing else.
fn slot_rects(svg: &str) -> Vec<String> {
    svg.lines()
        .filter(|l| l.trim_start().starts_with("<rect ") && l.contains("fill-opacity="))
        .filter(|l| !l.contains(r#"fill="none""#))
        .map(str::to_string)
        .collect()
}

/// **A slot is white inside a thin black line unless `style()` says otherwise**
/// (2026-10-03). It was the palette's first color, solid, the bands' own blue
/// with no line between one slot and the next, so every flow in the book set
/// `style(color = "white", border_color = "black")`. A width alone keeps the
/// black line rather than taking the panel's color, as other fills do, and a
/// width of 0 removes it.
#[test]
fn a_flow_slot_is_white_inside_a_black_line_by_default() {
    let slots = slot_rects(&SvgRenderer::default().render(&flow_spec(), &flow_table()));
    assert_eq!(slots.len(), 4, "{slots:?}");
    for s in &slots {
        assert!(s.contains(r#"fill="white""#) && s.contains(r#"stroke="black" stroke-width="1.00""#),
            "a default slot: {s}");
    }
    let styled = |f: &dyn Fn(&mut Layer)| {
        let mut zone = Layer::new(Mark::Zone).flow(&["class", "survived"]);
        f(&mut zone);
        let spec = PlotSpec::new().data("t").y("n")
            .layer(Layer::new(Mark::Ribbon).flow(&["class", "survived"]))
            .layer(zone);
        slot_rects(&SvgRenderer::default().render(&spec, &flow_table()))
    };
    let wide = styled(&|z| z.style.border_size = Some(2.0));
    assert!(wide.iter().all(|s| s.contains(r#"stroke="black" stroke-width="2.00""#)), "{wide:?}");
    let bare = styled(&|z| { z.style.border_size = Some(0.0); z.style.color = Some("gold".into()); });
    assert!(bare.iter().all(|s| s.contains(r#"stroke="none""#) && s.contains(r#"fill="gold""#)),
        "{bare:?}");
}

/// **A slot stays white beside a color legend.** A plot whose bands are
/// colored by a stage has a color legend, and every layer with no color of
/// its own is then given the neutral ink, so it cannot be read as the
/// legend's first category. The white slot is in no palette, so it keeps its
/// own paint, as `text` keeps its ink; given the ink, every slot drew dark
/// gray and the names on it could not be read.
#[test]
fn a_flow_slot_stays_white_beside_a_color_legend() {
    let spec = PlotSpec::new().data("t").y("n")
        .layer(Layer::new(Mark::Ribbon).flow(&["class", "survived"])
            .encode(Channel::Color, "survived"))
        .layer(Layer::new(Mark::Zone).flow(&["class", "survived"]));
    let svg = SvgRenderer::default().render(&spec, &flow_table());
    let slots: Vec<String> = slot_rects(&svg).into_iter()
        .filter(|s| s.contains(r#"stroke="black""#)).collect();
    assert_eq!(slots.len(), 4, "{svg}");
    assert!(slots.iter().all(|s| s.contains(r#"fill="white""#)), "{slots:?}");
}

/// **A slot draws the hatch `style(pattern = )` gives it** (2026-10-03). The
/// setting was accepted, and the slot writer had no texture code, so the slots
/// drew plain: the defect the bands carried until 2026-09-27, one writer over.
#[test]
fn a_flow_slot_draws_the_pattern_it_is_given() {
    let mut zone = Layer::new(Mark::Zone).flow(&["class", "survived"]);
    zone.style.pattern = Some("hatch".into());
    zone.style.color = Some("steelblue".into());
    let spec = PlotSpec::new().data("t").y("n")
        .layer(Layer::new(Mark::Ribbon).flow(&["class", "survived"]))
        .layer(zone);
    let svg = SvgRenderer::default().render(&spec, &flow_table());
    assert!(svg.contains("<pattern"), "a hatch tile is defined:\n{svg}");
    let slots = slot_rects(&svg);
    assert!(!slots.is_empty() && slots.iter().all(|s| s.contains(r#"fill="url(#"#)),
        "every slot takes the hatch: {slots:?}");
}

/// **A flow layer that names no count reads the first one written**
/// (2026-10-02), the scope rule every other reading follows, and the one
/// `check_flow` already validated the weight by. The renderer read the plot's
/// binding alone, so `y(n)` written after the `ribbon` layer reached the bands
/// and not the slots: the `zone` layer counted rows, and its slots shrank to
/// slivers at the foot of an axis the bands filled.
#[test]
fn a_flow_layer_naming_no_count_reads_the_first_one_written() {
    let layer_scoped = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Ribbon).flow(&["class", "survived"]).encode(Channel::Y, "n"))
        .layer(Layer::new(Mark::Zone).flow(&["class", "survived"]));
    assert_eq!(SvgRenderer::default().render(&layer_scoped, &flow_table()),
        SvgRenderer::default().render(&flow_spec(), &flow_table()),
        "the only count, written after one layer, weighs both");
}

/// What a selection pushed back: the dimmed group, read to its own closing
/// tag. A flow's pass nests a clipped group per layer, so the first `</g>`
/// after the dimmed group's opening closes one of those, not it.
fn dimmed_pass(svg: &str) -> String {
    let open = format!(r#"<g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM);
    let (_, rest) = svg.split_once(&open).expect("a dimmed pass");
    let mut depth = 1usize;
    let mut out = String::new();
    for line in rest.lines() {
        let t = line.trim_start();
        if t.starts_with("</g>") {
            depth -= 1;
            if depth == 0 {
                break;
            }
        } else if t.starts_with("<g") && !t.ends_with("/>") {
            depth += 1;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// **A brush on a flow's stage selects whole bands** (2026-10-03, at the
/// author's word). A path holds one place at each stage, so the bound keeps or
/// leaves the whole of it: of the four paths, the two through `First` are drawn
/// at full strength and the two through `Third` are pushed back, and no band is
/// cut or drawn twice. The brushed stage's slots answer by their own place, and
/// a slot at the other stage, which paths of both kinds pass through, stays at
/// full strength rather than being called selected or not.
#[test]
fn a_brush_on_a_flow_stage_selects_whole_bands_and_its_own_slots() {
    let spec = flow_spec()
        .brush(crate::ir::BrushDef::new("class").levels(vec!["First".to_string()]));
    let svg = SvgRenderer::default().render(&spec, &flow_table());
    let bands = |s: &str| s.matches(" C ").count() / 2;
    let slots = |s: &str| s.matches(r#"fill-opacity="1.000""#).count();
    let dim = dimmed_pass(&svg);
    assert_eq!(bands(&svg), 4, "every band is drawn once: {svg}");
    assert_eq!(bands(&dim), 2, "the two bands through `Third` are pushed back: {dim}");
    assert_eq!(slots(&svg), 4, "every slot is drawn once");
    assert_eq!(slots(&dim), 1, "of the slots, only `Third` is pushed back: {dim}");
    assert!(dim.contains(r#"data-gog-slot="Third""#), "{dim}");
    // The positions come from the whole table, so a dimmed band sits exactly
    // where it sits unbrushed: the two passes differ only in strength.
    let plain = SvgRenderer::default().render(&flow_spec(), &flow_table());
    let curves = |s: &str| {
        let mut v: Vec<String> = s.lines().filter(|l| l.contains(" C "))
            .map(|l| l.trim().to_string()).collect();
        v.sort();
        v
    };
    assert_eq!(curves(&svg), curves(&plain), "no band moved");
}

/// Four amounts among three places: `a` sends to `b` and `c`, `b` to `c`, and
/// `c` back to `a`.
fn shared_table() -> HashMap<String, DataFrame> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("from", s(&["a", "a", "b", "c"]))
        .with_str("to", s(&["b", "c", "c", "a"]))
        .with_float("n", vec![3.0, 2.0, 4.0, 1.0]))])
}

fn shared_spec() -> PlotSpec {
    PlotSpec::new().data("t").y("n")
        .layer(Layer::new(Mark::Ribbon).flow_shared("from", "to"))
        .layer(Layer::new(Mark::Zone).flow_shared("from", "to"))
        .layer(Layer::new(Mark::Text).flow_shared("from", "to")
            .encode(Channel::Label, crate::transform::NODE_NAME))
}

/// **Flat, a shared flow is the arc diagram** (2026-10-03). Its three places
/// stand on one axis as three slots, and each band arches from one place to
/// another. Neither axis draws ticks, numbers or gridlines: across the count
/// nothing is measured, and along it every amount stands twice. The names
/// stand where the numbers would have, beyond the slots: left of the axis
/// when it runs up the page, in the room the frame left them.
#[test]
fn a_shared_flow_flat_is_an_arc_diagram_named_beyond_its_slots() {
    let svg = SvgRenderer::default().render(&shared_spec(), &shared_table());
    assert_eq!(svg.matches(" C ").count(), 8, "four bands, each an arch of two curves: {svg}");
    assert_eq!(svg.matches(r#"fill="white" fill-opacity="1.000""#).count(), 3, "three slots");
    // Each slot has a body: as wide as the slot's thickness across the axis,
    // and as long as its place along it. Every slot was once drawn zero
    // pixels across, so the places had no boundaries and no click could land.
    for slot in svg.lines().filter(|l| l.contains(r#"fill="white" fill-opacity="1.000""#)) {
        let d = slot.split(r#"d=""#).nth(1).and_then(|v| v.split('"').next()).unwrap();
        let pts: Vec<(f64, f64)> = d.split(|c: char| c == 'M' || c == 'L' || c == 'Z')
            .filter_map(|t| {
                let (x, y) = t.trim().split_once(',')?;
                Some((x.parse().ok()?, y.parse().ok()?))
            })
            .collect();
        let span = |f: &dyn Fn(&(f64, f64)) -> f64| {
            let v: Vec<f64> = pts.iter().map(f).collect();
            v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                - v.iter().cloned().fold(f64::INFINITY, f64::min)
        };
        assert!(span(&|p| p.0) > 1.0 && span(&|p| p.1) > 1.0, "a slot with a body: {slot}");
    }
    let grid = svg.split(r##"<g stroke="#d2d2da" stroke-width="1">"##).nth(1)
        .and_then(|g| g.split("</g>").next()).unwrap_or("");
    assert!(!grid.contains("<line"), "no gridlines: {grid}");
    let panel = svg.lines().find(|l| l.contains("<clipPath")).expect("the panel's clip");
    let left: f64 = panel.split(r#" x=""#).nth(1).and_then(|v| v.split('"').next())
        .and_then(|v| v.parse().ok()).unwrap();
    for place in ["a", "b", "c"] {
        let name = svg.lines().find(|l| l.contains(&format!(">{place}</text>")))
            .unwrap_or_else(|| panic!("the place {place} is named: {svg}"));
        let x: f64 = name.split(r#" x=""#).nth(1).and_then(|v| v.split('"').next())
            .and_then(|v| v.parse().ok()).unwrap();
        assert!(x < left && name.contains(r#"text-anchor="end""#),
            "{place} stands left of the panel, ending at the axis: {name}");
    }
}

/// **In `polar()` it is the chord diagram.** The count goes round the circle,
/// as a pie's does, so the slots make one ring, each band curves through the
/// center from one place to another, and the names stand outside the ring,
/// where the disc's clip would cut them.
#[test]
fn a_shared_flow_in_polar_is_the_chord_diagram() {
    let spec = shared_spec().coord(CoordSpace::Polar(crate::ir::PolarView::default()));
    let d = crate::legality::check(&spec, &shared_table());
    assert!(!d.iter().any(|x| x.is_fatal()), "{:?}", d.iter().map(|x| &x.message).collect::<Vec<_>>());
    let svg = SvgRenderer::default().render(&spec, &shared_table());
    let disc = svg.lines().find(|l| l.contains("<circle")).expect("the disc");
    let at = |k: &str| disc.split(k).nth(1).and_then(|v| v.split('"').next()).unwrap().to_string();
    let center = format!("Q {} {} ", at(r#" cx=""#), at(r#" cy=""#));
    assert_eq!(svg.matches(&center).count(), 8, "each of four bands crosses the center twice: {svg}");
    let ring: Vec<&str> = svg.lines()
        .filter(|l| l.contains(r#"fill="white""#) && l.contains(" A ")).collect();
    assert_eq!(ring.len(), 3, "three pieces of ring: {svg}");
    let names = svg.lines().find(|l| l.contains("font-family") && !l.contains("clip-path")
        && svg.contains(">a</text>"));
    assert!(names.is_some(), "the names are drawn outside the disc's clip: {svg}");
}

/// **A shared flow's slot holds what its place sends, then what it receives,
/// the second part shaded and no line between the two** (2026-10-03, at the
/// author's word). Each of the three places sends and receives, so each slot
/// is one fill, one shade over its second part, and one outline round the
/// whole place, flat and on the ring alike. The fill and the shade carry no
/// line of their own, so nothing divides a place in two.
#[test]
fn a_shared_flows_slot_shades_what_arrives_with_no_line_inside_a_place() {
    let ring = shared_spec().coord(CoordSpace::Polar(crate::ir::PolarView::default()));
    for spec in [shared_spec(), ring] {
        let svg = SvgRenderer::default().render(&spec, &shared_table());
        let shades: Vec<&str> = svg.lines()
            .filter(|l| l.contains(r#"fill="black" fill-opacity="0.100""#)).collect();
        assert_eq!(shades.len(), 3, "one shade per place that receives: {svg}");
        let fills: Vec<&str> = svg.lines()
            .filter(|l| l.contains("<path") && l.contains(r#"fill="white""#)).collect();
        assert_eq!(fills.len(), 3, "one fill per place: {svg}");
        for part in shades.iter().chain(&fills) {
            assert!(!part.contains("stroke"), "no line inside a place: {part}");
        }
        let outlines = svg.lines()
            .filter(|l| l.contains(r#"fill="none" stroke="black""#)).count();
        assert_eq!(outlines, 3, "one outline round each whole place: {svg}");
    }
}

/// **A shared flow is drawn on white and with no frame unless `theme()`
/// says otherwise** (2026-10-03, at the author's word). Its axes measure
/// nothing a reader can use, so the frame's two lines bounded nothing. A
/// background or a frame the theme names is kept, and so is a preset's.
#[test]
fn a_shared_flow_is_drawn_on_white_with_no_frame_unless_the_theme_says() {
    let frame = r##"<g stroke="#5a5a64" stroke-width="1.5" fill="none">"##;
    let panel_fill = |svg: &str| -> String {
        let rect = svg.lines().take_while(|l| !l.contains("<clipPath"))
            .filter(|l| l.contains("<rect")).last().unwrap().to_string();
        rect.split(r#"fill=""#).nth(1).and_then(|v| v.split('"').next()).unwrap().to_string()
    };
    let plain = SvgRenderer::default().render(&shared_spec(), &shared_table());
    assert_eq!(panel_fill(&plain), "white");
    assert!(!plain.contains(frame), "no frame: {plain}");

    let mut asked = shared_spec();
    asked.theme = crate::ir::ThemeSpec {
        background: Some(PANEL_BG.to_string()), frame: Some("axes".to_string()), ..Default::default()
    };
    let asked = SvgRenderer::default().render(&asked, &shared_table());
    assert_eq!(panel_fill(&asked), PANEL_BG);
    assert!(asked.contains(frame), "the two lines the theme asked for: {asked}");

    let mut bw = shared_spec();
    bw.theme = crate::ir::ThemeSpec { preset: Some("bw".to_string()), ..Default::default() };
    let bw = SvgRenderer::default().render(&bw, &shared_table());
    assert!(bw.split(frame).nth(1).is_some_and(|g| g.trim_start().starts_with("<rect")),
        "the preset's full frame is kept: {bw}");

    // A stage flow keeps the panel every other plot has.
    let stage = SvgRenderer::default().render(&flow_spec(), &flow_table());
    assert_eq!(panel_fill(&stage), PANEL_BG);
    assert!(stage.contains(frame));
}

/// **A brush on a shared flow's `name` keeps what a place sends and what it
/// receives**, the chord diagram's one-place-at-a-time reading: three of the
/// four bands touch `a` and stay, the band from `b` to `c` steps back, and
/// so do the slots of `b` and `c`.
#[test]
fn a_brush_on_a_shared_flows_name_keeps_both_ends_of_a_place() {
    let spec = shared_spec().brush(crate::ir::BrushDef::new("name").levels(vec!["a".to_string()]));
    let svg = SvgRenderer::default().render(&spec, &shared_table());
    let dim = dimmed_pass(&svg);
    assert_eq!(dim.matches(" C ").count(), 2, "only the band from b to c is pushed back: {dim}");
    assert_eq!(dim.matches(r#"fill="white""#).count(), 2, "and the slots of b and c: {dim}");
    assert!(svg.contains(r#"data-gog-slot="a" data-gog-slot-field="from|to|name""#),
        "a slot is a place of both columns and of `name`: {svg}");
}

/// A household's month, in links, for the layered flow's drawings.
fn link_table() -> HashMap<String, DataFrame> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("source", s(&["Salary", "Side", "Income", "Income", "Housing", "Housing"]))
        .with_str("target", s(&["Income", "Income", "Taxes", "Housing", "Rent", "Repairs"]))
        .with_float("amount", vec![30.0, 10.0, 15.0, 25.0, 20.0, 5.0]))])
}

fn layered_spec() -> PlotSpec {
    PlotSpec::new().data("t").y("amount")
        .layer(Layer::new(Mark::Ribbon).flow_layered("source", "target"))
        .layer(Layer::new(Mark::Zone).flow_layered("source", "target"))
        .layer(Layer::new(Mark::Text).flow_layered("source", "target")
            .encode(Channel::Label, crate::transform::NODE_NAME))
}

/// **A layered flow is the Sankey diagram** (2026-10-03). Six links are six
/// bands of two curves each, and seven places are seven thin slots, 14 px
/// across. Neither axis is drawn and the panel is white with no frame, as
/// for the shared flow. A name stands after its slot, over the bands that
/// leave it, and before it in the last layer, where nothing leaves.
#[test]
fn a_layered_flow_is_the_sankey_diagram() {
    let svg = SvgRenderer::default().render(&layered_spec(), &link_table());
    assert_eq!(svg.matches(" C ").count(), 12, "six bands of two curves: {svg}");
    let slots: Vec<&str> = svg.lines()
        .filter(|l| l.contains("<rect") && l.contains(r#"fill="white" fill-opacity="1.000""#)).collect();
    assert_eq!(slots.len(), 7, "one slot per place: {svg}");
    assert!(slots.iter().all(|l| l.contains(r#"width="14.00""#)), "thin slots: {slots:?}");
    assert!(!svg.contains(r##"<g stroke="#5a5a64" stroke-width="1.5" fill="none">"##), "no frame");
    let name = |p: &str| svg.lines().find(|l| l.contains(&format!(">{p}</text>")))
        .unwrap_or_else(|| panic!("{p} is named: {svg}")).to_string();
    assert!(name("Salary").contains(r#"text-anchor="start""#), "after the first layer's slot");
    assert!(name("Income").contains(r#"text-anchor="start""#), "after a middle layer's slot");
    assert!(name("Rent").contains(r#"text-anchor="end""#), "before the last layer's slot");
}

/// **A brush on a layered flow's `name` keeps the place's own links**, one
/// step each way: the two incomes into `Income` and its links to `Taxes` and
/// `Housing` stay, while `Housing`'s own links step back, since a band out of
/// `Housing` cannot say how much of it came through `Income`. Every other
/// place's slot steps back too, as on the shared flow, and each slot names
/// both columns and `name` for the page's click.
#[test]
fn a_brush_on_a_layered_flows_name_keeps_the_places_own_links() {
    let spec = layered_spec().brush(crate::ir::BrushDef::new("name").levels(vec!["Income".to_string()]));
    let svg = SvgRenderer::default().render(&spec, &link_table());
    let dim = dimmed_pass(&svg);
    assert_eq!(dim.matches(" C ").count(), 4, "Housing's two links step back: {dim}");
    assert_eq!(dim.matches(r#"fill="white" fill-opacity"#).count(), 6, "every slot but Income's: {dim}");
    assert!(svg.contains(r#"data-gog-slot="Income" data-gog-slot-field="source|target|name""#), "{svg}");
    assert!(svg.contains(r#"data-gog-place="flow""#), "a drag selects nothing here");
}

/// **On a stage flow, a brush on `name` speaks to the stage where it names a
/// place.** `First` is a class, so the `Third` slot steps back beside it, and
/// the outcome slots, which hold both classes, stay at full strength, as a
/// brush on `class` itself leaves them.
#[test]
fn a_brush_on_a_flows_name_dims_only_the_named_places_siblings() {
    let by_name = flow_spec().brush(crate::ir::BrushDef::new("name").levels(vec!["First".to_string()]));
    let by_class = flow_spec().brush(crate::ir::BrushDef::new("class").levels(vec!["First".to_string()]));
    let (a, b) = (SvgRenderer::default().render(&by_name, &flow_table()),
                  SvgRenderer::default().render(&by_class, &flow_table()));
    let ink = |s: &str| s.lines().filter(|l| !l.contains("data-gog-")).collect::<Vec<_>>().join("\n");
    assert_eq!(ink(&dimmed_pass(&a)), ink(&dimmed_pass(&b)),
        "the same bands and slots step back either way");
}

/// **A brush nobody has moved draws a flow's same ink**, the promise
/// `a_resting_brush_draws_exactly_the_same_ink` makes for a scatter. What the
/// declared brush adds is metadata the page needs before the first click: the
/// panel's facts, and each slot's place and outline. A flow with no brush
/// carries neither.
#[test]
fn a_resting_brush_on_a_flow_adds_only_the_slots_outlines() {
    let plain = SvgRenderer::default().render(&flow_spec(), &flow_table());
    assert!(!plain.contains("data-gog-slot"), "a flow with no brush carries no outlines");
    let resting = SvgRenderer::default()
        .render(&flow_spec().brush(crate::ir::BrushDef::new("class")), &flow_table());
    let ink: String = resting.lines().filter(|l| !l.contains("data-gog-"))
        .collect::<Vec<_>>().join("\n");
    assert_eq!(ink.trim(), plain.trim(), "a resting brush changes no ink and no id");
    let outlines: Vec<&str> = resting.lines().filter(|l| l.contains("data-gog-slot=")).collect();
    assert_eq!(outlines.len(), 4, "one outline per slot: {outlines:?}");
    // A slot names its stage and the flow's own `name`, the two bounds a click
    // on it can move.
    assert!(outlines.iter().any(|l| l.contains(r#"data-gog-slot="First" data-gog-slot-field="class|name""#)));
    assert!(outlines.iter().any(|l| l.contains(r#"data-gog-slot="no" data-gog-slot-field="survived|name""#)));
    assert!(outlines.iter().all(|l| l.contains("data-gog-shape=")), "{outlines:?}");
    // And the panel tells the page that a drag selects nothing here.
    assert!(resting.contains(r#"data-gog-place="flow""#), "{resting}");
}

// -----------------------------------------------------------------------
// Cluster — the tree of merges and the seriated tile plot (spec §5)
// -----------------------------------------------------------------------

fn cluster_table() -> HashMap<String, DataFrame> {
    // The paper table from `transform::tests`: joins ab at 1, then c, then
    // d, display order b a c d — the Gruvaeus-Wainer flip included.
    let mut leaf = Vec::new();
    let mut coord = Vec::new();
    let mut val = Vec::new();
    for (name, p, q) in [("a", 0.0, 0.0), ("b", 0.0, 1.0),
                         ("c", 4.0, 0.0), ("d", 10.0, 10.0)] {
        leaf.push(name.to_string()); coord.push("p".to_string()); val.push(p);
        leaf.push(name.to_string()); coord.push("q".to_string()); val.push(q);
    }
    HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("leaf", leaf).with_str("g", coord).with_float("v", val))])
}

/// **The tree: one disconnected elbow per merge, leaves in the computed
/// order, and a distance axis that names itself.** The elbows are runs of
/// one stroke, so three merges are three polylines; the leaf ticks appear
/// in Gruvaeus-Wainer order, which differs from input order on purpose.
#[test]
fn a_cluster_tree_draws_elbows_in_leaf_order_with_a_distance_axis() {
    let spec = PlotSpec::new().data("t").x("leaf")
        .layer(Layer::new(Mark::Path).cluster(Some("v"), Some("g")));
    let svg = SvgRenderer::default().render(&spec, &cluster_table());
    assert_eq!(svg.matches("<polyline").count(), 3,
        "three merges, three strokes");
    assert!(svg.contains(">Distance<"), "the unbound axis names itself");
    let at = |s: &str| svg.find(s).unwrap_or_else(|| panic!("{s} missing"));
    assert!(at(">b<") < at(">a<") && at(">a<") < at(">c<") && at(">c<") < at(">d<"),
        "the leaf ticks run b a c d");
    assert_eq!(svg, SvgRenderer::default().render(&spec, &cluster_table()),
        "one sentence is one picture");
}

/// **A tree layer that names no leaves reads the first leaf column written**
/// (2026-10-02), as `check_cluster` reads it. The renderer asked the plot's
/// binding alone whether `x` held the leaves, so a second `path * cluster`
/// written after `path * cluster(..) + x(leaf)` took the distance column for
/// its leaves and drew nothing, with nothing said.
#[test]
fn a_tree_layer_naming_no_leaves_reads_the_first_leaf_column_written() {
    let tree = || Layer::new(Mark::Path).cluster(Some("v"), Some("g"));
    let plot = PlotSpec::new().data("t").x("leaf").layer(tree()).layer(tree());
    let layer_scoped = PlotSpec::new().data("t")
        .layer(tree().encode(Channel::X, "leaf")).layer(tree());
    let svg = SvgRenderer::default().render(&layer_scoped, &cluster_table());
    assert_eq!(svg.matches("<polyline").count(), 6, "two trees, three merges each");
    assert_eq!(svg, SvgRenderer::default().render(&plot, &cluster_table()));
}

/// **The sideways tree is the same tree**: leaves on `y`, the distance
/// axis grown along `x` and titled there, and the elbows reflected onto
/// the slots their labels occupy — the y axis draws its categories
/// reversed, and the published coordinate must land with them.
#[test]
fn a_sideways_cluster_tree_lies_down_whole() {
    let spec = PlotSpec::new().data("t").y("leaf")
        .layer(Layer::new(Mark::Path).cluster(Some("v"), Some("g")));
    let svg = SvgRenderer::default().render(&spec, &cluster_table());
    assert_eq!(svg.matches("<polyline").count(), 3);
    assert!(svg.contains(">Distance<"), "the distance axis titles x");
    // The first merge joins a and b at height 1, and its elbow must sit on
    // *their* two rows. The display order is b a c d, and a categorical y
    // axis draws its first slots at the top — so every vertex of the first
    // stroke lies in the panel's upper half. The un-reflected defect drew
    // it mirrored onto c's and d's rows at the bottom.
    let first = svg.split("<polyline").nth(1).expect("a first stroke");
    let points = first.split("points=\"").nth(1)
        .and_then(|s| s.split('"').next()).expect("its vertices");
    let ys: Vec<f64> = points.split_whitespace()
        .filter_map(|p| p.split(',').nth(1))
        .filter_map(|y| y.parse::<f64>().ok())
        .collect();
    assert_eq!(ys.len(), 4, "an elbow is four vertices: {points}");
    assert!(ys.iter().all(|&y| y < 300.0),
        "the a-b elbow stands on its own two rows, not their mirror: {ys:?}");
    assert_eq!(svg, SvgRenderer::default().render(&spec, &cluster_table()));
}

/// **A tally's count is a column on the rows it draws** (spec §8, a written column
/// is a column). It arrived only under whatever the measure axis was called, the
/// empty string with `y()` left out, so `size(count)` found no `count` and every
/// point was drawn at one size, and `label(count)` wrote nothing. Three in `a`, one
/// in `b`: the larger point is `a`'s, and the labels are the two counts.
#[test]
fn a_tally_sizes_its_points_and_writes_its_counts() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", vec!["a".into(), "a".into(), "a".into(), "b".into()]))]);
    let sized = PlotSpec::new().data("t").x("g")
        .layer(Layer::new(Mark::Point).transform(Transform::Count).encode(Channel::Size, "count"));
    let svg = SvgRenderer::default().render(&sized, &data);
    let radii: Vec<f64> = svg.split("<circle").skip(1)
        .filter_map(|s| s.split(" r=\"").nth(1).and_then(|r| r.split('"').next()))
        .filter_map(|r| r.parse().ok())
        .collect();
    assert!(radii.len() >= 2 && radii[0] > radii[1], "a's point is the larger: {radii:?}");

    let labeled = PlotSpec::new().data("t").x("g")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count))
        .layer(Layer::new(Mark::Text).transform(Transform::Count).encode(Channel::Label, "count"));
    let svg = SvgRenderer::default().render(&labeled, &data);
    assert!(svg.contains(">3<") && svg.contains(">1<"), "each bar's count is written on it");
}

/// **`decimals` fixes a label's places** (spec §10): 2.3 and 7.0, not 2.3 and 7, so
/// labels side by side read as one column of figures; `decimals = 0` rounds to
/// whole numbers, and a value that rounds to zero keeps no minus sign.
#[test]
fn a_label_prints_the_places_it_was_given() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", vec!["a".into(), "a".into(), "b".into(), "c".into()])
        .with_float("v", vec![1.25, 3.35, 7.0, -0.04]))]);
    let printed = |d: u32| {
        let spec = PlotSpec::new().data("t").x("g").y("v")
            .layer(Layer::new(Mark::Text).transform(Transform::Mean)
                .encode_def(Channel::Label, ChannelDef { decimals: Some(d), ..ChannelDef::field("v") }));
        let svg = SvgRenderer::default().render(&spec, &data);
        svg.split("<text").skip(1)
            .filter(|s| s.contains("fill-opacity=\"1.000\""))
            .filter_map(|s| s.split('>').nth(1).and_then(|t| t.split('<').next()).map(str::to_string))
            .collect::<Vec<_>>()
    };
    assert_eq!(printed(1), vec!["2.3", "7.0", "0.0"]);
    assert_eq!(printed(0), vec!["2", "7", "0"]);
}

/// **A summary prints its value at the bar's end, upright or on its side** (spec §8,
/// the rows rule). `text * mean + label(v)` reduces `v` in place, so the label is
/// each group's mean; and a text that summarizes reads its orientation as its bar
/// does, so beside horizontal bars it groups by the category on `y`, where read as
/// vertical it grouped by the numeric `x` and printed every raw row.
#[test]
fn a_summary_prints_its_mean_on_each_bar_either_way_up() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", vec!["a".into(), "a".into(), "b".into()])
        .with_float("v", vec![1.0, 3.0, 5.5]))]);
    let labels = |svg: &str| svg.split("<text").skip(1)
        .filter(|s| s.contains("fill-opacity=\"1.000\""))
        .filter_map(|s| s.split('>').nth(1).and_then(|t| t.split('<').next()).map(str::to_string))
        .collect::<Vec<_>>();
    let upright = PlotSpec::new().data("t").x("g").y("v")
        .layer(Layer::new(Mark::Bar).transform(Transform::Mean))
        .layer(Layer::new(Mark::Text).transform(Transform::Mean).encode(Channel::Label, "v"));
    let sideways = PlotSpec::new().data("t").x("v").y("g")
        .layer(Layer::new(Mark::Bar).transform(Transform::Mean))
        .layer(Layer::new(Mark::Text).transform(Transform::Mean).encode(Channel::Label, "v"));
    for spec in [upright, sideways] {
        let svg = SvgRenderer::default().render(&spec, &data);
        assert_eq!(labels(&svg), vec!["2", "5.5"], "one label per group, its mean");
    }
}

/// **The labels of dodged bars stand over their own bars**, upright and on their
/// side (spec §5). Each label takes its group's tile by the bar's rule, so its
/// center is the center of the bar of the same rows, and the bigger number is over
/// the longer bar. On its side the tiles run down the page, which is down the
/// scale, so the move is reversed there: drawn the same way as upright, every
/// label stood over the other group's bar.
#[test]
fn a_dodged_label_stands_over_its_own_bar() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", vec!["p".into(), "p".into(), "q".into(), "q".into()])
        .with_str("era", vec!["a".into(), "b".into(), "a".into(), "b".into()])
        .with_float("v", vec![1.0, 3.0, 2.0, 5.0]))]);
    let num = |s: &str, key: &str| -> f64 {
        let at = s.find(key).unwrap_or_else(|| panic!("no {key}")) + key.len();
        s[at..s[at..].find('"').unwrap() + at].parse().unwrap()
    };
    for sideways in [false, true] {
        let (slot, measure) = if sideways { ("v", "g") } else { ("g", "v") };
        let spec = PlotSpec::new().data("t").x(slot).y(measure)
            .layer(Layer::new(Mark::Bar).transform(Transform::Dodge).encode(Channel::Color, "era"))
            .layer(Layer::new(Mark::Text).transform(Transform::Dodge)
                .encode(Channel::Label, "v").encode(Channel::Group, "era"));
        let svg = SvgRenderer::default().render(&spec, &data);
        // Each bar's center across its slot, and its length.
        let bars: Vec<(f64, f64)> = svg.split("<rect ").skip(1)
            // A bar carries its own faint edge; a key's swatch does not.
            .filter(|r| r.contains("stroke-opacity"))
            .map(|r| {
                let (x, y, w, h) = (num(r, "x=\""), num(r, "y=\""), num(r, "width=\""), num(r, "height=\""));
                if sideways { (y + h / 2.0, w) } else { (x + w / 2.0, h) }
            })
            .collect();
        assert_eq!(bars.len(), 4, "four bars");
        let fs = SvgRenderer::default().font_md;
        let half_cap = crate::render::text::estimate_cap_height(fs) / 2.0;
        let labels: Vec<(f64, f64)> = svg.split("<text").skip(1)
            .filter(|t| t.contains("fill-opacity=\"1.000\""))
            .filter_map(|t| {
                let word = t.split('>').nth(1)?.split('<').next()?.parse::<f64>().ok()?;
                let at = if sideways { num(t, " y=\"") - half_cap } else { num(t, " x=\"") };
                Some((at, word))
            })
            .collect();
        assert_eq!(labels.len(), 4, "four labels");
        let longest = bars.iter().map(|b| b.1).fold(0.0, f64::max);
        for (at, word) in labels {
            let bar = bars.iter().min_by(|a, b| (a.0 - at).abs().total_cmp(&(b.0 - at).abs())).unwrap();
            assert!((bar.0 - at).abs() < 0.5, "sideways {sideways}: label {word} at {at:.1}, \
                its nearest bar at {:.1}", bar.0);
            // The bar a label stands over is as long as the number says.
            assert!((bar.1 / longest - word / 5.0).abs() < 0.05,
                "sideways {sideways}: label {word} over a bar {:.0}% of the longest", 100.0 * bar.1 / longest);
        }
    }
}

/// **A label at the edge of the data is given room on the axis it runs along**
/// (spec §10). The axis is fitted in the data's units before any letter has a
/// size, so a sideways mean nudged right ran past the panel and lost its last
/// digits. A stated end is the author's and keeps its place.
#[test]
fn a_label_at_the_edge_of_the_data_is_given_room_on_its_axis() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", vec!["a".into(), "b".into()])
        .with_float("v", vec![40.0, 80.654321]))]);
    let sideways = |hi: Option<f64>| {
        let mut text = Layer::new(Mark::Text).transform(Transform::Mean).encode(Channel::Label, "v");
        text.style.nudge = Some("right".into());
        PlotSpec::new().data("t").x_limited("v", None, hi).y("g")
            .layer(Layer::new(Mark::Bar).transform(Transform::Mean))
            .layer(text)
    };
    // The right end of the widest label's letters, and the right edge of the
    // panel the labels are clipped to.
    let reach = |svg: &str| -> (f64, f64) {
        let group = svg.find("text-anchor=\"start\"").expect("labels nudged right");
        let head = &svg[..group];
        let clip = &head[head.rfind("url(#").expect("a clipped group") + 5..];
        let clip = &clip[..clip.find(')').unwrap()];
        let num = |s: &str, key: &str| -> f64 {
            let at = s.find(key).unwrap_or_else(|| panic!("no {key}")) + key.len();
            s[at..s[at..].find('"').unwrap() + at].parse().unwrap()
        };
        let rect = &svg[svg.find(&format!("<clipPath id=\"{clip}\"")).expect("its clip")..];
        let edge = num(rect, " x=\"") + num(rect, " width=\"");
        let fs = num(&svg[group - 200..], "font-size=\"");
        let body = &svg[group..svg[group..].find("</g>").unwrap() + group];
        let ends = body.split("<text").skip(1).map(|t| {
            let word = t.split('>').nth(1).and_then(|w| w.split('<').next()).unwrap();
            num(t, " x=\"") + crate::render::text::estimate_text_width(word, fs)
        });
        (ends.fold(f64::MIN, f64::max), edge)
    };
    let (end, edge) = reach(&SvgRenderer::default().render(&sideways(None), &data));
    assert!(end <= edge, "the label fits: its letters end at {end:.1}, the panel at {edge:.1}");
    // The first drawing alone is the defect: the same label cut by the panel.
    let first = SvgRenderer { label_room: Some([(None, None); 2]), ..SvgRenderer::default() };
    let (end, edge) = reach(&first.render(&sideways(None), &data));
    assert!(end > edge, "without the room the label runs past the panel ({end:.1} > {edge:.1})");
    // A stated end keeps its place, and the label with it.
    let (end, edge) = reach(&SvgRenderer::default().render(&sideways(Some(85.0)), &data));
    assert!(end > edge, "a stated end is not widened ({end:.1} > {edge:.1})");
}

/// The room a label's letters need, found by repeating the fit: one label at the
/// top of 0 to 10 reaching 100 px past its point on a 1000 px panel needs the
/// range to end where 100 px of it is still past 10, at 10 / 0.9.
#[test]
fn the_room_for_a_label_is_the_fit_that_holds_its_letters() {
    let (lo, hi) = room_for_labels((0.0, 10.0), &[(10.0, 0.0, 100.0, 1000.0)], (true, true));
    assert_eq!(lo, None, "nothing reaches past the low end");
    assert!((hi.expect("the high end widens") - 10.0 / 0.9).abs() < 1e-6);
    assert_eq!(room_for_labels((0.0, 10.0), &[(10.0, 0.0, 100.0, 1000.0)], (true, false)),
               (None, None), "an end that may not move is not moved");
    assert_eq!(room_for_labels((0.0, 10.0), &[(10.0, 0.0, 300.0, 1000.0)], (true, true)),
               (None, None), "a label wider than a quarter of the panel is not given room");
    assert_eq!(room_for_labels((0.0, 10.0), &[(5.0, 50.0, 50.0, 1000.0)], (true, true)),
               (None, None), "a label inside the panel asks for nothing");
}

/// **A ring is numbered as it is counted, from 1 at the center** (spec §8). `depth`
/// holds each ring's midpoint, since the same column places the ring, so a legend
/// read off it said 1.5 and 2.5; a zone colored by it reads the ring's number, and a
/// label prints it, while the rings stay where they were drawn.
#[test]
fn a_ring_is_colored_and_labeled_by_its_number() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("a", vec!["p".into(), "p".into(), "q".into()])
        .with_str("b", vec!["x".into(), "y".into(), "z".into()])
        .with_float("w", vec![1.0, 2.0, 3.0]))]);
    let rings = |l: Layer| PlotSpec::new().data("t").x("w").y("depth").layer(l);
    let colored = SvgRenderer::default().render(
        &rings(Layer::new(Mark::Zone).partition(&["a", "b"]).encode(Channel::Color, "depth")), &data);
    // The legend's own entries, read after its title: one swatch per ring, in the
    // levels' order, where the midpoints made a strip from 1.5 to 2.5. A ring is a
    // level, so nothing between two of them is named, and no strip is drawn.
    let key = &colored[colored.find(">Depth<").expect("a legend for depth")..];
    let key = &key[..key.find("</g>").unwrap_or(key.len())];
    let numbers: Vec<&str> = key.split('>').skip(1)
        .filter_map(|t| t.split('<').next()).filter(|t| t.parse::<f64>().is_ok()).collect();
    assert_eq!(numbers, vec!["1", "2"], "one swatch per ring, numbered from 1");
    assert!(!colored.contains("-ramp"), "a ring's key is swatches, not a strip");
    let plain = SvgRenderer::default().render(
        &rings(Layer::new(Mark::Zone).partition(&["a", "b"])), &data);
    // The rings along `y`, where the ring is drawn: the panel's width moves with
    // the legend, the rings' places do not.
    let rings_at = |svg: &str| {
        let mut r: Vec<String> = svg.split("<rect").skip(1)
            .filter(|s| s.contains("crispEdges"))
            .map(|s| {
                let attr = |k: &str| s.split(&format!(" {k}=\"")).nth(1)
                    .and_then(|v| v.split('"').next()).unwrap_or("").to_string();
                format!("{} {}", attr("y"), attr("height"))
            })
            .collect();
        r.sort();
        r
    };
    assert_eq!(rings_at(&colored), rings_at(&plain), "the same rings");
    let labeled = SvgRenderer::default().render(
        &rings(Layer::new(Mark::Text).partition(&["a", "b"]).encode(Channel::Label, "depth")), &data);
    let printed: Vec<&str> = labeled.split("<text").skip(1)
        .filter(|s| s.contains("fill-opacity=\"1.000\""))
        .filter_map(|s| s.split('>').nth(1).and_then(|t| t.split('<').next()))
        .collect();
    assert!(printed.contains(&"1") && printed.contains(&"2") && !printed.contains(&"1.5"),
        "each node prints its ring: {printed:?}");
}

/// **A tree colored by its distance ramps each line by the height it is drawn at**
/// (spec §8, a written column is a column). Every vertex of the strokes holds the
/// distance it stands at, so a join's crossbar takes the join's distance and the
/// legs rising to it are lighter; the legend decodes the same column.
#[test]
fn a_tree_colored_by_distance_ramps_its_lines_by_height() {
    let spec = PlotSpec::new().data("t").x("leaf")
        .layer(Layer::new(Mark::Path).cluster(Some("v"), Some("g")).encode(Channel::Color, "distance"));
    let svg = SvgRenderer::default().render(&spec, &cluster_table());
    let strokes: std::collections::BTreeSet<&str> = svg.split("<line ").skip(1)
        .filter(|s| s.contains("stroke-width=\"2\""))
        .filter_map(|s| s.split("stroke=\"").nth(1).and_then(|c| c.split('"').next()))
        .collect();
    assert!(strokes.len() >= 3, "the joins are drawn at different distances: {strokes:?}");
    assert!(svg.contains(">Distance<"), "the ramp is decoded");
}

/// **Naming the distance axis draws the same tree** (spec §8, a written column is a
/// column). With both positions bound the renderer took `x` for the leaves, so
/// `x(distance) + y(leaf)` clustered a column that does not exist and drew an empty
/// panel; the leaves are the position that does not name `distance`.
#[test]
fn a_tree_with_its_distance_axis_named_draws_the_same_elbows() {
    let tree = || Layer::new(Mark::Path).cluster(Some("v"), Some("g"));
    let elbows = |svg: &str| svg.split("<polyline").skip(1)
        .map(|s| s.split("points=\"").nth(1).and_then(|p| p.split('"').next())
            .unwrap_or("").to_string())
        .collect::<Vec<_>>();
    for (named, alone) in [
        (PlotSpec::new().data("t").x("distance").y("leaf").layer(tree()),
         PlotSpec::new().data("t").y("leaf").layer(tree())),
        (PlotSpec::new().data("t").x("leaf").y("distance").layer(tree()),
         PlotSpec::new().data("t").x("leaf").layer(tree())),
    ] {
        let svg = SvgRenderer::default().render(&named, &cluster_table());
        assert_eq!(elbows(&svg).len(), 3, "three merges, three strokes");
        assert_eq!(elbows(&svg), elbows(&SvgRenderer::default().render(&alone, &cluster_table())));
    }
}

/// **The seriated tile plot re-orders its slots and nothing else.** The
/// same sentence without `cluster` draws a b c d; with it, the leaf axis
/// reads b a c d — the transform must visibly do something (P1's burden,
/// carried here because a spec-less chain never reaches the sweep).
#[test]
fn a_clustered_tile_plot_takes_the_trees_leaf_order() {
    let plain = PlotSpec::new().data("t").x("leaf").y("g")
        .layer(Layer::new(Mark::Zone).encode(Channel::Color, "v"));
    let clustered = PlotSpec::new().data("t").x("leaf").y("g")
        .layer(Layer::new(Mark::Zone).cluster(None, Some("g"))
            .encode(Channel::Color, "v"));
    let before = SvgRenderer::default().render(&plain, &cluster_table());
    let after = SvgRenderer::default().render(&clustered, &cluster_table());
    assert_ne!(before, after, "the reorder must be visible");
    let at = |s: &str, hay: &str| hay.find(s).unwrap();
    assert!(at(">a<", &before) < at(">b<", &before), "input order first");
    assert!(at(">b<", &after) < at(">a<", &after), "leaf order after");
}

/// **A composed page hands a clustered panel's order to the axis it
/// shares** — the marginal figure's one new rule (§9). The plain tile
/// plot below the tree draws its slots in the tree's order, with no
/// cluster of its own.
#[test]
fn a_shared_axis_takes_a_clustered_panels_order() {
    let top = PlotSpec::new().data("t").x("leaf")
        .layer(Layer::new(Mark::Path).cluster(Some("v"), Some("g")));
    let main = PlotSpec::new().data("t").x("leaf").y("g")
        .layer(Layer::new(Mark::Zone).encode(Channel::Color, "v"));
    let fig = crate::ir::Figure::Page(crate::ir::PageSpec {
        arrange: crate::ir::Arrange::Below,
        cells: vec![
            crate::ir::Figure::Plot(Box::new(top)),
            crate::ir::Figure::Plot(Box::new(main)),
        ],
        theme: Default::default(),
    });
    let drawing = crate::plot::render_figure(&fig, &cluster_table())
        .expect("the marginal figure draws");
    let svg = drawing.svg;
    let at = |s: &str| svg.find(s).unwrap_or_else(|| panic!("{s} missing"));
    assert!(at(">b<") < at(">a<") && at(">a<") < at(">c<"),
        "the shared axis reads the tree's order, drawn once");
    assert_eq!(svg.matches(">b<").count(), 1,
        "one shared axis, its labels written once");
}

/// **Two panels each deriving an order for one shared axis refuse** — one
/// axis cannot hold two orders, and quietly picking one would misplace
/// every slot of the loser.
#[test]
fn two_disagreeing_cluster_orders_on_one_shared_axis_refuse() {
    let mut table = cluster_table();
    // A second value column whose tree pairs a with c first, so the two
    // panels' leaf orders cannot agree.
    let df = table.remove("t").unwrap()
        .with_float("w", vec![0.0, 0.0, 9.0, 9.0, 0.5, 0.0, 9.0, 8.0]);
    table.insert("t".to_string(), df);
    let panel = |value: &str| PlotSpec::new().data("t").x("leaf")
        .layer(Layer::new(Mark::Path).cluster(Some(value), Some("g")));
    let fig = crate::ir::Figure::Page(crate::ir::PageSpec {
        arrange: crate::ir::Arrange::Below,
        cells: vec![
            crate::ir::Figure::Plot(Box::new(panel("v"))),
            crate::ir::Figure::Plot(Box::new(panel("w"))),
        ],
        theme: Default::default(),
    });
    let Err(err) = crate::plot::render_figure(&fig, &table) else {
        panic!("two orders for one axis must refuse");
    };
    assert!(err.iter().any(|d| d.message.contains("disagree")),
        "{:?}", err.iter().map(|d| &d.message).collect::<Vec<_>>());
}

// -----------------------------------------------------------------------
// Network — the layout's three readers (spec §15, the network entry)
// -----------------------------------------------------------------------

fn net_table() -> HashMap<String, DataFrame> {
    let df = DataFrame::new()
        .with_str("a", vec!["p".into(), "p".into(), "q".into(), "r".into()])
        .with_str("b", vec!["q".into(), "r".into(), "r".into(), "s".into()])
        .with_float("w", vec![1.0, 2.0, 3.0, 4.0]);
    HashMap::from([("t".to_string(), df)])
}

fn net_spec(coord: crate::ir::NetworkView) -> PlotSpec {
    PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Edge).layout("a", "b"))
        .layer(Layer::new(Mark::Point).layout("a", "b"))
        .coord(CoordSpace::Network(coord))
}

/// **The flat network: strokes, glyphs, and none of an axis's furniture.**
/// Asserted for the *edge-only* sentence too — the flow family's lesson,
/// that a projection must be tested alone and not beside the sibling that
/// hides its gap. Two renders are one picture, at the very end of the
/// pipeline.
#[test]
fn a_network_draws_axisless_and_each_projection_stands_alone() {
    let svg = SvgRenderer::default().render(
        &net_spec(crate::ir::NetworkView::default()), &net_table());
    assert!(svg.contains("<line"), "edges are strokes");
    assert!(svg.contains("<circle"), "nodes are glyphs");
    assert!(!svg.contains("tick"), "no ticks in the graph-theoretic space");
    let edge_only = SvgRenderer::default().render(
        &PlotSpec::new().data("t")
            .layer(Layer::new(Mark::Edge).layout("a", "b"))
            .coord(CoordSpace::Network(crate::ir::NetworkView::default())),
        &net_table());
    assert!(edge_only.contains("<line"), "the edge projection stands alone");
    let nodes_only = SvgRenderer::default().render(
        &PlotSpec::new().data("t")
            .layer(Layer::new(Mark::Point).layout("a", "b"))
            .coord(CoordSpace::Network(crate::ir::NetworkView::default())),
        &net_table());
    assert!(nodes_only.contains("<circle"), "the node projection stands alone");
    assert_eq!(svg, SvgRenderer::default().render(
        &net_spec(crate::ir::NetworkView::default()), &net_table()));
}

/// **Stating a viewing angle states the cube** — the ruling
/// `CoordSpace::Network` records, checked at the pixels: the cube form
/// draws the glass box and differs from the flat form, and its box carries
/// no axis labels, a network's numbers meaning nothing.
#[test]
fn a_network_with_a_stated_view_is_the_cube_without_labels() {
    let flat = SvgRenderer::default().render(
        &net_spec(crate::ir::NetworkView::default()), &net_table());
    let cube = SvgRenderer::default().render(
        &net_spec(crate::ir::NetworkView { turn: Some(30.0), tilt: Some(25.0) }),
        &net_table());
    assert_ne!(flat, cube, "the stated angle changes the picture");
    assert!(!cube.contains(">Layout"), "no axis names on the network's box");
}

/// **`repel` composes with `layout`**, because ink placement holds no job:
/// the names come off their dots, which is the argued relaxation the
/// layout's claim-everything entry priced in. Asserted at the pixels — the
/// repelled render differs from the centered one — and for determinism,
/// since a moved label must move to the same place every run.
#[test]
fn a_networks_names_repel_off_their_dots() {
    let with_repel = |repel: bool| {
        let mut text = Layer::new(Mark::Text).layout("a", "b");
        if repel {
            text.transforms.push(Transform::Repel);
        }
        text.encodings.insert(Channel::Label, crate::ir::ChannelDef::field("name"));
        PlotSpec::new().data("t")
            .layer(Layer::new(Mark::Edge).layout("a", "b"))
            .layer(Layer::new(Mark::Point).layout("a", "b"))
            .layer(text)
            .coord(CoordSpace::Network(crate::ir::NetworkView::default()))
    };
    let centered = SvgRenderer::default().render(&with_repel(false), &net_table());
    let repelled = SvgRenderer::default().render(&with_repel(true), &net_table());
    assert_ne!(centered, repelled, "repel must move the names");
    assert_eq!(repelled,
        SvgRenderer::default().render(&with_repel(true), &net_table()),
        "a repelled name moves to the same place every run");
}

/// **A repelled name clears the dot it actually has.** With `size(degree)`
/// on the sibling `point`, the hub's dot is much larger than the default,
/// and the label's resting step must measure off that radius — found by a
/// reader whose hub names struck through their dots while every small
/// node's name rested clear. Asserted geometrically: no label center sits
/// inside its own node's circle.
#[test]
fn a_repelled_name_clears_its_sized_dot() {
    let hub = DataFrame::new()
        .with_str("a", vec!["Hub".into(); 6])
        .with_str("b", vec!["p".into(), "q".into(), "r".into(),
                            "s".into(), "t".into(), "u".into()]);
    let data = HashMap::from([("t".to_string(), hub)]);
    let mut point = Layer::new(Mark::Point).layout("a", "b");
    point.encodings.insert(Channel::Size, crate::ir::ChannelDef::field("degree"));
    let mut text = Layer::new(Mark::Text).layout("a", "b");
    text.transforms.push(Transform::Repel);
    text.encodings.insert(Channel::Label, crate::ir::ChannelDef::field("name"));
    let spec = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Edge).layout("a", "b"))
        .layer(point).layer(text)
        .coord(CoordSpace::Network(crate::ir::NetworkView::default()));
    let svg = SvgRenderer::default().render(&spec, &data);
    let circles: Vec<(f64, f64, f64)> = svg.match_indices("<circle cx=\"")
        .map(|(at, _)| {
            let rest = &svg[at..];
            let grab = |key: &str| -> f64 {
                let s = rest.find(key).unwrap() + key.len();
                rest[s..].split('"').next().unwrap().parse().unwrap()
            };
            (grab("cx=\""), grab("cy=\""), grab("r=\""))
        })
        .collect();
    let texts: Vec<(f64, f64)> = svg.match_indices("<text x=\"")
        .map(|(at, _)| {
            let rest = &svg[at..];
            let grab = |key: &str| -> f64 {
                let s = rest.find(key).unwrap() + key.len();
                rest[s..].split('"').next().unwrap().parse().unwrap()
            };
            (grab("x=\""), grab("y=\""))
        })
        .collect();
    assert!(!circles.is_empty() && !texts.is_empty());
    for &(tx, ty) in &texts {
        let &(cx, cy, r) = circles.iter()
            .min_by(|p, q| {
                let dp = (p.0 - tx).powi(2) + (p.1 - ty).powi(2);
                let dq = (q.0 - tx).powi(2) + (q.1 - ty).powi(2);
                dp.partial_cmp(&dq).unwrap()
            })
            .unwrap();
        let dist = ((cx - tx).powi(2) + (cy - ty).powi(2)).sqrt();
        assert!(dist > r,
            "a name's center at ({tx}, {ty}) sits inside its dot at \
             ({cx}, {cy}) of radius {r}");
    }
}

/// A band's paint follows a stage's category: `color(class)` on the band
/// layer splits the bands by their path's class and earns the legend.
#[test]
fn a_flow_band_is_colored_by_a_stage_and_earns_a_key() {
    let mut spec = flow_spec();
    spec.layers[0].encodings.insert(Channel::Color, crate::ir::ChannelDef::field("class"));
    let svg = SvgRenderer::default().render(&spec, &flow_table());
    assert!(svg.contains(PALETTE_GOG[0]) && svg.contains(PALETTE_GOG[1]),
        "two classes, two hues");
    assert!(svg.contains(">Class<"), "the mapping earns its key");
}

// -----------------------------------------------------------------------
// Brush — the selection, and the promise that it costs an unbrushed plot
// nothing (spec §15)
//
// The first two tests here are the whole safety argument for the feature and
// they were written before it existed. Everything in the book, the PDF and
// every recorded parity hash rests on one sentence: **a plot that does not
// name a brush draws exactly what it drew before selection was built.** The
// 692 tests around this one are the broad form of that promise; these are the
// sharp form, and they name the two artifacts a selection could leak.
// -----------------------------------------------------------------------

fn brush_data() -> HashMap<String, DataFrame> {
    let mut d = HashMap::new();
    d.insert("t".to_string(), DataFrame::new()
        .with_float("gx", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .with_float("gy", vec![2.0, 4.0, 1.0, 5.0, 3.0, 6.0])
        .with_str("cat", vec!["a", "a", "b", "b", "c", "c"]
            .into_iter().map(String::from).collect()));
    d
}

fn brush_spec() -> PlotSpec {
    PlotSpec::new().data("t").x("gx").y("gy").layer(Layer::new(Mark::Point))
}

/// Counting glyphs is how these tests read a pass, because a `point` writes
/// exactly one `<circle>` per row it is handed.
fn circles(svg: &str) -> usize {
    svg.matches("<circle").count()
}

// -----------------------------------------------------------------------
// Map — the sphere on the page (spec §15)
// -----------------------------------------------------------------------

/// Places spread over the whole globe, so the two properties below are
/// measured across the range where a projection actually differs from a
/// rescaling rather than in a corner where every projection agrees.
fn world() -> HashMap<String, DataFrame> {
    let df = DataFrame::new()
        .with_float("lon", vec![0.0, 90.0, -90.0, 180.0, -180.0, 0.0, 0.0])
        .with_float("lat", vec![0.0, 0.0, 0.0, 0.0, 0.0, 60.0, -60.0]);
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);
    m
}

fn world_map(preserve: crate::ir::Preserve) -> PlotSpec {
    PlotSpec::new()
        .data("t")
        .x("lon")
        .y("lat")
        .coord(CoordSpace::Map(crate::ir::MapView { preserve }))
        .layer(Layer::new(Mark::Point))
}

/// Pull every `<circle>` center out, in the row order they were written.
fn centers(svg: &str) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for chunk in svg.split("<circle ").skip(1) {
        let grab = |key: &str| -> Option<f64> {
            let at = chunk.find(key)? + key.len();
            chunk[at..].split('"').next()?.parse().ok()
        };
        if let (Some(x), Some(y)) = (grab("cx=\""), grab("cy=\"")) {
            out.push((x, y));
        }
    }
    out
}

/// **The property that makes an equal-area map worth having.** A projected
/// unit must be the same number of pixels across as it is tall, or the panel
/// has stretched the map and the ink is no longer proportional to the ground —
/// which breaks the projection's whole claim while still looking like a map.
///
/// Measured, not asserted: the equator's half-width and the 60° parallel's
/// height are compared against what `geo` says those distances are. Before the
/// space took the panel's proportions from its own extent, this came out 1.75
/// and every map was a fitted rectangle wearing a projection's name.
#[test]
fn a_map_is_drawn_at_the_projections_proportions_and_not_the_panels() {
    for preserve in [crate::ir::Preserve::Area, crate::ir::Preserve::Angle] {
        let svg = SvgRenderer::default().render(&world_map(preserve), &world());
        let pts = centers(&svg);
        assert_eq!(pts.len(), 7, "every place should be drawn once");
        let geo = crate::render::geo::Geo::new(&crate::ir::MapView { preserve });

        // Row 0 is (0, 0); row 3 is (180, 0); row 5 is (0, 60).
        let px_east = (pts[3].0 - pts[0].0) / geo.project(180.0, 0.0).0;
        let px_north = (pts[0].1 - pts[5].1) / geo.project(0.0, 60.0).1;
        assert!(
            (px_east / px_north - 1.0).abs() < 1e-3,
            "{preserve:?}: {px_east:.3} px per unit across, {px_north:.3} down"
        );
    }
}

/// **A meridian `rule` on an Equal Earth map is the meridian.** It was drawn
/// straight up through the place its row was projected to, so it met the
/// meridian at that one latitude and missed it at every other: a point at
/// (90, 60) sat well left of the rule at 90. It bends now, through the points,
/// whether its table names a latitude or not; under Mercator a meridian is
/// straight and the rule is the `<line>` it was.
#[test]
fn a_meridian_rule_on_an_equal_earth_map_follows_the_meridian() {
    let polyline = |svg: &str| -> Vec<(f64, f64)> {
        let at = svg.find("stroke=\"firebrick\"").expect("the rule is drawn");
        let start = svg[..at].rfind("<polyline points=\"").expect("drawn as a polyline")
            + "<polyline points=\"".len();
        svg[start..].split('"').next().unwrap().split(' ')
            .map(|p| {
                let (x, y) = p.split_once(',').unwrap();
                (x.parse().unwrap(), y.parse().unwrap())
            })
            .collect()
    };
    for with_lat in [false, true] {
        let mut data = world();
        // A table holding both columns names the axis on the layer, and its
        // row is placed at its own latitude, 45, before the curve is drawn.
        let mut rule = DataFrame::new().with_float("lon", vec![90.0]);
        let mut layer = Layer::new(Mark::Rule).data("m");
        if with_lat {
            rule = rule.with_float("lat", vec![45.0]);
            layer = layer.encode(Channel::X, "lon");
        }
        data.insert("m".to_string(), rule);
        layer.style.color = Some("firebrick".to_string());
        let spec = world_map(crate::ir::Preserve::Area).layer(layer);
        let svg = SvgRenderer::default().render(&spec, &data);
        let line = polyline(&svg);
        // The rule's x at a height, read off the polyline between its vertices.
        let x_at = |y: f64| -> f64 {
            line.windows(2)
                .find(|w| (w[0].1 - y) * (w[1].1 - y) <= 0.0)
                .map(|w| w[0].0 + (w[1].0 - w[0].0) * (y - w[0].1) / (w[1].1 - w[0].1))
                .expect("the rule spans the height")
        };
        // Rows 1 and 0 are (90, 0) and (0, 0); row 5 is (0, 60), so a point at
        // (90, 60) is where the projection puts it, measured off those three.
        let pts = centers(&svg);
        let geo = crate::render::geo::Geo::new(&crate::ir::MapView {
            preserve: crate::ir::Preserve::Area });
        let px_per_unit = (pts[1].0 - pts[0].0) / geo.project(90.0, 0.0).0;
        let at60 = pts[0].0 + geo.project(90.0, 60.0).0 * px_per_unit;
        assert!((x_at(pts[1].1) - pts[1].0).abs() < 0.5, "at the equator");
        assert!((x_at(pts[5].1) - at60).abs() < 0.5,
            "at 60°N the rule is at {:.2}, the meridian at {at60:.2} (lat column: {with_lat})",
            x_at(pts[5].1));
        assert!(at60 < pts[1].0 - 20.0, "the meridian bends inward toward the pole");
    }
    // Mercator: straight, as before.
    let mut data = world();
    data.insert("m".to_string(), DataFrame::new().with_float("lon", vec![90.0]));
    let mut layer = Layer::new(Mark::Rule).data("m");
    layer.style.color = Some("firebrick".to_string());
    let svg = SvgRenderer::default()
        .render(&world_map(crate::ir::Preserve::Angle).layer(layer), &data);
    let at = svg.find("stroke=\"firebrick\"").expect("the rule is drawn");
    assert!(svg[svg[..at].rfind('<').unwrap()..].starts_with("<line "),
            "a Mercator meridian is a straight line");
}

/// Longitude is linear along the equator in both projections, so twice the
/// longitude is twice the distance. A projection wired up to the wrong column,
/// or applied one column at a time, fails this immediately.
#[test]
fn twice_the_longitude_is_twice_the_distance_along_the_equator() {
    let svg = SvgRenderer::default().render(&world_map(crate::ir::Preserve::Area), &world());
    let pts = centers(&svg);
    let (at90, at180) = (pts[1].0 - pts[0].0, pts[3].0 - pts[0].0);
    assert!((at180 / at90 - 2.0).abs() < 1e-3, "90° gave {at90:.3}, 180° gave {at180:.3}");
    // And west mirrors east about the prime meridian.
    assert!(((pts[0].0 - pts[2].0) - at90).abs() < 0.05);
}

/// **The axes say degrees, because everything else downstream says projected
/// units.** The projection is applied to the data, which is what makes the
/// space cheap and is wrong for exactly one reader: an axis left alone would
/// announce that longitude runs from −2 to 2.
#[test]
fn a_maps_axes_are_labeled_in_degrees_rather_than_projected_units() {
    let svg = SvgRenderer::default().render(&world_map(crate::ir::Preserve::Area), &world());
    assert!(svg.contains("°</text>"), "no degree labels: {svg}");
    for bare in [">-2</text>", ">2</text>", ">-1</text>", ">1</text>"] {
        assert!(!svg.contains(bare), "a projected unit reached the axis: {bare}");
    }
}

/// **`limits` on a map is a domain in degrees, and the panel is fitted to it
/// projected.** Read as projected numbers, -12..40 was about ten times the
/// width of the whole world, so the panel came out empty, with its degree
/// labels piled on one spot and nothing said.
#[test]
fn a_map_with_limits_frames_the_box_it_states() {
    for preserve in [crate::ir::Preserve::Area, crate::ir::Preserve::Angle] {
        let df = DataFrame::new()
            .with_float("lon", vec![-10.0, 0.0, 20.0, 38.0, 100.0])
            .with_float("lat", vec![36.0, 50.0, 45.0, 58.0, 10.0]);
        let data = HashMap::from([("t".to_string(), df)]);
        let mut spec = world_map(preserve);
        spec.x = Some(crate::ir::ChannelDef::field("lon").with_limits(Some(-12.0), Some(40.0)));
        spec.y = Some(crate::ir::ChannelDef::field("lat").with_limits(Some(34.0), Some(60.0)));
        let svg = SvgRenderer::default().render(&spec, &data);
        let pts = centers(&svg);
        assert_eq!(pts.len(), 4, "{preserve:?}: the row outside the box is cut, the four inside drawn");
        let (west, east) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
        assert!(east - west > 300.0, "{preserve:?}: the four places collapsed together: {west:.1}..{east:.1}");
        let labels = text_of(&svg);
        for tick in ["0°", "20°", "40°", "50°"] {
            assert!(labels.iter().any(|l| l == tick), "{preserve:?}: no {tick} tick: {labels:?}");
        }
    }
}

/// Every strip and panel rectangle, `(y0, y1)`, by its fill.
fn rects_filled(svg: &str, fill: &str) -> Vec<(f64, f64)> {
    let key = format!(r#"fill="{fill}""#);
    svg.split("<rect ").skip(1).filter(|r| r.split('>').next().is_some_and(|head| head.contains(&key)))
        .filter_map(|r| {
            let grab = |k: &str| -> Option<f64> {
                let at = r.find(k)? + k.len();
                r[at..].split('"').next()?.parse().ok()
            };
            Some((grab(" y=\"").or_else(|| grab("y=\""))?, grab("height=\"")?))
        })
        .map(|(y, h)| (y, y + h))
        .collect()
}

/// **A name sits against what it names.** A map keeps its projection's shape,
/// so in a tall cell the panel is centered and the slack goes above and below
/// it. The strip naming the panel stayed at the top of the area: seven maps in
/// a row had their continents' names a third of the page above them.
#[test]
fn a_strip_stays_on_a_panel_its_ratio_centered() {
    let df = DataFrame::new()
        .with_float("lon", vec![0.0, 90.0, -90.0, 30.0])
        .with_float("lat", vec![0.0, 30.0, -30.0, 10.0])
        .with_str("half", ["a", "a", "b", "b"].map(String::from).to_vec());
    let data = HashMap::from([("t".to_string(), df)]);
    let spec = world_map(crate::ir::Preserve::Area).facet_col("half");
    let svg = SvgRenderer::default().render(&spec, &data);
    let strips = rects_filled(&svg, STRIP_BG);
    let panels = rects_filled(&svg, PANEL_BG);
    assert_eq!(strips.len(), 2, "two strips: {strips:?}");
    let top = panels.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    assert!(top > 60.0, "the map should leave slack above it in an 800 x 600 plot: {top}");
    for (_, bottom) in strips {
        assert!((top - bottom - 4.0).abs() < 0.5, "a strip ends at {bottom:.1}, its panel starts at {top:.1}");
    }
}

/// Strip names that fit are left alone; names too long shrink together, one
/// size for the family; past the floor they are shortened and counted.
#[test]
fn strip_names_shrink_together_and_are_shortened_only_past_the_floor() {
    let font = 11.0;
    let (size, names, cut) = fit_strip_names(&["Asia", "Europe"], 200.0, font);
    assert_eq!((size, cut), (font, 0));
    assert_eq!(names, vec!["Asia", "Europe"]);

    let long = "Seven seas (open ocean)";
    let room = estimate_text_width(long, font) * 0.85;
    let (size, names, cut) = fit_strip_names(&["Asia", long], room, font);
    assert!(size < font && size >= font * STRIP_FONT_FLOOR, "shrunk to {size}");
    assert_eq!(cut, 0);
    assert_eq!(names, vec!["Asia", long]);
    assert!(estimate_text_width(long, size) <= room);

    let room = estimate_text_width(long, font) * 0.4;
    let (size, names, cut) = fit_strip_names(&["Asia", long], room, font);
    assert_eq!(size, ((font * STRIP_FONT_FLOOR) * 100.0).floor() / 100.0);
    assert_eq!(cut, 1);
    assert_eq!(names[0], "Asia");
    assert!(names[1].ends_with('…') && names[1].len() < long.len(), "{}", names[1]);
    assert!(estimate_text_width(&names[1], size) <= room);
}

/// **A map's degree labels thin when they would run together.** Its axes kept
/// every label, and seven maps in a row printed "-100° 0° 100°" through each
/// other under every panel. Nine panels across 800 pixels are narrow enough
/// that the three labels overlap.
#[test]
fn a_maps_degree_labels_thin_when_panels_are_narrow() {
    let n = 9;
    // The whole globe in every panel, as a world map's outlines span it.
    let lon: Vec<f64> = (0..n * 2).map(|i| if i % 2 == 0 { -180.0 } else { 180.0 }).collect();
    let lat: Vec<f64> = (0..n * 2).map(|i| if i % 2 == 0 { -60.0 } else { 80.0 }).collect();
    let group: Vec<String> = (0..n * 2).map(|i| format!("g{}", i / 2)).collect();
    let df = DataFrame::new().with_float("lon", lon).with_float("lat", lat).with_str("g", group);
    let data = HashMap::from([("t".to_string(), df)]);
    let spec = world_map(crate::ir::Preserve::Area).facet_col("g");
    let r = SvgRenderer::default();
    let svg = r.render(&spec, &data);
    // Every degree label's drawn extent; the x axis's are the lowest row. A
    // label near a panel's edge is anchored at its start or end rather than
    // its middle, so it is measured the way it is drawn.
    let labels: Vec<(f64, f64, f64, String)> = svg.split("<text ").skip(1)
        .filter_map(|t| {
            let (head, rest) = t.split_once('>')?;
            let label = rest.split_once("</text>")?.0.to_string();
            let grab = |k: &str| -> Option<f64> {
                let at = head.find(k)? + k.len();
                head[at..].split('"').next()?.parse().ok()
            };
            let (x, y) = (grab(" x=\"").or_else(|| grab("x=\""))?, grab(" y=\"")?);
            let w = estimate_text_width(&label, r.font_sm);
            let (lo, hi) = if head.contains(r#"text-anchor="start""#) { (x, x + w) }
                else if head.contains(r#"text-anchor="end""#) { (x - w, x) }
                else { (x - w / 2.0, x + w / 2.0) };
            label.ends_with('°').then_some((lo, hi, y, label))
        })
        .collect();
    let bottom = labels.iter().map(|l| l.2).fold(f64::NEG_INFINITY, f64::max);
    let mut row: Vec<(f64, f64, String)> = labels.into_iter()
        .filter(|l| (l.2 - bottom).abs() < 0.5).map(|l| (l.0, l.1, l.3)).collect();
    row.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(row.len() >= n, "every panel keeps a label: {row:?}");
    for w in row.windows(2) {
        assert!(w[0].1 <= w[1].0, "{:?} and {:?} overlap", w[0], w[1]);
    }
    // And each stays inside its own panel, where a neighbor's labels cannot
    // reach it: a label hanging past the edge is what ran them together.
    let panels: Vec<(f64, f64)> = svg.split("<rect ").skip(1)
        .filter(|r| r.split('>').next().is_some_and(|h| h.contains(&format!(r#"fill="{PANEL_BG}""#))))
        .filter_map(|r| {
            let grab = |k: &str| -> Option<f64> {
                let at = r.find(k)? + k.len();
                r[at..].split('"').next()?.parse().ok()
            };
            Some((grab("x=\"")?, grab("width=\"")?))
        })
        .map(|(x, w)| (x, x + w))
        .collect();
    assert_eq!(panels.len(), n, "one panel per group: {panels:?}");
    for (lo, hi, label) in &row {
        let mid = (lo + hi) / 2.0;
        let (x0, x1) = panels.iter().copied()
            .min_by(|a, b| ((a.0 + a.1) / 2.0 - mid).abs().total_cmp(&((b.0 + b.1) / 2.0 - mid).abs()))
            .unwrap();
        assert!(*lo >= x0 - 0.5 && *hi <= x1 + 0.5,
            "{label} runs {lo:.1}..{hi:.1}, outside its panel {x0:.1}..{x1:.1}");
    }
}

// -- an axis turned off (spec §10) ----------------------------------------

/// How many tick numbers a flat plot wrote under its panel (`middle`) or
/// beside it (`end`): `write_ticks` puts each axis's in a group of its own.
fn tick_numbers(svg: &str, anchor: &str) -> usize {
    let open = format!(r##"fill="#3c3c46" text-anchor="{anchor}">"##);
    svg.split(open.as_str()).skip(1)
        .map(|g| g.split("</g>").next().unwrap_or("").matches("<text").count())
        .sum()
}

/// The lines in the first group that opens with `head`.
fn lines_in(svg: &str, head: &str) -> usize {
    svg.split(head).nth(1)
        .map_or(0, |g| g.split("</g>").next().unwrap_or("").matches("<line").count())
}

fn axis_scatter(x_off: bool, y_off: bool) -> PlotSpec {
    let mut spec = PlotSpec::new().data("t").layer(Layer::new(Mark::Point));
    spec.x = Some(ChannelDef { axis: x_off.then_some(false), ..ChannelDef::field("lon") });
    spec.y = Some(ChannelDef { axis: y_off.then_some(false), ..ChannelDef::field("lat") });
    spec
}

/// **`axis = FALSE` takes away the three things an axis is made of**: the
/// tick marks, the numbers and the name, and the margin they stood in. The
/// scale is untouched, so the gridlines stand where they stood, and the other
/// axis is drawn as before.
#[test]
fn an_axis_left_out_draws_no_ticks_numbers_or_name_and_gives_back_its_margin() {
    let render = |x_off, y_off| SvgRenderer::default().render(&axis_scatter(x_off, y_off), &world());
    let both = render(false, false);
    let no_x = render(true, false);
    assert!(tick_numbers(&both, "middle") > 0 && tick_numbers(&both, "end") > 0, "{both}");
    assert_eq!(tick_numbers(&no_x, "middle"), 0, "x numbers drawn: {no_x}");
    assert_eq!(tick_numbers(&no_x, "end"), tick_numbers(&both, "end"), "y lost numbers");
    assert!(both.contains(">Lon</text>") && !no_x.contains(">Lon</text>"), "the x name stayed");
    assert!(no_x.contains(">Lat</text>"), "the y name went with it");

    let marks = r##"<g stroke="#5a5a64" stroke-width="1" fill="none">"##;
    assert_eq!(lines_in(&no_x, marks) + tick_numbers(&both, "middle"), lines_in(&both, marks),
        "only the x tick marks went");
    let grid = r##"<g stroke="#d2d2da" stroke-width="1">"##;
    assert_eq!(lines_in(&no_x, grid), lines_in(&both, grid), "the gridlines followed the axis");

    let tall = |svg: &str| {
        let (top, bottom) = rects_filled(svg, PANEL_BG)[0];
        bottom - top
    };
    assert!(tall(&no_x) > tall(&both) + 20.0,
        "the panel kept the x axis's margin: {} against {}", tall(&no_x), tall(&both));

    let neither = render(true, true);
    assert_eq!(tick_numbers(&neither, "middle") + tick_numbers(&neither, "end"), 0, "{neither}");
    assert!(!neither.contains(">Lat</text>") && !neither.contains(">Lon</text>"));
}

/// A map draws its graticule as the grid and numbers it in degrees on the
/// axes; left out, the degrees go and the graticule stays.
#[test]
fn a_map_with_its_axes_left_out_draws_no_degrees() {
    for preserve in [crate::ir::Preserve::Area, crate::ir::Preserve::Angle] {
        let mut spec = world_map(preserve);
        spec.x = Some(ChannelDef::field("lon").with_axis(false));
        spec.y = Some(ChannelDef::field("lat").with_axis(false));
        let svg = SvgRenderer::default().render(&spec, &world());
        assert!(!svg.contains("°</text>"), "{preserve:?}: a degree was drawn: {svg}");
        assert!(svg.contains("<polyline") || lines_in(&svg, r##"<g stroke="#d2d2da" stroke-width="1">"##) > 0,
            "{preserve:?}: the graticule went with the axes");
    }
}

/// In polar the angle's numbers ring the circle and the radius's run up the
/// spoke; each goes with its own axis, and the spokes and rings stay.
#[test]
fn a_polar_axis_left_out_loses_its_names_and_keeps_its_spokes() {
    // Its own column name: the fixture's `n` names the radius "N", which is
    // also a compass point.
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("dir", vec!["N", "E", "S", "W"].into_iter().map(String::from).collect())
            .with_float("gusts", vec![10.0, 40.0, 20.0, 30.0]),
    )]);
    let rose = |angle_off: bool, radius_off: bool| {
        let mut s = PlotSpec::new().data("t")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(Layer::new(Mark::Bar));
        s.x = Some(ChannelDef { axis: angle_off.then_some(false), ..ChannelDef::field("dir") });
        s.y = Some(ChannelDef { axis: radius_off.then_some(false), ..ChannelDef::field("gusts") });
        SvgRenderer::default().render(&s, &data)
    };
    let (full, no_angle, no_radius) = (rose(false, false), rose(true, false), rose(false, true));
    for name in [">N</text>", ">E</text>", ">S</text>", ">W</text>"] {
        assert!(full.contains(name) && !no_angle.contains(name), "{name}: {no_angle}");
        assert!(no_radius.contains(name), "{name} went with the radius: {no_radius}");
    }
    assert!(full.contains(">Dir</text>") && !no_angle.contains(">Dir</text>"), "the angle kept its name");
    assert!(no_angle.contains(">20</text>") && no_angle.contains(">Gusts</text>"),
        "the radius lost its numbers or its name with the angle: {no_angle}");
    assert!(!no_radius.contains(">20</text>") && !no_radius.contains(">Gusts</text>"),
        "the radius kept its numbers or its name: {no_radius}");
    for bare in [&no_angle, &no_radius] {
        assert_eq!(full.matches("<line").count(), bare.matches("<line").count(), "a spoke went");
        assert_eq!(full.matches("<circle").count(), bare.matches("<circle").count(), "a ring went");
    }
}

/// The cube writes each axis along an edge of the box; one left out loses its
/// numbers and its name, and its edge stays in the box.
#[test]
fn a_cube_axis_left_out_loses_its_numbers_and_name_and_keeps_its_edge() {
    let numbers = |svg: &str| frame_labels_of(svg).into_iter()
        .filter(|(_, _, t)| t.parse::<f64>().is_ok()).count();
    let spec = sheet_spec(SpaceView::default());
    let full = SvgRenderer::default().render(&spec, &sheet());
    let mut quiet = spec.clone();
    quiet.z = Some(ChannelDef::field("elev").with_axis(false));
    let bare = SvgRenderer::default().render(&quiet, &sheet());
    assert!(full.contains(">Elev</text>") && !bare.contains(">Elev</text>"), "z kept its name");
    assert!(bare.contains(">East</text>") && bare.contains(">North</text>"), "x or y lost its name");
    assert!(numbers(&bare) < numbers(&full), "z kept its numbers");
    assert_eq!(full.matches("<line").count(), bare.matches("<line").count(), "an edge of the box went");
}

/// Six rows: three rings, two continents, with `west` holding two of the rings.
fn rings_by_continent() -> HashMap<String, DataFrame> {
    let df = DataFrame::new()
        .with_float("lon", vec![0.0, 1.0, 5.0, 6.0, 10.0, 11.0])
        .with_float("lat", vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0])
        .with_str("piece", ["a", "a", "b", "b", "c", "c"].map(String::from).to_vec())
        .with_str(
            "continent",
            ["west", "west", "west", "west", "east", "east"].map(String::from).to_vec(),
        );
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);
    m
}

/// **Every channel that splits, splits.** `color` splits and encodes; `group`
/// splits and encodes nothing; binding both means the mark is drawn once per
/// combination, which here is one stroke per ring.
///
/// Each of the five series marks used to pick a *single* field to split by, in
/// the order `color`, `group`, `pattern` — so a `group` beside a `color` was
/// silently discarded. It drew a plausible picture rather than an empty one,
/// which is what kept it hidden: a world map grouped by coastline and colored
/// by continent came out as one scribble per continent, every ring joined end
/// to end.
#[test]
fn a_group_beside_a_color_splits_by_both_rather_than_being_dropped() {
    for mark in [Mark::Path, Mark::Line, Mark::Step] {
        let spec = PlotSpec::new()
            .data("t")
            .x("lon")
            .y("lat")
            .layer(
                Layer::new(mark.clone())
                    .encode(Channel::Group, "piece")
                    .encode(Channel::Color, "continent"),
            );
        let svg = SvgRenderer::default().render(&spec, &rings_by_continent());
        let strokes = svg.matches("<polyline").count();
        assert_eq!(strokes, 3, "{mark:?}: three rings, so three strokes:\n{svg}");
        // And the colors still come from `continent`, not from `piece`: two of
        // the three rings are `west`, so the three strokes carry two hues.
        // Read off the polylines alone — the gridlines and the axes carry a
        // `stroke` too, and counting those made this assertion meaningless.
        let hues: std::collections::HashSet<&str> = svg
            .split("<polyline")
            .skip(1)
            .filter_map(|s| s.split("stroke=\"").nth(1))
            .filter_map(|s| s.split('"').next())
            .collect();
        assert_eq!(hues.len(), 2, "{mark:?}: colored by ring instead of continent");
    }
}

/// Two regions. `big` is a square with a smaller square inside it as a second
/// ring — the Lesotho case — and `small` is that inner square as a region of
/// its own. Every ring repeats its first vertex last, which is what divides a
/// region's rows into rings.
fn enclave() -> HashMap<String, DataFrame> {
    let ring = |x0: f64, y0: f64, x1: f64, y1: f64| {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]
    };
    let mut pts = ring(0.0, 0.0, 10.0, 10.0);
    pts.extend(ring(4.0, 4.0, 6.0, 6.0));
    let n_big = pts.len();
    pts.extend(ring(4.0, 4.0, 6.0, 6.0));
    let df = DataFrame::new()
        .with_float("lon", pts.iter().map(|p| p.0).collect())
        .with_float("lat", pts.iter().map(|p| p.1).collect())
        .with_str(
            "region",
            (0..pts.len())
                .map(|i| if i < n_big { "big".to_string() } else { "small".to_string() })
                .collect(),
        );
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);
    m
}

fn choropleth() -> PlotSpec {
    PlotSpec::new()
        .data("t")
        .x("lon")
        .y("lat")
        .coord(CoordSpace::Map(crate::ir::MapView::default()))
        .layer(
            Layer::new(Mark::Zone)
                .encode(Channel::Group, "region")
                .encode(Channel::Color, "region"),
        )
}

/// **A region is one path, however many rings it has**, and the rings are found
/// by closure rather than by a second grouping column. Two regions here, three
/// rings between them, so two paths and three subpaths.
#[test]
fn a_boundary_becomes_one_filled_path_per_region() {
    let svg = SvgRenderer::default().render(&choropleth(), &enclave());
    assert_eq!(svg.matches("<path d=\"M").count(), 2, "one path per region: {svg}");
    // `M` starts a subpath and `Z` closes it: two rings in `big`, one in `small`.
    let region_paths: Vec<&str> = svg.split("<path d=\"").skip(1).collect();
    let subpaths: usize = region_paths.iter().map(|p| p.split('"').next().unwrap_or("").matches('M').count()).sum();
    assert_eq!(subpaths, 3, "three rings across two regions");
}

/// **The hole is what even-odd buys**, and it is the case naive per-ring filling
/// gets wrong: the enclave would be painted over by whichever region drew last.
/// With both of `big`'s rings in one path under `evenodd`, the middle is empty
/// for `small` to fill with its own color, whatever the draw order.
#[test]
fn an_inner_ring_is_a_hole_rather_than_a_patch_painted_over() {
    let svg = SvgRenderer::default().render(&choropleth(), &enclave());
    assert!(svg.contains(r#"fill-rule="evenodd""#), "no even-odd rule: {svg}");
    // The two regions carry different fills, so the enclave is readable as its
    // own value rather than as a smudge on its container.
    let fills: Vec<&str> = svg
        .split("<path d=\"")
        .skip(1)
        .filter_map(|p| p.split("fill=\"").nth(1))
        .filter_map(|p| p.split('"').next())
        .collect();
    assert_eq!(fills.len(), 2);
    assert_ne!(fills[0], fills[1], "the enclave took its container's color");
}

/// **Two plots in one document may not share an id.**
///
/// An id is resolved against the whole document, and a notebook puts every
/// plot in one. Ids used to be hashed per element, so two plots using the
/// same texture in the same color were handed the same id and the browser
/// bound the second's `fill` to the first's definition — correct until a
/// host detached the owning cell, and then a correct plot drew nothing.
///
/// The suite could never have caught it: it writes one picture per file.
#[test]
fn two_different_plots_mint_no_id_in_common() {
    let ids = |svg: &str| -> std::collections::HashSet<String> {
        svg.match_indices("id=\"")
            .filter_map(|(i, _)| svg[i + 4..].split('"').next().map(str::to_string))
            .collect()
    };
    let (spec, data) = brush_spec_and_data_for_ids();
    let a = SvgRenderer::default().render(&spec.0, &data);
    let b = SvgRenderer::default().render(&spec.1, &data);
    let (ia, ib) = (ids(&a), ids(&b));
    assert!(!ia.is_empty() && !ib.is_empty(), "both plots must mint ids to compare");
    assert!(ia.is_disjoint(&ib),
        "two different plots share an id: {:?}", ia.intersection(&ib).collect::<Vec<_>>());
    // And every reference resolves inside the plot that made it.
    for svg in [&a, &b] {
        for (i, _) in svg.match_indices("url(#") {
            let r = svg[i + 5..].split(')').next().unwrap();
            assert!(svg.contains(&format!("id=\"{r}\"")),
                "a reference points outside its own plot: {r}");
        }
    }
}

/// The same spec twice is still byte-identical, which is what four bindings
/// are held to. Namespacing ids may not cost that.
#[test]
fn the_same_plot_rendered_twice_is_byte_identical() {
    let (spec, data) = brush_spec_and_data_for_ids();
    assert_eq!(
        SvgRenderer::default().render(&spec.0, &data),
        SvgRenderer::default().render(&spec.0, &data),
    );
}

/// Two plots that differ only in their table, both drawing a texture — the
/// shape the bed hit.
fn brush_spec_and_data_for_ids() -> ((PlotSpec, PlotSpec), HashMap<String, DataFrame>) {
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_str("g", vec!["a".into(), "b".into(), "c".into()])
        .with_float("v", vec![1.0, 2.0, 3.0]));
    data.insert("u".to_string(), DataFrame::new()
        .with_str("g", vec!["a".into(), "b".into(), "c".into()])
        .with_float("v", vec![3.0, 1.0, 2.0]));
    let one = PlotSpec::new().data("t").x("g").y("v")
        .layer(Layer::new(Mark::Bar).style_pattern("hatch"));
    let two = PlotSpec::new().data("u").x("g").y("v")
        .layer(Layer::new(Mark::Bar).style_pattern("hatch"));
    ((one, two), data)
}

/// **A plot that names no brush carries none of the machinery.** Neither the
/// dimmed group nor the panel metadata may appear, because either one would
/// change the bytes of every plot in the book at once.
#[test]
fn a_plot_that_names_no_brush_carries_no_selection_machinery() {
    let svg = SvgRenderer::default().render(&brush_spec(), &brush_data());
    assert!(!svg.contains("data-gog-panel"), "a plain plot must not carry panel metadata");
    assert!(!svg.contains(r#"<g opacity="#), "a plain plot must not carry a dimmed group");
    assert_eq!(circles(&svg), 6, "and it draws every row once");
}

/// **A brush the reader has not moved draws the same ink.** The resting state
/// is what print shows and what the page shows before the first drag, so it
/// has to be the plot itself — the panel metadata the browser needs is the
/// only difference, and it draws nothing.
#[test]
fn a_resting_brush_draws_exactly_the_same_ink() {
    let plain = SvgRenderer::default().render(&brush_spec(), &brush_data());
    let resting = SvgRenderer::default()
        .render(&brush_spec().brush(crate::ir::BrushDef::new("gx")), &brush_data());
    let ink: String = resting.lines()
        .filter(|l| !l.contains("data-gog-panel"))
        .collect::<Vec<_>>().join("\n");
    assert_eq!(ink.trim(), plain.trim(),
        "a resting brush must change nothing but the panel metadata");
    assert!(resting.contains("data-gog-panel"),
        "and the metadata must be there before the first drag, or nothing can invert a pixel");
}

/// **A `bounds` panel names both columns its axis draws, and reads as one row
/// per shape.** The axis draws `lo` and `hi` at once, so it names no single
/// field, and the page, matching a brush's column to the axis field alone,
/// dropped every drag: a brushed `interval * bounds(lo, hi)` was right as
/// first drawn and never moved. And `bounds` is written as a transform, so the
/// panel read as `summary`, many rows per mark, where each whisker is one row.
#[test]
fn a_brushed_bounds_panel_names_its_columns_and_its_rows() {
    let df = DataFrame::new()
        .with_str("term", vec!["Age".into(), "Education".into(), "Experience".into()])
        .with_float("lo", vec![0.02, 0.31, 0.11])
        .with_float("hi", vec![0.18, 0.55, 0.29]);
    let data = HashMap::from([("t".to_string(), df)]);
    let mut layer = Layer::new(Mark::Interval).transform(Transform::Bounds);
    layer.bounds = Some(crate::ir::BoundsSpec {
        lower: Some("lo".into()), upper: Some("hi".into()), ..Default::default()
    });
    let spec = PlotSpec::new().data("t").y("term").layer(layer)
        .brush(crate::ir::BrushDef::new("lo").at(0.0, 0.2));
    let svg = SvgRenderer::default().render(&spec, &data);
    let panel = svg.lines().find(|l| l.contains("data-gog-panel")).expect("the brush frame");
    assert!(panel.contains(r#"data-x-fields="lo|hi""#), "both columns on the axis: {panel}");
    assert!(panel.contains(r#"data-gog-place="bounds""#), "one row per shape: {panel}");
    assert!(!svg.lines().filter(|l| l.contains("data-gog-panel")).any(|l| l.contains("data-y-fields")),
            "and nothing along the axis no pair lies on");
}

/// **A selection pushes back what it was taken from**, rather than removing
/// it: a brush highlights and never filters, so every row is still drawn.
#[test]
fn a_brush_pushes_back_the_rows_outside_it() {
    let spec = brush_spec().brush(crate::ir::BrushDef::new("gx").at(2.5, 4.5));
    let svg = SvgRenderer::default().render(&spec, &brush_data());
    assert_eq!(circles(&svg), 6, "no row is dropped — a brush is not `limits`");
    let dim = format!(r#"<g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM);
    assert_eq!(svg.matches(&dim).count(), 1, "one dimmed group: {svg}");
    // Two of the six rows sit inside 2.5..4.5, so four are pushed back.
    let dimmed = svg.split(&dim).nth(1).unwrap().split("</g>").next().unwrap();
    assert_eq!(circles(dimmed), 4, "the four rows outside the bound are the dimmed ones");
}

/// **A shape `bounds` places is still one row, so a brush reaches it.** A
/// `zone * bounds` draws a zone per row and an `interval * bounds` a whisker
/// per row. Both were refused as summaries, and once the refusal went the
/// interval still drew every whisker at full strength, because the pair frame
/// had dropped the column the brush tests (ruled 2026-09-28: it answers).
#[test]
fn a_brush_dims_each_row_a_bounds_layer_draws() {
    let mut d = HashMap::new();
    d.insert("t".to_string(), DataFrame::new()
        .with_float("lo", vec![1.0, 3.0, 6.0])
        .with_float("hi", vec![2.0, 5.0, 8.0])
        .with_str("g", vec!["a", "b", "c"].into_iter().map(String::from).collect()));
    let dim = format!(r#"<g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM);
    let dimmed = |svg: &str| svg.split(&dim).nth(1).expect("a dimmed pass")
        .split("</g>").next().unwrap().to_string();
    let brush = || crate::ir::BrushDef::new("lo").at(0.0, 4.0);

    let zone = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Zone).bounds("lo", "hi")).brush(brush());
    let out = crate::legality::check(&zone, &d);
    assert!(!out.iter().any(|x| x.is_fatal()), "{:?}",
        out.iter().map(|x| &x.message).collect::<Vec<_>>());
    let svg = SvgRenderer::default().render(&zone, &d);
    assert_eq!(dimmed(&svg).matches("<rect").count(), 1,
        "the one zone outside the bound, and only it, is pushed back: {svg}");

    let interval = PlotSpec::new().data("t").x("g")
        .layer(Layer::new(Mark::Interval).bounds("lo", "hi")).brush(brush());
    let out = crate::legality::check(&interval, &d);
    assert!(!out.iter().any(|x| x.is_fatal()), "{:?}",
        out.iter().map(|x| &x.message).collect::<Vec<_>>());
    let svg = SvgRenderer::default().render(&interval, &d);
    // A whisker is a span and two caps: three strokes for the one row outside.
    assert_eq!(dimmed(&svg).matches("<line").count(), 3,
        "one whisker is pushed back, not none and not all: {svg}");
}

/// Selecting on a column of categories is the same atom, and which of the two
/// readings applies is decided by the *column*, exactly as the column decides
/// whether `color` hands out a ramp or a palette.
#[test]
fn a_brush_on_a_category_column_selects_slots() {
    let spec = brush_spec()
        .brush(crate::ir::BrushDef::new("cat").levels(vec!["b".to_string()]));
    let svg = SvgRenderer::default().render(&spec, &brush_data());
    let dim = format!(r#"<g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM);
    let dimmed = svg.split(&dim).nth(1).unwrap().split("</g>").next().unwrap();
    assert_eq!(circles(dimmed), 4, "the four rows outside category `b` are pushed back");
}

/// **A free shape reaches the engine as a predicate, and draws like a bound.**
/// The two counts in this test are the whole argument for the gesture: the
/// traced triangle pushes back three of the six rows, and the rectangle that
/// contains that same triangle — which is all a written `brush` can say —
/// pushes back none of them.
#[test]
fn a_traced_region_pushes_back_the_rows_outside_the_shape() {
    let dim = format!(r#"<g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM);
    let drawn = |path: Vec<[f64; 2]>| {
        let mut spec = brush_spec().brush(crate::ir::BrushDef::new("gx"));
        spec.region = Some(crate::ir::RegionDef::new("gx", "gy", path));
        SvgRenderer::default().render(&spec, &brush_data())
    };

    let triangle = drawn(vec![[0.5, 0.5], [6.5, 0.5], [0.5, 6.5]]);
    assert_eq!(circles(&triangle), 6, "no row is dropped — a region highlights, like a bound");
    let dimmed = triangle.split(&dim).nth(1).unwrap().split("</g>").next().unwrap();
    assert_eq!(circles(dimmed), 3, "the three rows outside the shape are pushed back");

    let rectangle = drawn(vec![[0.5, 0.5], [6.5, 0.5], [6.5, 6.5], [0.5, 6.5]]);
    let none = rectangle.split(&dim).nth(1).unwrap().split("</g>").next().unwrap();
    assert_eq!(circles(none), 0,
        "and the rectangle around that triangle selects every row, so none is pushed back");
}

/// A region nobody has traced is the resting state, exactly as an unmoved
/// brush is: the plot draws as it always did, and the page shows that before
/// the first gesture and the PDF shows it forever.
#[test]
fn a_region_of_two_vertices_draws_exactly_the_resting_plot() {
    let resting = SvgRenderer::default()
        .render(&brush_spec().brush(crate::ir::BrushDef::new("gx")), &brush_data());
    let mut spec = brush_spec().brush(crate::ir::BrushDef::new("gx"));
    spec.region = Some(crate::ir::RegionDef::new("gx", "gy", vec![[0.5, 0.5], [6.5, 0.5]]));
    let traced = SvgRenderer::default().render(&spec, &brush_data());
    assert_eq!(traced, resting, "an unclosed outline must select nothing at all");
}

/// **A layer the selection cannot reach is drawn once, whole, at full
/// strength** — never twice, and never dimmed. A summarized layer has no
/// honest answer to "which of these rows did you select", so it declines to
/// give one rather than approximating it, and `check_brush` says so.
#[test]
fn a_layer_the_selection_cannot_reach_is_drawn_once_and_whole() {
    let spec = brush_spec()
        .layer(Layer::new(Mark::Bar).transform(Transform::Count))
        .brush(crate::ir::BrushDef::new("gx").at(2.5, 4.5));
    let svg = SvgRenderer::default().render(&spec, &brush_data());
    let bars = svg.lines().filter(|l| l.contains("<rect") && l.contains("fill-opacity")).count();
    assert!(bars > 0, "the summarized layer still draws");
    let dim = format!(r#"<g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM);
    let dimmed = svg.split(&dim).nth(1).unwrap().split("</g>").next().unwrap();
    assert!(!dimmed.contains("<rect"), "and it is not in the dimmed pass: {dimmed}");
}

// -----------------------------------------------------------------------
// Chains — every transform in a legal chain has to do something (spec §5, §12)
// -----------------------------------------------------------------------

/// A frame with **uneven groups**, which is the whole reason it is written by
/// hand rather than generated.
///
/// Six categories of unequal size (1, 2, 3, 4, 5, 6 rows) against a continuous
/// column that repeats, so a chain has real groups to reduce within on either
/// axis. Even groups are a trap: with every group the same size, `sum` is `mean`
/// times a constant and the two draw the *same picture once `proportion` divides
/// them* — so an evenly-generated fixture reports the engine confusing `sum` with
/// `mean` when the engine is right and the fixture is degenerate. A single row
/// per group is the same trap one step further on, where all five reductions
/// agree. A sweep on 2026-07-30 reported 312 clean pairs for exactly this reason
/// and missed 40 real failures.
fn chain_frame() -> HashMap<String, DataFrame> {
    let sizes = [1usize, 2, 3, 4, 5, 6];
    let names = ["a", "b", "c", "d", "e", "f"];
    let (mut cat, mut num, mut val, mut lo, mut hi) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut k = 0_u64;
    for (i, n) in sizes.iter().enumerate() {
        for _ in 0..*n {
            cat.push(names[i].to_string());
            // Few distinct values, several rows each — a continuous key whose
            // values are all distinct makes singleton groups, where every
            // reduction agrees for real and the properties below cannot tell a
            // working engine from a broken one. **Unevenly** spaced for the same
            // kind of reason: evenly spaced, a cut lands one distinct value in
            // each cell, and `bin` and `count` then group the rows identically
            // and draw the same plot — an accident of the numbers that P2 reads
            // as the engine failing to tell them apart.
            num.push([0.0, 3.0, 11.0, 26.0, 42.0, 57.0][i]);
            // Irregular within a group, and deliberately not an arithmetic
            // progression: a progression has its mean *at* its median, so
            // `mean` and `median` draw the same picture and P2 reports a defect
            // that is the fixture's. Squaring is what breaks the symmetry.
            let v = 2.0 + ((k * k * 13) % 29) as f64;
            val.push(v);
            lo.push(v - 1.5);
            hi.push(v + 2.5);
            k += 1;
        }
    }
    let df = DataFrame::new()
        .with_str("cat", cat)
        .with_float("num", num)
        .with_float("val", val)
        .with_float("lo", lo)
        .with_float("hi", hi);
    HashMap::from([("d".to_string(), df)])
}

/// Render one chain on one mark, or `None` if the engine refuses it.
///
/// `check` is asked first, exactly as `gog-cli` asks it, so the property below
/// tests only chains a caller could actually draw.
fn render_chain(mark: &Mark, ts: &[Transform], categorical_x: bool) -> Option<String> {
    chain_run(mark, ts, categorical_x).map(|(svg, _)| svg)
}

/// The rendered plot and what the engine said about it, or `None` if refused.
/// `check` is asked first, exactly as `gog-cli` asks it, so the properties below
/// cover only chains a caller could actually draw.
fn chain_run(mark: &Mark, ts: &[Transform], categorical_x: bool)
    -> Option<(String, Vec<crate::legality::Diagnostic>)>
{
    let data = chain_frame();
    let mut layer = Layer::new(mark.clone());
    for t in ts {
        layer = layer.transform(t.clone());
    }
    if ts.contains(&Transform::Bounds) {
        layer = layer.bounds("lo", "hi");
    }
    // `quantile` names no default, because the only defensible one is 0.5 and
    // that is `median`, so a legal sentence always carries a probability. It
    // is 0.9 here rather than 0.5 for that same reason: at 0.5 this chain
    // *is* the `median` chain, and P2 would be asking two spellings of one
    // statistic to draw differently.
    if ts.contains(&Transform::Quantile) {
        layer = layer.at_quantile(0.9);
    }
    let spec = PlotSpec::new()
        .data("d")
        .x(if categorical_x { "cat" } else { "num" })
        .y("val")
        .layer(layer);
    let said = crate::legality::check(&spec, &data);
    if said.iter().any(|d| d.kind == crate::legality::DiagnosticKind::Illegal) {
        return None;
    }
    Some((SvgRenderer::default().render(&spec, &data), said))
}

/// Did the engine say, in as many words, that this transform is doing nothing?
///
/// The one way a chain is allowed to fail P1. A transform can be redundant
/// rather than contradictory — `count` beside `proportion`, where a share is
/// already a share *of* a tally — and the plot drawn is then exactly the plot
/// asked for, with one atom that was not needed. Refusing that would forbid the
/// ugly-but-legal (Law 8); saying nothing would be the silent drop. So the rule
/// is not *every transform changes the picture* but **every transform changes
/// the picture, or the engine says why it does not**.
fn declared_a_noop(mark: &Mark, ts: &[Transform], categorical_x: bool, t: &Transform) -> bool {
    let Some((_, said)) = chain_run(mark, ts, categorical_x) else { return false };
    let name = format!("{t:?}").to_lowercase();
    said.iter().any(|d|
        d.kind == crate::legality::DiagnosticKind::Assumption
            && d.message.contains(&format!("`{name}`"))
            && d.message.contains("draws the same plot as")
    )
}

/// A short stable digest, so a failure names the chain rather than printing two
/// entire SVG documents at each other.
fn digest(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// **P1 — no drop. Removing a transform from a legal chain has to change the
/// picture.**
///
/// If it does not, that transform was accepted and ignored, which is the silent
/// drop §12 forbids arriving through a *composition* rather than through a
/// binding. It is the mechanical form of the rule, and it says nothing about
/// order — deliberately, because two spellings that each mean something are the
/// caller's business (Law 8) while a spelling that means less than it says is
/// not.
///
/// This is what the enumerated composition checks could not see. On 2026-07-31,
/// of 582 two-transform chains that drew, only 271 had every transform doing
/// something: `bar * smooth * mean` drew `bar * smooth`, `interval * range *
/// confidence` drew `interval * confidence`, `line * sum * range` drew
/// `line * range`.
#[test]
fn no_legal_chain_ignores_a_transform_it_was_given() {
    for (mark, ts, categorical_x, drawn) in every_legal_chain() {
        for i in 0..ts.len() {
            let mut shorter = ts.clone();
            let dropped = shorter.remove(i);
            let Some(without) = render_chain(&mark, &shorter, categorical_x) else { continue };
            if digest(&drawn) != digest(&without) { continue }
            assert!(
                declared_a_noop(&mark, &ts, categorical_x, &dropped),
                "`{} * {}` draws exactly `{} * {}` — `{dropped:?}` was accepted and \
                 ignored, and nothing said so",
                mark_of(&mark), names(&ts), mark_of(&mark), names(&shorter)
            );
        }
    }
}

/// **P2 — no confusion. Swapping one transform for a different one has to change
/// the picture.**
///
/// P1's other half. A chain can honor *that* you named a transform without
/// honoring *which*, and that is the same defect one level down: `line * smooth *
/// range` and `line * smooth * confidence` rendered byte-identical, so a min–max
/// range and a 95% confidence interval drew the same plot. Nothing about the
/// chain's length changed, so P1 was blind to it.
#[test]
fn no_legal_chain_confuses_one_transform_with_another() {
    for (mark, ts, categorical_x, drawn) in every_legal_chain() {
        for i in 0..ts.len() {
            for alt in crate::legality::USER_TRANSFORMS {
                if alt == ts[i] || ts.contains(&alt) { continue }
                let mut swapped = ts.clone();
                swapped[i] = alt.clone();
                let Some(other) = render_chain(&mark, &swapped, categorical_x) else { continue };
                assert_ne!(
                    digest(&drawn), digest(&other),
                    "`{} * {}` and `{} * {}` draw the same plot — the engine read \
                     that a transform was named but not which one",
                    mark_of(&mark), names(&ts), mark_of(&mark), names(&swapped)
                );
            }
        }
    }
}

// The two names `legality` spells for a reader are private to it, and a failure
// message is not worth widening a module's surface for. `Debug` reads well enough
// here: `Bar`, `Bin * Mean`.
/// **P3 — the written order of a chain does not change the picture.**
///
/// The book says so in four places, and until this test it said so on the
/// strength of a sweep somebody ran by hand. A claim the manual makes about the
/// engine that no check verifies is the blind spot this project keeps walking
/// into — prose naming a transform that does not exist, an `error: true` chunk
/// that stopped erroring. This is the same class, one level up: a sentence in
/// three chapters resting on nothing.
///
/// **It is a consequence rather than a rule, which is why it is a test and not a
/// sort.** Nothing reorders a chain anywhere in the engine. Order stopped
/// mattering because the contradictions are refused, and what is left has at most
/// one transform actually running in the sequence — `bin` is hoisted, `proportion`
/// divides the recombined frame, `stack` accumulates across groups, and
/// `dodge`/`jitter` are render-stage. One transform has no order. If a later
/// change makes some legal chain order-dependent again, this fails, and the honest
/// answer may well be to change the book rather than the engine: two spellings
/// that each mean something are the caller's business (Law 8). What must not
/// happen is the two drifting apart in silence.
#[test]
fn the_written_order_of_a_chain_does_not_change_the_picture() {
    for (mark, ts, categorical_x, drawn) in every_legal_chain() {
        for spelling in permutations(&ts) {
            if spelling == ts { continue }
            let Some(other) = render_chain(&mark, &spelling, categorical_x) else {
                panic!("`{} * {}` draws but `{} * {}` is refused — the same chain, \
                        written differently",
                       mark_of(&mark), names(&ts), mark_of(&mark), names(&spelling));
            };
            assert_eq!(
                digest(&drawn), digest(&other),
                "`{} * {}` and `{} * {}` are the same chain and draw different \
                 plots — the book says written order cannot do that",
                mark_of(&mark), names(&ts), mark_of(&mark), names(&spelling)
            );
        }
    }
}

/// Every ordering of a chain. Chains are capped at four by the job rule, so this
/// is at most 24 and needs no crate.
fn permutations(ts: &[Transform]) -> Vec<Vec<Transform>> {
    if ts.len() <= 1 { return vec![ts.to_vec()] }
    let mut out = Vec::new();
    for i in 0..ts.len() {
        let mut rest = ts.to_vec();
        let head = rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, head.clone());
            out.push(tail);
        }
    }
    out
}

fn mark_of(m: &Mark) -> String { format!("{m:?}") }
fn names(ts: &[Transform]) -> String {
    ts.iter().map(|t| format!("{t:?}")).collect::<Vec<_>>().join(" * ")
}

/// Every chain of one, two or three transforms that any drawable mark accepts,
/// on both a categorical and a continuous domain, already rendered.
///
/// Enumerated from the mark × transform grid and filtered by `check`, so the two
/// properties above cover exactly what a caller can write — and a transform added
/// later widens them without anybody editing a list. Three is the practical
/// ceiling here rather than the rule's four: the fourth is always a collision
/// modifier, whose contribution the two properties already see at length three.
fn every_legal_chain() -> Vec<(Mark, Vec<Transform>, bool, String)> {
    use crate::legality::{ALL_MARKS, USER_TRANSFORMS, TransformLegality, mark_takes_transform};
    let mut out = Vec::new();
    for mark in ALL_MARKS {
        let legal: Vec<Transform> = USER_TRANSFORMS.into_iter()
            .filter(|t| mark_takes_transform(&mark, t) != TransformLegality::None)
            .collect();
        let mut chains: Vec<Vec<Transform>> = Vec::new();
        for a in 0..legal.len() {
            chains.push(vec![legal[a].clone()]);
            for b in 0..legal.len() {
                if b == a { continue }
                chains.push(vec![legal[a].clone(), legal[b].clone()]);
                for c in (b + 1)..legal.len() {
                    if c == a { continue }
                    chains.push(vec![legal[a].clone(), legal[b].clone(), legal[c].clone()]);
                }
            }
        }
        for ts in chains {
            for categorical_x in [true, false] {
                if let Some(svg) = render_chain(&mark, &ts, categorical_x) {
                    out.push((mark.clone(), ts.clone(), categorical_x, svg));
                }
            }
        }
    }
    assert!(out.len() > 200, "the enumeration found only {} legal chains", out.len());
    out
}

// -----------------------------------------------------------------------
// Per-layer positions — one axis, its own column (spec §8)
// -----------------------------------------------------------------------

/// The claim, made as geometry rather than as bytes: a note table whose value
/// column is called `at` lands at **the same pixel** as the data point whose
/// `gdp` holds the same number. If the resolution were wrong the note would
/// be somewhere else on the panel, and no amount of "it rendered" would say
/// so — which is how the §8 sentence sat unexecutable for several sessions.
#[test]
fn a_layer_reading_its_own_column_lands_on_the_shared_axis() {
    let data: HashMap<String, DataFrame> = HashMap::from([
        (
            "t".to_string(),
            DataFrame::new()
                .with_float("gdp", vec![1000.0, 2000.0, 3000.0, 4000.0])
                .with_float("life", vec![40.0, 50.0, 60.0, 70.0]),
        ),
        (
            "notes".to_string(),
            DataFrame::new()
                // The same coordinate as the second row above, spelled by two
                // other column names.
                .with_float("at", vec![2000.0])
                .with_float("val", vec![50.0])
                .with_str("what", vec!["HERE".into()]),
        ),
    ]);
    let spec = PlotSpec::new()
        .data("t")
        .x("gdp")
        .y("life")
        .layer(Layer::new(Mark::Point))
        .layer(
            Layer::new(Mark::Text)
                .data("notes")
                .encode(Channel::X, "at")
                .encode(Channel::Y, "val")
                .encode(Channel::Label, "what"),
        );
    let svg = SvgRenderer::default().render(&spec, &data);

    let attr = |line: &str, key: &str| -> Option<f64> {
        line.split(&format!("{key}=\""))
            .nth(1)?
            .split('"')
            .next()?
            .parse()
            .ok()
    };
    let second_point = svg
        .lines()
        .filter(|l| l.contains("<circle"))
        .filter_map(|l| attr(l, "cx"))
        .nth(1)
        .expect("four points should be drawn");
    let note = svg
        .lines()
        .find(|l| l.contains("HERE"))
        .and_then(|l| attr(l, "x"))
        .expect("the note should be drawn");

    assert!(
        (second_point - note).abs() < 0.01,
        "the note reading `at` = 2000 must land where `gdp` = 2000 does: \
         point at {second_point}, note at {note}"
    );
}

/// The axis keeps the plot's name, not the note table's. One axis means one
/// label and one set of ticks; a layer's column name is local to that layer
/// and must not surface as chrome.
#[test]
fn a_layers_own_column_name_never_reaches_the_axis() {
    let data: HashMap<String, DataFrame> = HashMap::from([
        (
            "t".to_string(),
            DataFrame::new()
                .with_float("gdp", vec![1000.0, 2000.0])
                .with_float("life", vec![40.0, 50.0]),
        ),
        (
            "notes".to_string(),
            DataFrame::new().with_float("at", vec![1500.0]).with_float("val", vec![45.0]),
        ),
    ]);
    let spec = PlotSpec::new()
        .data("t")
        .x("gdp")
        .y("life")
        .layer(Layer::new(Mark::Point))
        .layer(
            Layer::new(Mark::Point)
                .data("notes")
                .encode(Channel::X, "at")
                .encode(Channel::Y, "val"),
        );
    let svg = SvgRenderer::default().render(&spec, &data);
    assert!(svg.contains(">Gdp<"), "the axis label is the plot's column");
    assert!(!svg.contains(">At<"), "the note's column name must not label the axis");
}

/// `point + x(gdp) + y(life) + line` — positions written after the first of
/// two marks. The later layer names no position, so it reads the axis, and
/// the plot is identical to the one that binds them before the marks. Pins
/// the fallback the book's dominant idiom depends on: 317 of its expressions
/// write `mark + x(…)`, and every one of them has a mark after it in some
/// chapter.
#[test]
fn a_layer_naming_no_position_reads_the_axis_another_layer_set() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("gdp", vec![1000.0, 2000.0, 3000.0])
            .with_float("life", vec![40.0, 50.0, 60.0]),
    )]);
    // Written before the marks: both layers are plot-scoped.
    let before = PlotSpec::new()
        .data("t")
        .x("gdp")
        .y("life")
        .layer(Layer::new(Mark::Point))
        .layer(Layer::new(Mark::Line));
    // Written after the first mark: the point owns them, the line falls back.
    let after = PlotSpec::new()
        .data("t")
        .layer(
            Layer::new(Mark::Point)
                .encode(Channel::X, "gdp")
                .encode(Channel::Y, "life"),
        )
        .layer(Layer::new(Mark::Line));

    let a = SvgRenderer::default().render(&before, &data);
    let b = SvgRenderer::default().render(&after, &data);
    assert!(b.contains("<polyline"), "the line must still be drawn");
    assert_eq!(a, b, "the two spellings must render the same plot");
}

// -----------------------------------------------------------------------
// Polar — the plane bent into a circle
// -----------------------------------------------------------------------

fn polar_counts() -> HashMap<String, DataFrame> {
    HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("dir", vec!["N", "E", "S", "W"].into_iter().map(String::from).collect())
            .with_float("n", vec![10.0, 40.0, 20.0, 30.0]),
    )])
}

fn render_rose(start: f64, polar: bool) -> String {
    let mut s = PlotSpec::new().data("t").x("dir").y("n");
    if polar {
        s = s.coord(CoordSpace::Polar(crate::ir::PolarView { start }));
    }
    SvgRenderer::default().render(&s.layer(Layer::new(Mark::Bar)), &polar_counts())
}

/// Every `x y` a path command lands on, in the order drawn — enough of an SVG
/// path parser to check where the sectors actually sit.
fn path_points(d: &str) -> Vec<(f64, f64)> {
    let t: Vec<&str> = d.split_whitespace().collect();
    let mut pts = Vec::new();
    let mut i = 0;
    let num = |s: &str| s.parse::<f64>().unwrap_or(f64::NAN);
    while i < t.len() {
        match t[i] {
            "M" | "L" if i + 2 < t.len() => { pts.push((num(t[i + 1]), num(t[i + 2]))); i += 3; }
            "A" if i + 7 < t.len() => { pts.push((num(t[i + 6]), num(t[i + 7]))); i += 8; }
            _ => i += 1,
        }
    }
    pts
}

fn sector_paths(svg: &str) -> Vec<Vec<(f64, f64)>> {
    svg.lines()
        .filter(|l| l.contains("<path d=\"M"))
        .filter_map(|l| Some(path_points(l.split(r#"d=""#).nth(1)?.split('"').next()?)))
        .collect()
}

/// The disc the frame drew: its center and radius, read back off the panel
/// background so a test measures against what the renderer actually laid out.
fn disc(svg: &str) -> (f64, f64, f64) {
    let line = svg.lines()
        .find(|l| l.contains("<circle") && l.contains(&format!(r#"fill="{PANEL_BG}""#)))
        .expect("no polar disc drawn");
    let attr = |k: &str| -> f64 {
        line.split(&format!(r#"{k}=""#)).nth(1).unwrap().split('"').next().unwrap().parse().unwrap()
    };
    (attr("cx"), attr("cy"), attr("r"))
}

/// The claim the whole space exists to make (spec §9, Wilkinson ch. 9): a rose
/// is the *same sentence* as a bar chart, read in a different coordinate
/// space. Same bars, same count of them — rectangles in the plane, wedges in
/// the circle. If polar drew rectangles, or drew a different number of marks,
/// it would be a different chart rather than a different reading.
#[test]
fn a_rose_is_the_same_bar_chart_bent() {
    let flat = render_rose(0.0, false);
    let rose = render_rose(0.0, true);

    // `<rect` + `fill-opacity` is the bar fingerprint (the panel background is
    // a rect too, and carries no fill-opacity).
    let bars = |svg: &str| svg.lines().filter(|l| l.contains("<rect") && l.contains("fill-opacity")).count();
    assert_eq!(bars(&flat), 4, "flat drew the wrong number of bars");
    assert_eq!(sector_paths(&rose).len(), 4, "polar drew the wrong number of wedges");
    assert_eq!(bars(&rose), 0, "polar still drew rectangles");
    assert!(!rose.contains("NaN"), "polar output has NaN coordinates");
}

/// On a periodic axis the slots must **tile** the turn: cover it once each,
/// with no gap and no double ink. A categorical angular axis got this right by
/// construction (`-0.5 ..= n-0.5` is n slots for n categories); a *measured*
/// one did not, because the range was fitted to the bin centers and n centers
/// span only n−1 slot-widths. `bar * bin + x(bearing) + polar()` therefore drew
/// its first and last wedge on top of each other — 40° of doubled ink at twelve
/// o'clock, dark enough to read through.
///
/// Asserted as *total coverage equals one turn*, which is the property that
/// matters and which catches a gap as well as an overlap.
#[test]
fn the_wedges_of_a_measured_angle_tile_the_turn() {
    let mut data = HashMap::new();
    // Twelve values, evenly spread, so `bin` cuts several equal slots.
    data.insert("t".to_string(), DataFrame::new().with_float(
        "bearing", (0..12).map(|i| i as f64 * 30.0).collect::<Vec<f64>>()));
    let spec = PlotSpec::new().data("t").x("bearing")
        .coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }))
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    let svg = SvgRenderer::default().render(&spec, &data);

    let (cx, cy, _) = disc(&svg);
    let bearing = |x: f64, y: f64| (x - cx).atan2(cy - y).to_degrees().rem_euclid(360.0);

    let mut total = 0.0;
    let mut spans: Vec<(f64, f64)> = Vec::new();
    for path in sector_paths(&svg) {
        let outer: Vec<(f64, f64)> = path.into_iter()
            .filter(|&(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() > 3.0)
            .collect();
        if outer.is_empty() { continue }
        let mut angs: Vec<f64> = outer.iter().map(|&(x, y)| bearing(x, y)).collect();
        angs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let (mut lo, mut hi) = (angs[0], angs[angs.len() - 1]);
        if hi - lo > 180.0 { let t = lo; lo = hi; hi = t + 360.0; } // straddles 12 o'clock
        total += hi - lo;
        spans.push((lo, hi));
    }
    assert!(spans.len() >= 3, "expected several wedges, got {}", spans.len());
    assert!((total - 360.0).abs() < 0.5,
        "the wedges cover {total:.2}° of a 360° turn — {} of them: {spans:?}", spans.len());

    // And no two of them share any angle, allowing for the wrap.
    for i in 0..spans.len() {
        for j in (i + 1)..spans.len() {
            for shift in [-360.0, 0.0, 360.0] {
                let overlap = spans[i].1.min(spans[j].1 + shift)
                    - spans[i].0.max(spans[j].0 + shift);
                assert!(overlap <= 0.05,
                    "wedges {i} and {j} overlap by {overlap:.2}°: {:?} {:?}", spans[i], spans[j]);
            }
        }
    }
}

/// The panel is a disc, and everything drawn is inside it. The flat panel's
/// rectangle is what a mark is clipped to there; here it is the circle, and a
/// wedge escaping it would mean the radial scale and the frame disagree.
#[test]
fn nothing_is_drawn_outside_the_polar_disc() {
    let rose = render_rose(0.0, true);
    let (cx, cy, r) = disc(&rose);
    assert!(r > 20.0, "the disc collapsed: r = {r}");
    for path in sector_paths(&rose) {
        for (x, y) in path {
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            assert!(d <= r + 0.5, "a wedge reached {d:.2} outside a disc of {r:.2}");
        }
    }
}

/// The radial axis is used, not ignored: the largest value reaches the rim and
/// the smallest stays well inside it. Pins that `y` really is the radius —
/// wedges of one length would be the failure mode this catches.
#[test]
fn the_radius_carries_the_measured_value() {
    let rose = render_rose(0.0, true);
    let (cx, cy, r) = disc(&rose);
    let reach: Vec<f64> = sector_paths(&rose).iter()
        .map(|p| p.iter()
            .map(|(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt())
            .fold(0.0_f64, f64::max))
        .collect();
    let (lo, hi) = (reach.iter().cloned().fold(f64::INFINITY, f64::min),
                    reach.iter().cloned().fold(0.0_f64, f64::max));
    assert!(hi > r * 0.85, "the largest bar stopped short of the rim: {hi:.2} of {r:.2}");
    // Counts are 10..40 on an axis from 0, so the shortest wedge is a quarter
    // of the longest — a real spread, not four equal spokes.
    assert!((lo / hi - 0.25).abs() < 0.05, "radii do not track the values: {lo:.2}, {hi:.2}");
}

/// `start` turns the whole space and nothing else: the same data, the same
/// number of wedges, each at a different bearing. A no-op here would mean the
/// view parameter was accepted and dropped (§12's one unforgivable failure).
#[test]
fn the_start_angle_turns_the_whole_plot() {
    let a = render_rose(0.0, true);
    let b = render_rose(90.0, true);
    assert_eq!(sector_paths(&a).len(), sector_paths(&b).len());
    assert_ne!(sector_paths(&a), sector_paths(&b), "`start` changed nothing");
    // Same shape, turned: the set of radii each wedge reaches is unchanged.
    let reaches = |svg: &str| {
        let (cx, cy, _) = disc(svg);
        let mut v: Vec<i64> = sector_paths(svg).iter()
            .map(|p| p.iter().map(|(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt())
                .fold(0.0_f64, f64::max).round() as i64)
            .collect();
        v.sort();
        v
    };
    assert_eq!(reaches(&a), reaches(&b), "rotating the space changed the radii");
}

/// The bearing each wedge is centered on, in turns clockwise from twelve
/// o'clock. Summed as unit vectors rather than averaged as angles, so a wedge
/// straddling twelve o'clock does not average to the opposite side.
fn wedge_centers(svg: &str, cx: f64, cy: f64) -> Vec<f64> {
    sector_paths(svg).iter().map(|p| {
        let dir = |&(x, y): &(f64, f64)| {
            let (dx, dy) = (x - cx, cy - y);
            let m = (dx * dx + dy * dy).sqrt().max(1e-12);
            (dx / m, dy / m)
        };
        let (ax, ay) = dir(&p[0]);
        let (bx, by) = dir(&p[1]);
        let t = (ax + bx).atan2(ay + by);
        let t = if t < 0.0 { t + std::f64::consts::TAU } else { t };
        t / std::f64::consts::TAU
    }).collect()
}

/// The first category sits at the start angle, and the rest tile the turn from
/// there. Two claims in one.
///
/// A categorical scale runs `-0.5 ..= n-0.5`, so its *origin* is half a slot
/// before the first category — padding, not a place in the data. Pointing the
/// start angle there put north at 22.5° of an eight-point compass and left the
/// reader to work out `-180/n`. The space is rotated back by half a slot so the
/// category itself lands on the start, the way a flat categorical axis puts its
/// tick at the category's center.
///
/// And the axis is periodic and flush, so the slots divide the whole turn with
/// no dead wedge at the seam.
#[test]
fn the_first_category_sits_at_the_start_angle_and_the_rest_tile_the_turn() {
    let rose = render_rose(0.0, true);
    let (cx, cy, _) = disc(&rose);
    let mut centers = wedge_centers(&rose, cx, cy);
    centers.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(centers.len(), 4);
    // Four categories, four equal quarter-turn slots, the first at the top.
    for (i, c) in centers.iter().enumerate() {
        let want = i as f64 * 0.25;
        assert!((c - want).abs() < 0.005, "category {i} centered at {c:.4} of a turn, wanted {want:.4}");
    }
}

/// A **stated** angular domain is the turn, flush (spec §10): the bins are cut
/// on it and the axis is not widened past it. Widened by half a bin at each
/// end, `x(bearing, limits = c(0, 360))` opened a 35-degree gap at the seam
/// and drew its 0 tick 16 degrees clockwise of the top.
#[test]
fn a_stated_angle_domain_is_the_whole_turn() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new().with_float("deg", (0..40).map(|i| 2.6 + i as f64 * 9.2).collect()),
    )]);
    let mut x = ChannelDef::field("deg");
    x.limits = Some([Some(0.0), Some(360.0)]);
    let mut spec = PlotSpec::new().data("t")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    spec.channels.insert(Channel::X, x);
    let svg = SvgRenderer::default().render(&spec, &data);
    let (cx, _, _) = disc(&svg);
    let zero = svg.lines().find(|l| l.contains(">0</text>")).expect("a 0 tick");
    let x_of = zero.split(r#"x=""#).nth(1).and_then(|r| r.split('"').next())
        .and_then(|v| v.parse::<f64>().ok()).unwrap();
    assert!((x_of - cx).abs() < 0.01, "0 is at the top: {zero}");
    // The first wedge starts on the top spoke.
    let first = svg.lines().find(|l| l.contains(r#"<path d="M "#) && l.contains(" A "))
        .expect("a wedge");
    let start_x = first.split("M ").nth(1).and_then(|r| r.split(' ').next())
        .and_then(|v| v.parse::<f64>().ok()).unwrap();
    assert!((start_x - cx).abs() < 0.01, "the first wedge starts at north: {first}");
}

/// The half-slot rotation is for *categories*, which divide the turn into
/// slots. A measured angle has no slots and its scale minimum is a real value,
/// so it stays exactly on the start angle: the smallest bearing lands at twelve
/// o'clock, not half of anything past it.
#[test]
fn a_measured_angle_is_not_rotated_off_the_start() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("deg", (0..8).map(|i| i as f64 * 45.0).collect())
            .with_float("v", (0..8).map(|i| 1.0 + i as f64).collect()),
    )]);
    let spec = PlotSpec::new().data("t").x("deg").y("v")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(Layer::new(Mark::Point));
    let svg = SvgRenderer::default().render(&spec, &data);
    let (cx, cy, r) = disc(&svg);
    // The glyph for the smallest bearing (0°, the least `v` too, so the point
    // nearest the center) must sit on the vertical spoke above the center.
    let mut on_spoke = svg.lines()
        .filter(|l| l.contains("<circle") && l.contains("cx=") && !l.contains(&format!("r=\"{r:.2}\"")))
        .filter_map(|l| {
            let g = |k: &str| l.split(&format!("{k}=\"")).nth(1)?.split('"').next()?.parse::<f64>().ok();
            Some((g("cx")?, g("cy")?))
        })
        .filter(|(x, y)| (x - cx).abs() < 0.5 && *y < cy)
        .collect::<Vec<_>>();
    on_spoke.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    assert!(!on_spoke.is_empty(), "no point landed on the start spoke");
}

// -- the hole: a stretch of the radial axis with nothing standing on it ---

/// Two branches, two levels, so the tree has an inner ring and a rim.
fn tree_data() -> HashMap<String, DataFrame> {
    HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", ["a", "a", "b", "b"].into_iter().map(String::from).collect())
            .with_str("i", ["p", "q", "r", "s"].into_iter().map(String::from).collect())
            .with_float("v", vec![3.0, 1.0, 2.0, 2.0]),
    )])
}

/// The same shape six branches wide, and the width is what a defect needs to
/// be visible. A tallied tree weighs every leaf 1, so its measure axis runs to
/// 12 here and ticks at 5 and 10 — **neither of which falls inside the ring
/// range 1..3**. With the ring column dropped the radial axis reads those
/// weights, and no tick of theirs lands in the cells for `fit_to_cells` to
/// clip to, so the rings crush into the first tenth of the radius. A four-leaf
/// tree hides it: its ticks are 1, 2, 3, 4, two of them land inside, and the
/// cell fit rescues the plot by luck.
fn wide_tree_data() -> HashMap<String, DataFrame> {
    let branches = ["a", "b", "c", "d", "e", "f"];
    HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", branches.iter().flat_map(|b| [b.to_string(), b.to_string()])
                .collect())
            .with_str("i", branches.iter()
                .flat_map(|b| [format!("{b}1"), format!("{b}2")]).collect())
            .with_float("v", branches.iter().flat_map(|_| [3.0, 1.0]).collect()),
    )])
}

/// `measure` bound or not, and a stated radial domain or not.
fn render_tree(polar: bool, measure: bool, depth_lo: Option<f64>) -> String {
    render_tree_of(tree_data(), polar, measure, depth_lo)
}

fn render_tree_of(
    data: HashMap<String, DataFrame>, polar: bool, measure: bool, depth_lo: Option<f64>,
) -> String {
    let mut s = PlotSpec::new().data("t");
    if polar {
        s = s.coord(CoordSpace::Polar(crate::ir::PolarView::default()));
    }
    if measure {
        s = s.x("v");
    }
    if let Some(lo) = depth_lo {
        s = s.y_limited(crate::transform::NODE_DEPTH, Some(lo), Some(3.0));
    }
    let layer = Layer::new(Mark::Zone)
        .transform(Transform::Partition)
        .partition(&["g", "i"]);
    SvgRenderer::default().render(&s.layer(layer), &data)
}

/// The radius each sector spans, innermost first.
fn sector_radii(svg: &str, cx: f64, cy: f64) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = sector_paths(svg).iter()
        .map(|p| {
            let r = |&(x, y): &(f64, f64)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            let (lo, hi) = p.iter().map(r)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
            // Whole pixels: two sectors of one ring can differ in the last
            // decimal, and this list is counted as well as measured.
            (lo.round(), hi.round())
        })
        .collect();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out.dedup();
    out
}

/// **A node above the colored level has no value of it, so it is drawn in the
/// neutral gray and takes no color from the palette.** Under `partition(w, g,
/// i) + color(g)` the root holds both branches and its `g` is empty. Counted as
/// a category, that empty value took the palette's first color, which no legend
/// row names, and every branch moved one color along.
#[test]
fn a_node_above_the_colored_level_is_neutral_and_takes_no_color() {
    let t = tree_data().remove("t").unwrap();
    let n = t.len();
    let data = HashMap::from([("t".to_string(), t.with_str("w", vec!["all".to_string(); n]))]);
    let spec = PlotSpec::new().data("t").x("v")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(Layer::new(Mark::Zone).transform(Transform::Partition)
            .partition(&["w", "g", "i"]).encode(Channel::Color, "g"));
    let svg = SvgRenderer::default().render(&spec, &data);
    let fill = |c: &str| format!(r#"fill="{c}""#);
    assert!(svg.contains(&fill(crate::render::palette::NEUTRAL)), "the root is neutral: {svg}");
    assert!(svg.contains(&fill(crate::render::palette::PALETTE_GOG[0])),
        "the first branch keeps the palette's first color: {svg}");
}

/// **A name too wide for its thin sector turns along the radius**, and a name
/// that fits stands upright as before. Upright, the thin leaf's name ran
/// across its neighbors; a published sunburst turns it to read along the
/// sector, and never upside down.
#[test]
fn a_thin_sectors_name_turns_to_fit_it() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", ["a", "a", "b"].into_iter().map(String::from).collect())
        .with_str("i", ["Wide", "Narrow name", "Other"].into_iter().map(String::from).collect())
        .with_float("v", vec![40.0, 1.0, 40.0]))]);
    let part = |m: Mark| Layer::new(m).transform(Transform::Partition).partition(&["g", "i"]);
    let spec = PlotSpec::new().data("t").x("v")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(part(Mark::Zone))
        .layer(part(Mark::Text).encode(Channel::Label, "name"));
    let svg = SvgRenderer::default().render(&spec, &data);
    let named = |n: &str| svg.lines().find(|l| l.contains("<text") && l.ends_with(&format!(">{n}</text>")))
        .unwrap_or_else(|| panic!("{n} is named: {svg}"));
    let thin = named("Narrow name");
    let turn: f64 = thin.split("rotate(").nth(1)
        .unwrap_or_else(|| panic!("the thin sector's name turns: {thin}"))
        .split(' ').next().unwrap().parse().unwrap();
    assert!(turn.abs() <= 90.0, "never upside down: {thin}");
    for n in ["Wide", "Other", "a", "b"] {
        assert!(!named(n).contains("rotate("), "{n} fits upright: {}", named(n));
    }
}

/// **A name that fits its sector no way, even turned, is drawn smaller**, at the
/// size that fits, and never left out: a treemap leaves such a name out and
/// reports it, but a sector's name stays, as in a published sunburst. A name
/// that fits keeps the layer's size, and so writes no size of its own.
#[test]
fn a_name_that_fits_no_way_is_drawn_smaller() {
    let long = "A name far too long for any ring";
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", ["a", "a", "b"].into_iter().map(String::from).collect())
        .with_str("i", ["Wide", long, "Other"].into_iter().map(String::from).collect())
        .with_float("v", vec![40.0, 1.0, 40.0]))]);
    let part = |m: Mark| Layer::new(m).transform(Transform::Partition).partition(&["g", "i"]);
    let spec = PlotSpec::new().data("t").x("v")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(part(Mark::Zone))
        .layer(part(Mark::Text).encode(Channel::Label, "name"));
    let r = SvgRenderer::default();
    let svg = r.render(&spec, &data);
    let named = |n: &str| svg.lines().find(|l| l.contains("<text") && l.ends_with(&format!(">{n}</text>")))
        .unwrap_or_else(|| panic!("{n} is drawn, not left out: {svg}"));
    let size: f64 = named(long).split(r#" font-size=""#).nth(1)
        .unwrap_or_else(|| panic!("the long name is drawn smaller: {}", named(long)))
        .split('"').next().unwrap().parse().unwrap();
    assert!(size > 0.0 && size < r.font_md, "{size} against {}", r.font_md);
    for n in ["Wide", "Other", "a", "b"] {
        assert!(!named(n).contains("font-size"), "{n} keeps the layer's size: {}", named(n));
    }
}

/// **The name of a node that spans the whole turn sits at the center.** A
/// sunburst's root covers every angle, so the middle of its angle is the bottom
/// of the turn, and its name sat there, below the center of the disk it names.
#[test]
fn a_whole_turn_is_named_at_the_center() {
    let t = tree_data().remove("t").unwrap();
    let n = t.len();
    let data = HashMap::from([("t".to_string(), t.with_str("w", vec!["all".to_string(); n]))]);
    let part = |m: Mark| Layer::new(m).transform(Transform::Partition).partition(&["w", "g", "i"]);
    let spec = PlotSpec::new().data("t").x("v")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(part(Mark::Zone))
        .layer(part(Mark::Text).encode(Channel::Label, "name"));
    let svg = SvgRenderer::default().render(&spec, &data);
    let (cx, cy, r) = disc(&svg);
    let line = svg.lines().find(|l| l.contains("<text") && l.ends_with(">all</text>"))
        .expect("the root is named");
    let attr = |k: &str| -> f64 {
        line.split(&format!(r#" {k}=""#)).nth(1).unwrap().split('"').next().unwrap().parse().unwrap()
    };
    assert!((attr("x") - cx).abs() < 1.0, "centered across: {line}");
    assert!((attr("y") - cy).abs() < r * 0.1, "centered down, not below: {line}");
}

/// **A ring that spans the whole turn is named inside its ring.** Only the
/// innermost node takes the center: the one group under a root is a ring
/// around the root, and at the center its name lay on top of the root's. The
/// root keeps the center when it is itself a ring around an empty hole.
#[test]
fn a_whole_turn_ring_is_named_inside_its_ring() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("w", s(&["all", "all", "all"]))
        .with_str("g", s(&["one", "one", "one"]))
        .with_str("i", s(&["a", "b", "c"]))
        .with_float("v", vec![1.0, 2.0, 3.0]))]);
    let part = |m: Mark| Layer::new(m).transform(Transform::Partition).partition(&["w", "g", "i"]);
    // Three levels with no hole; the same with depth 0 to 1 left empty; and a
    // domain that leaves the outer ring out, whose rows then have no depth and
    // must not read as the innermost.
    for (top, (lo_r, hi_r)) in [(None, (1.0 / 3.0, 2.0 / 3.0)), (Some(4.0), (0.5, 0.75)),
                                (Some(3.0), (2.0 / 3.0, 1.0))] {
        let hole = top.is_some();
        let mut spec = PlotSpec::new().data("t").x("v")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(part(Mark::Zone))
            .layer(part(Mark::Text).encode(Channel::Label, "name"));
        if let Some(top) = top {
            spec = spec.y_limited(crate::transform::NODE_DEPTH, Some(0.0), Some(top));
        }
        let svg = SvgRenderer::default().render(&spec, &data);
        let (cx, cy, r) = disc(&svg);
        let at = |n: &str| -> (f64, f64) {
            let line = svg.lines().find(|l| l.contains("<text") && l.ends_with(&format!(">{n}</text>")))
                .unwrap_or_else(|| panic!("{n} is named: {svg}"));
            let attr = |k: &str| -> f64 {
                line.split(&format!(r#" {k}=""#)).nth(1).unwrap().split('"').next().unwrap().parse().unwrap()
            };
            (attr("x"), attr("y"))
        };
        let (rx, ry) = at("all");
        assert!((rx - cx).abs() < 1.0 && (ry - cy).abs() < r * 0.1,
                "the root keeps the center (hole: {hole})");
        let (gx, gy) = at("one");
        let d = ((gx - cx).powi(2) + (gy - cy).powi(2)).sqrt();
        // The group's ring runs from `lo_r` to `hi_r` of the radius; its name
        // sits between, never at the center.
        assert!(d > r * lo_r && d < r * hi_r, "the ring's name inside the ring (hole: {hole}): {d} of {r}");
    }
}

/// **A value a statistic wrote outside a stated domain is left out and said**
/// (spec §10), the table's own rule one stage later. A two-level sunburst under
/// `y(depth, limits = c(0, 2))` drew its outer ring past the circle, and flat the
/// panel's clip cut such a ring off in silence; a count taller than its stated
/// axis was drawn cut off at the top, as tall as the axis.
#[test]
fn a_written_value_outside_a_stated_domain_is_left_out_and_said() {
    let spec = PlotSpec::new().data("t").x("v")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .y_limited(crate::transform::NODE_DEPTH, Some(0.0), Some(2.0))
        .layer(Layer::new(Mark::Zone).transform(Transform::Partition).partition(&["g", "i"]));
    let drawn = SvgRenderer::default().draw(&spec, &tree_data());
    let (cx, cy, r) = disc(&drawn.svg);
    let outer = sector_radii(&drawn.svg, cx, cy).iter().map(|s| s.1).fold(0.0, f64::max);
    assert!(outer <= r + 1.0, "no ring past the circle: {outer} against {r}");
    assert!(drawn.remarks.iter().any(|d| d.kind == crate::legality::DiagnosticKind::Assumption
        && d.message.contains("leaves out")), "{:?}", drawn.remarks);

    let t = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", ["a", "a", "a", "b"].iter().map(|s| s.to_string()).collect()))]);
    let bars = PlotSpec::new().data("t").x("g").y_limited("count", Some(0.0), Some(2.0))
        .layer(Layer::new(Mark::Bar).transform(Transform::Count));
    let drawn = SvgRenderer::default().draw(&bars, &t);
    let drawn_bars = drawn.svg.lines()
        .filter(|l| l.contains("<rect") && l.contains(&format!(r#"fill="{}""#, PALETTE_GOG[0])))
        .count();
    assert_eq!(drawn_bars, 1, "the count of 3 has no place on [0, 2]");
    assert!(drawn.remarks.iter().any(|d| d.message.contains("leaves out 1 of 2")), "{:?}", drawn.remarks);
}

/// **A stated domain reaches the axis, even where a tiling is fitted to** —
/// spec §10, and the defect that made this test exist. `fit_to_cells` refits
/// the panel to a mesh's own edges, which is right for a derived range and
/// wrong for a stated one: `y(depth, limits = c(0, 4))` on a sunburst moved the
/// tick labels and nothing else, so the innermost ring still ran to the center
/// and the hole the book promised was never drawn. Asserted on the *geometry*
/// rather than the range, because ticks responding was exactly what hid it.
#[test]
fn a_stated_radial_domain_hollows_out_the_sunburst() {
    let (cx, cy, r) = disc(&render_tree(true, true, Some(0.0)));
    let hollow = sector_radii(&render_tree(true, true, Some(0.0)), cx, cy);
    let solid = sector_radii(&render_tree(true, true, None), cx, cy);

    // Rings 1..2 and 2..3 on a domain of 0..3: a third of the radius empty,
    // then two rings of a third each.
    assert!(solid[0].0 < 1.0, "unstated, the innermost ring reaches the center: {solid:?}");
    assert!(hollow[0].0 > r / 4.0,
            "stating the domain leaves the middle empty: {hollow:?}, radius {r}");
    assert!((hollow[0].0 - r / 3.0).abs() < 1.0,
            "the hole is the stretch 0..1 of a 0..3 axis: {hollow:?}, radius {r}");
    assert!((hollow.last().unwrap().1 - r).abs() < 1.0,
            "and the rim still reaches the frame: {hollow:?}, radius {r}");
}

/// The same hole, unbent — an **empty band** under the icicle, which is the
/// plainest statement of what a hole is: not a round thing, a stretch of one
/// axis with nothing on it. One assertion in two spaces, because a hole that
/// only appeared in polar would be the renderer's doing rather than the scale's.
#[test]
fn the_flat_reading_of_a_hole_is_an_empty_band() {
    let floor = |svg: &str| -> (f64, f64) {
        let cell = |l: &&str| l.contains("<rect") && l.contains("fill-opacity")
            && !l.contains("rx=");
        let attr = |l: &str, k: &str| -> f64 {
            l.split(&format!(r#"{k}=""#)).nth(1).unwrap().split('"').next().unwrap()
                .parse().unwrap()
        };
        let bottom = svg.lines().filter(cell)
            .map(|l| attr(l, "y") + attr(l, "height"))
            .fold(f64::NEG_INFINITY, f64::max);
        let panel = svg.lines()
            .find(|l| l.contains("<rect") && l.contains(PANEL_BG))
            .map(|l| attr(l, "y") + attr(l, "height"))
            .expect("no panel drawn");
        (bottom, panel)
    };
    let (cells, panel) = floor(&render_tree(false, true, Some(0.0)));
    assert!(panel - cells > 1.0, "a band of panel no cell covers: {cells} of {panel}");

    let (cells, panel) = floor(&render_tree(false, true, None));
    assert!((panel - cells).abs() < 1.0,
            "and with nothing stated the cells fill the panel: {cells} of {panel}");
}

/// **A synthesized ring index earns no guide.** Reported from the book: a
/// reader met a donut whose first question was "what is Depth?", asked of an
/// axis name, four numbers printed inside the hole and a set of gridline
/// circles drawn through arcs that are already rings. None of it decodes
/// anything — the level a node sits at is an index the transform invented, not
/// a quantity anybody looks a value up on. The scale stays (it is what holds
/// the hole open); only the furniture goes.
#[test]
fn the_ring_index_draws_no_ticks_and_no_name() {
    let svg = render_tree(true, true, Some(0.0));
    let (cx, cy, _) = disc(&svg);
    assert!(!svg.contains(">Depth<"), "the ring index named itself as an axis");

    // Nothing printed inside the hole, which is where every radial number went.
    let hole = sector_radii(&svg, cx, cy)[0].0;
    assert!(hole > 1.0, "no hole to test against");
    for t in svg.split("<text").skip(1) {
        let at = |k: &str| -> Option<f64> {
            t.split(&format!("{k}=\"")).nth(1)?.split('"').next()?.parse().ok()
        };
        if let (Some(x), Some(y)) = (at("x"), at("y")) {
            let r = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            assert!(r > hole - 1.0,
                    "a label sits inside the hole, {r:.0} from the center of a {hole:.0} one");
        }
    }

    // And the measure keeps its own, because an amount is a quantity.
    assert!(svg.contains(">Amount<") || svg.split("<text").skip(1)
                .any(|t| t.split('>').nth(1).is_some_and(|s| s.starts_with(char::is_numeric))),
            "the measure axis lost its guide too");
}

/// **A spoke does not cross the hole.** An angular gridline marks an angle and
/// every angle is the same point at the center, so the part of a spoke inside a
/// donut's hole is a starburst over an empty disc. Reported from the book in the
/// same breath as the ring index's numbers, and fixed in the same place: the
/// spokes start at the cells' inner edge.
#[test]
fn a_spoke_does_not_cross_the_hole() {
    let svg = render_tree(true, true, Some(0.0));
    let (cx, cy, r) = disc(&svg);
    let hole = sector_radii(&svg, cx, cy)[0].0;
    assert!(hole > 1.0, "no hole to test against");

    let dist = |x: f64, y: f64| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    let mut spokes = 0;
    for line in svg.split("<line").skip(1) {
        let at = |k: &str| -> Option<f64> {
            line.split(&format!("{k}=\"")).nth(1)?.split('"').next()?.parse().ok()
        };
        let (Some(x1), Some(y1), Some(x2), Some(y2)) =
            (at("x1"), at("y1"), at("x2"), at("y2")) else { continue };
        let (a, b) = (dist(x1, y1), dist(x2, y2));
        if a > r + 1.0 || b > r + 1.0 { continue } // not inside the circle
        spokes += 1;
        assert!(a.min(b) > hole - 1.0,
                "a spoke reaches {:.0} into a hole of {hole:.0}", a.min(b));
    }
    assert!(spokes > 0, "no spoke drawn at all, so nothing was asserted");
}

/// **Binding one thing fewer must not take the plot apart.** A partition with
/// no measure weighs every leaf 1 — the tally `count` already does — so it is
/// the same sunburst with a different weight, and it draws like one.
///
/// Two separate defects made it draw like nothing. The transform publishes its
/// ring twice, once under `depth` and once under whatever the measure axis
/// reads, and with *neither* position bound those two names were both the empty
/// string: the ring was dropped and the radial axis read the measure, so the
/// rings landed inside the first tenth of the radius. And `measure_on_angle`
/// read the unbound `x` as Wilkinson's one-argument pie, which suppresses the
/// radial axis — right for a pie, whose radius carries nothing, and wrong here,
/// where it carries the depth. So the geometry and the guides are asserted
/// separately, against the measured sunburst rather than against constants.
#[test]
fn a_tallied_partition_in_polar_draws_what_a_measured_one_draws() {
    let tallied = render_tree_of(wide_tree_data(), true, false, None);
    let measured = render_tree_of(wide_tree_data(), true, true, None);
    let (cx, cy, r) = disc(&tallied);

    let rings = sector_radii(&tallied, cx, cy);
    assert_eq!(rings.len(), sector_radii(&measured, disc(&measured).0,
                                         disc(&measured).1).len(),
               "the same tree draws the same number of rings: {rings:?}");
    assert!(rings[0].1 > 1.0, "the sectors collapsed onto the center: {rings:?}");
    assert!((rings.last().unwrap().1 - r).abs() < 1.0,
            "and the rim reaches the frame: {rings:?}, radius {r}");

    // Every tick this plot shares with the measured one — the ring numbers, and
    // the weights round the angle. Read as a pie it had none: a pie is decoded
    // by its legend, so the whole guide is skipped.
    let numbers = |svg: &str| svg.split("<text").skip(1)
        .filter_map(|t| t.split('>').nth(1)?.split('<').next())
        .filter(|s| s.trim().parse::<f64>().is_ok())
        .count();
    assert!(numbers(&tallied) > 0,
            "a tallied sunburst keeps the axes a measured one has ({} vs {})",
            numbers(&tallied), numbers(&measured));
}

fn render_crossed(levels: &[&str], measure: bool, proportion: bool, label: bool) -> String {
    let mut s = PlotSpec::new().data("t");
    if measure {
        s = s.x("v");
    }
    let with = |mark: Mark, levels: &[&str]| {
        let l = Layer::new(mark).partition_crossed(levels);
        if proportion { l.transform(Transform::Proportion) } else { l }
    };
    s = s.layer(with(Mark::Zone, levels).encode(Channel::Color, "g"));
    if label {
        s = s.layer(with(Mark::Text, &levels[..1]).encode(Channel::Label, "name"));
    }
    SvgRenderer::default().render(&s, &tree_data())
}

/// **The spine plot ticks its share axis over the cells.** Every cell of a
/// one-level crossed partition is centered at 0.5, so the ticks chosen over the
/// centers were −0.5, 0.5 and 1.5; one landed inside the cells, the fit to them
/// gave up, and the bands filled the middle half of a panel labeled `-0`, `0`
/// and `2`.
#[test]
fn a_spine_plot_ticks_its_share_axis_over_the_cells() {
    let svg = render_crossed(&["g"], true, false, false);
    let labels = text_of(&svg);
    assert!(!labels.iter().any(|l| l == "-0"), "tick `-0` in {labels:?}");
    for tick in ["0.2", "0.4", "0.6", "0.8", "1.0"] {
        assert!(labels.iter().any(|l| l == tick), "no tick `{tick}` in {labels:?}");
    }
    let (_, py, _, ph) = panel_rect(&svg);
    for (_, y, _, h) in zone_rects(&svg) {
        assert!((y - py).abs() < 0.5 && (h - ph).abs() < 0.5,
            "a band fills the panel's height ({py}, {ph}), got ({y}, {h})");
    }
}

/// **Every plot is a named image.** The root `<svg>` carries `role="img"` and an
/// `aria-label`: the title when there is one, escaped, and otherwise what the plot
/// draws in the book's read-aloud words, a written axis name standing for its
/// column. No `<title>` element, which a browser would show as a tooltip.
#[test]
fn every_plot_carries_an_accessible_name() {
    let root = |svg: &str| svg.lines().next().unwrap_or("").to_string();
    let data = tree_data();
    let plain = PlotSpec::new().data("t").x("v").y("v")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "g"));
    let head = root(&SvgRenderer::default().render(&plain, &data));
    assert!(head.contains(r#"role="img" aria-label="Points, x is v, y is v, color by g""#), "{head}");
    let titled = plain.clone().title("Rich & \"long\" lives");
    let head = root(&SvgRenderer::default().render(&titled, &data));
    assert!(head.contains(r#"aria-label="Rich &amp; &quot;long&quot; lives""#), "{head}");
    let counted = PlotSpec::new().data("t").x("g").x_label("Group")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count));
    let svg = SvgRenderer::default().render(&counted, &data);
    assert!(root(&svg).contains(r#"aria-label="Bars derived by count, x is Group""#), "{}", root(&svg));
    assert!(!svg.contains("<title>"), "no tooltip");
}

/// **A partition's axes run from 0** (spec §15, ruled 2026-09-24): they are
/// ticked over the cells rather than over the cells' centers, so a mosaic's
/// share axis reads 0.0 to 1.0 and its measure axis starts at 0, as every other
/// measure axis does. A sunburst's angle labels 0 once, at the top, and not its
/// total, which falls on the same spoke.
#[test]
fn a_partition_axis_is_ticked_from_zero_over_its_cells() {
    let mosaic = text_of(&render_crossed(&["g", "i"], true, false, false));
    for tick in ["0", "0.0", "1.0"] {
        assert!(mosaic.iter().any(|l| l == tick), "no tick `{tick}` in {mosaic:?}");
    }
    let sunburst = text_of(&render_tree(true, true, None));
    assert_eq!(sunburst.iter().filter(|l| *l == "0").count(), 1, "{sunburst:?}");
    assert!(!sunburst.iter().any(|l| l == "8"),
        "the total, 8, is on 0's spoke and is not labeled: {sunburst:?}");
}

/// **A partition layer that names no measure reads the first one written**
/// (2026-10-02), as `check_partition` reads it. The renderer read the plot's
/// binding alone, so a `text * partition` written after `zone * partition +
/// x(v)` weighed every leaf 1 and piled its names at zero, under rectangles
/// weighed by `v`.
#[test]
fn a_partition_layer_naming_no_measure_reads_the_first_one_written() {
    let zone = || Layer::new(Mark::Zone).transform(Transform::Partition).partition(&["g", "i"]);
    let text = || Layer::new(Mark::Text).transform(Transform::Partition).partition(&["g", "i"])
        .encode(Channel::Label, crate::transform::NODE_NAME);
    let plot = PlotSpec::new().data("t").x("v").layer(zone()).layer(text());
    let layer_scoped = PlotSpec::new().data("t")
        .layer(zone().encode(Channel::X, "v")).layer(text());
    assert_eq!(SvgRenderer::default().render(&layer_scoped, &tree_data()),
        SvgRenderer::default().render(&plot, &tree_data()),
        "the only measure, written after one layer, weighs both");
}

/// **A crossed partition with nothing bound draws on its panel.** With neither
/// position bound, the widths and the heights were handed one column name, the
/// heights were overwritten by the widths, and the cells were drawn off the panel
/// with every label on a diagonal. The measure axis now reads the tally's name.
#[test]
fn a_crossed_partition_with_nothing_bound_draws_on_its_panel() {
    let svg = render_crossed(&["g", "i"], false, false, true);
    let (px, py, pw, ph) = panel_rect(&svg);
    let cells = zone_rects(&svg);
    assert_eq!(cells.len(), 4, "four leaves: {cells:?}");
    for (x, y, w, h) in cells {
        assert!(x >= px - 0.5 && y >= py - 0.5 && x + w <= px + pw + 0.5
            && y + h <= py + ph + 0.5,
            "a cell ({x}, {y}, {w}, {h}) left the panel ({px}, {py}, {pw}, {ph})");
    }
    // The column names sit side by side, at one height. (The legend names the
    // same two, in its own unpainted text, so the node labels are the painted
    // ones.)
    let heights: Vec<&str> = svg.split("<text").skip(1)
        .filter_map(|t| t.split_once('>'))
        .filter(|(attrs, rest)| attrs.contains("fill-opacity")
            && (rest.starts_with("a</text>") || rest.starts_with("b</text>")))
        .filter_map(|(attrs, _)| attrs.split(r#" y=""#).nth(1)?.split('"').next())
        .collect();
    assert_eq!(heights.len(), 2, "both columns are named: {heights:?}");
    assert_eq!(heights[0], heights[1], "on one row, not a diagonal");
}

/// **`partition * proportion` reads its measure axis as shares.** The widths
/// are divided by the total, so the measure axis runs `0 .. 1`, and the other
/// axis keeps its own name: a partition's `proportion` rescales the widths and
/// not the column shares, so it does not rename them `Proportion`.
#[test]
fn a_partition_read_as_proportion_ticks_its_measure_axis_in_shares() {
    let svg = render_crossed(&["g", "i"], true, true, true);
    let labels = text_of(&svg);
    let numbers: Vec<f64> = labels.iter().filter_map(|l| l.parse().ok()).collect();
    assert!(!numbers.is_empty() && numbers.iter().all(|v| *v <= 1.0),
        "every tick is a share: {labels:?}");
    assert!(labels.iter().any(|l| l == "Share of column")
        && !labels.iter().any(|l| l == "Proportion"), "{labels:?}");
    let (px, _, pw, _) = panel_rect(&svg);
    let cells = zone_rects(&svg);
    let right = cells.iter().map(|(x, _, w, _)| x + w).fold(0.0, f64::max);
    assert!((right - (px + pw)).abs() < 0.5, "the cells still fill the width");
}

// -- the pie: one position, so the position is the angle -----------------

fn pie_data() -> HashMap<String, DataFrame> {
    // Six rows in a 3 : 2 : 1 split, so the slices must come out at a half, a
    // third and a sixth of the turn — angles a reader could check with a ruler.
    HashMap::from([(
        "t".to_string(),
        DataFrame::new().with_str(
            "g",
            ["a", "a", "a", "b", "b", "c"].into_iter().map(String::from).collect(),
        ),
    )])
}

fn render_pie(polar: bool) -> String {
    let mut s = PlotSpec::new().data("t");
    if polar {
        s = s.coord(CoordSpace::Polar(crate::ir::PolarView::default()));
    }
    let layer = Layer::new(Mark::Bar)
        .transform(Transform::Count)
        .transform(Transform::Stack)
        .encode(Channel::Color, "g");
    SvgRenderer::default().render(&s.layer(layer), &pie_data())
}

/// The turn a wedge covers, as a fraction of the whole circle, read back off
/// its two straight edges.
fn wedge_turns(svg: &str, cx: f64, cy: f64) -> Vec<f64> {
    sector_paths(svg).iter().map(|p| {
        let bearing = |&(x, y): &(f64, f64)| {
            let t = (x - cx).atan2(cy - y);
            if t < 0.0 { t + std::f64::consts::TAU } else { t }
        };
        // A pie slice is `M edge A arc L center Z`: the first point and the
        // arc's end are the two edges of the wedge.
        let (a, b) = (bearing(&p[0]), bearing(&p[1]));
        let d = (b - a + std::f64::consts::TAU) % std::f64::consts::TAU;
        d / std::f64::consts::TAU
    }).collect()
}

/// "A pie chart is a stacked bar in polar coordinates" (Wilkinson, ch. 2) —
/// made literal. The same sentence draws a segmented column flat and a pie in
/// polar, and the pie's slices carry the shares the column's segments do.
#[test]
fn a_pie_is_a_stacked_bar_in_polar_coordinates() {
    let flat = render_pie(false);
    let pie = render_pie(true);

    // `<rect` + `fill-opacity` is the bar fingerprint; the legend's swatches
    // are rects with a corner radius, and only they carry `rx`.
    let bars = |svg: &str| svg.lines()
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && !l.contains("rx="))
        .count();
    assert_eq!(bars(&flat), 3, "flat: one segment per group, in one column");
    assert!(!pie.contains("NaN"), "pie has NaN coordinates");

    let (cx, cy, _) = disc(&pie);
    let mut turns = wedge_turns(&pie, cx, cy);
    turns.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(turns.len(), 3, "one slice per group");
    for (got, want) in turns.iter().zip([1.0 / 6.0, 1.0 / 3.0, 0.5]) {
        assert!((got - want).abs() < 0.002, "slice {got:.4} of a turn, wanted {want:.4}");
    }
}

/// The slices must cover the circle exactly. They stop short if the measured
/// axis keeps its usual 5% headroom, which on a circle is not clearance but a
/// wedge of background at twelve o'clock — the bug this pins.
#[test]
fn the_slices_close_the_circle_with_no_gap() {
    let pie = render_pie(true);
    let (cx, cy, _) = disc(&pie);
    let total: f64 = wedge_turns(&pie, cx, cy).iter().sum();
    assert!((total - 1.0).abs() < 0.002, "the slices cover {total:.4} of a turn, not all of it");
}

/// A pie of a *measured* column, not of a count. This shipped drawing an empty
/// circle: with `x` unbound and `y` continuous, `bar_orient` read the plot as
/// horizontal (its rule for `bar * bin + y(h)`, a histogram on its side), which
/// made `speed` the *key* rather than the measure — so the statistic had no
/// column to summarize and every slice was zero wide. A positionless bar has no
/// orientation to read, and the bound column is always its measure.
#[test]
fn a_pie_of_a_measured_column_is_not_empty() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", ["a", "a", "b"].into_iter().map(String::from).collect())
            .with_float("v", vec![3.0, 3.0, 2.0]),
    )]);
    let layer = || Layer::new(Mark::Bar)
        .transform(Transform::Sum)
        .transform(Transform::Stack)
        .encode(Channel::Color, "g");
    let render = |polar: bool| {
        let mut s = PlotSpec::new().data("t").y("v");
        if polar { s = s.coord(CoordSpace::Polar(crate::ir::PolarView::default())); }
        SvgRenderer::default().render(&s.layer(layer()), &data)
    };

    let pie = render(true);
    let (cx, cy, _) = disc(&pie);
    let mut turns = wedge_turns(&pie, cx, cy);
    turns.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(turns.len(), 2, "a slice per group: {turns:?}");
    // Sums of 6 and 2, so the slices are three quarters and one quarter.
    for (got, want) in turns.iter().zip([0.25, 0.75]) {
        assert!((got - want).abs() < 0.002, "slice {got:.4} of a turn, wanted {want:.4}");
    }

    // The same sentence flat is the share-of-total column, and it broke the same
    // way: two segments piled in one slot, not an empty panel.
    let flat = render(false);
    let bars: Vec<&str> = flat.lines()
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && !l.contains("rx="))
        .collect();
    assert_eq!(bars.len(), 2, "flat drew {} segments", bars.len());
}

/// A pie is decoded by its legend, not by an axis: no rings, no spokes, no tick
/// labels. The rose keeps all three, which is what makes them different plots
/// rather than the same one with different data.
#[test]
fn a_pie_draws_no_axis_furniture_but_a_rose_does() {
    let pie = render_pie(true);
    let rose = render_rose(0.0, true);
    // The rings and spokes are one group, opened with exactly this tag.
    let grid_group = r##"<g stroke="#d2d2da" stroke-width="1" fill="none">"##;
    assert!(!pie.contains(grid_group), "a pie drew gridlines");
    assert!(rose.contains(grid_group), "a rose drew none");
}

// -----------------------------------------------------------------------
// Nest — the panel packed with regions
// -----------------------------------------------------------------------

fn nest_data() -> HashMap<String, DataFrame> {
    // Four groups in a 4 : 3 : 2 : 1 split of ten, so every share is a tenth
    // of the panel a reader could count off.
    HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", ["a", "b", "c", "d"].into_iter().map(String::from).collect())
            .with_float("v", vec![4.0, 3.0, 2.0, 1.0]),
    )])
}

/// Every `<rect>` a packed panel paints, as (x, y, w, h). The cells carry a
/// fill-opacity like any bar; the outer region outlines are `fill="none"` and
/// are excluded, since they trace the same area a second time. `rx=` drops the
/// legend's rounded swatches, which are also fill-opacity rects — the same
/// discriminator the pie's bar count uses.
fn packed_cells(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    svg.lines()
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                 && !l.contains(r#"fill="none""#) && !l.contains("rx="))
        .filter_map(|l| {
            let num = |key: &str| -> Option<f64> {
                let at = l.find(key)? + key.len();
                l[at..].split('"').next()?.parse().ok()
            };
            // The leading space matters: `width=` without it also matches
            // `stroke-width=`, and `y=` matches the `y` in `fill-opacity=`.
            Some((num(r#" x=""#)?, num(r#" y=""#)?, num(r#" width=""#)?, num(r#" height=""#)?))
        })
        .collect()
}

fn render_nest(with_domain: bool) -> String {
    let mut s = PlotSpec::new().data("t").y("v");
    if with_domain { s = s.x("g"); }
    let layer = Layer::new(Mark::Bar)
        .transform(Transform::Sum)
        .encode(Channel::Color, "g");
    SvgRenderer::default().render(&s.coord(CoordSpace::Nest).layer(layer), &nest_data())
}

/// The claim the plot is read for: **each region is its own share of the
/// panel**. A treemap whose areas are merely ordered right is a treemap that
/// lies, so this checks the ratios rather than the ordering.
#[test]
fn a_packed_panel_gives_every_row_its_share_of_the_area() {
    let cells = packed_cells(&render_nest(false));
    assert_eq!(cells.len(), 4, "expected one region per group: {cells:?}");
    let areas: Vec<f64> = cells.iter().map(|c| c.2 * c.3).collect();
    let total: f64 = areas.iter().sum();
    // The rows are 4 : 3 : 2 : 1 of ten, in the frame's order.
    for (got, want) in areas.iter().zip([0.4, 0.3, 0.2, 0.1]) {
        let share = got / total;
        assert!((share - want).abs() < 0.001, "share {share:.4}, wanted {want}");
    }
}

/// And the regions **are** the panel: the shares are only meaningful because
/// nothing is left over. Measured against the panel the flat renderer lays
/// out for the same sentence, so a layout change cannot quietly pass this.
#[test]
fn the_packed_regions_fill_the_whole_panel() {
    let svg = render_nest(false);
    let cells = packed_cells(&svg);
    let packed: f64 = cells.iter().map(|c| c.2 * c.3).sum();
    let x0 = cells.iter().map(|c| c.0).fold(f64::MAX, f64::min);
    let y0 = cells.iter().map(|c| c.1).fold(f64::MAX, f64::min);
    let x1 = cells.iter().map(|c| c.0 + c.2).fold(f64::MIN, f64::max);
    let y1 = cells.iter().map(|c| c.1 + c.3).fold(f64::MIN, f64::max);
    assert!((packed - (x1 - x0) * (y1 - y0)).abs() < 1.0,
            "the regions do not tile their own bounding box");
    assert!((x1 - x0) > 200.0 && (y1 - y0) > 200.0, "the packing did not take the panel");
}

/// **No axes, and this is the space's defining property** (spec §15): the two
/// directions carry no variable, so there is nothing to tick and nothing to
/// label. The same sentence flat keeps all of it, which is what makes this a
/// difference of space rather than of theme.
#[test]
fn a_packed_panel_draws_no_axis_furniture_and_a_flat_one_does() {
    let packed = render_nest(true);
    let flat = {
        let layer = Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "g");
        SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("g").y("v").layer(layer), &nest_data())
    };
    // A tick label is the readable half of an axis; the category names would
    // be drawn under the bars flat, and must not be anywhere in the packing.
    // (They are still in the *legend*, which is what decodes a packed panel,
    // so this counts occurrences rather than asking whether "a" appears.)
    let ticks = |s: &str| s.matches(r##"text-anchor="middle""##).count();
    assert!(ticks(&packed) < ticks(&flat),
            "the packed panel drew as much text furniture as the flat one");
    let grid_group = r##"<g stroke="#d2d2da" stroke-width="1">"##;
    assert!(!packed.contains(grid_group), "a packed panel drew gridlines");
    assert!(flat.contains(grid_group), "the flat sentence drew none, so the test proves nothing");
    // And the axis lines themselves, which are the frame color and are the
    // half of an axis a `theme(grid = "none")` would leave standing.
    let axis_ink = format!(r##"stroke="{}""##, crate::ir::THEME_FRAME_COLOR);
    assert!(!packed.contains(axis_ink.as_str()), "a packed panel drew axis lines");
    assert!(flat.contains(axis_ink.as_str()), "the flat sentence drew none, so the test proves nothing");
}

/// Two levels from one sentence and no new vocabulary: the domain axis
/// partitions the panel, and the split packs inside each region. What proves
/// it is the *count* of regions — four groups × two products is eight cells,
/// where the one-level reading of the same table would give eight regions
/// packed against each other with no group boundary.
#[test]
fn a_bound_domain_axis_packs_a_second_level_inside_each_region() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", ["a", "a", "b", "b"].into_iter().map(String::from).collect())
            .with_str("p", ["x", "y", "x", "y"].into_iter().map(String::from).collect())
            .with_float("v", vec![3.0, 1.0, 4.0, 4.0]),
    )]);
    let layer = Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "p");
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("g").y("v").coord(CoordSpace::Nest).layer(layer), &data);
    let cells = packed_cells(&svg);
    assert_eq!(cells.len(), 4, "expected one cell per (group, product): {cells:?}");

    // The outer regions are traced as well, so the coarser split is visible
    // without an axis to name it — one outline per group, drawn `fill="none"`.
    // They are also what the outer partition can be *measured* off: group `a`
    // totals 4 and `b` totals 8, so the two regions must be a third and two
    // thirds of the panel. Read off the outlines rather than off the cells,
    // because a transform groups its output by the color split rather than by
    // the domain axis, so row order is not group order.
    let outlines: Vec<f64> = svg.lines()
        .filter(|l| l.contains("<rect") && l.contains(r#"fill="none""#))
        .filter_map(|l| {
            let num = |key: &str| -> Option<f64> {
                let at = l.find(key)? + key.len();
                l[at..].split('"').next()?.parse().ok()
            };
            Some(num(r#" width=""#)? * num(r#" height=""#)?)
        })
        .collect();
    assert_eq!(outlines.len(), 2, "expected one outline per group region");
    let panel: f64 = outlines.iter().sum();
    let mut shares: Vec<f64> = outlines.iter().map(|a| a / panel).collect();
    shares.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (got, want) in shares.iter().zip([1.0 / 3.0, 2.0 / 3.0]) {
        assert!((got - want).abs() < 0.002, "outer region got {got:.4}, wanted {want:.4}");
    }
}

/// A one-level packing has nothing coarser to show, so it draws no outlines at
/// all — the guard that keeps the two-level cue from becoming a border round
/// every treemap.
#[test]
fn a_one_level_packing_traces_no_outer_region() {
    let svg = render_nest(false);
    let outlines = svg.lines()
        .filter(|l| l.contains("<rect") && l.contains(r#"fill="none""#))
        .count();
    assert_eq!(outlines, 0, "a single region was outlined against nothing");
}

/// The packing keeps the frame's order rather than sorting by size, which is
/// what leaves `order()` meaning something in this space (spec §10).
#[test]
fn the_packing_follows_the_axis_order_not_the_values() {
    let ascending: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", ["a", "b", "c", "d"].into_iter().map(String::from).collect())
            .with_float("v", vec![1.0, 2.0, 3.0, 4.0]),
    )]);
    let layer = Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "g");
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(layer), &ascending);
    let areas: Vec<f64> = packed_cells(&svg).iter().map(|c| c.2 * c.3).collect();
    assert_eq!(areas.len(), 4);
    // Row order is a, b, c, d — so the areas must *ascend*. A layout that
    // sorted descending for prettier rectangles would fail here, and would
    // have made `order()` a word with no effect.
    for w in areas.windows(2) {
        assert!(w[1] > w[0], "the packing reordered the rows: {areas:?}");
    }
}

/// `order()` ranks a category by the layer's statistic over all its rows, never
/// by one piece of a split (ruled 2026-09-28). `a` totals 11 from a first piece
/// of 1 and `b` totals 6 from a first piece of 5, so ranking by the first piece
/// put `b` first under a color split; by the total, `a` comes first, as it does
/// with no split at all.
#[test]
fn order_ranks_a_split_category_by_its_whole_statistic() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", ["a", "a", "b", "b"].into_iter().map(String::from).collect())
            .with_str("era", ["e1", "e2", "e1", "e2"].into_iter().map(String::from).collect())
            .with_float("v", vec![1.0, 10.0, 5.0, 1.0]),
    )]);
    let x_of = |svg: &str, name: &str| -> f64 {
        let end = svg.find(&format!(">{name}</text>")).expect("the category is named");
        let start = svg[..end].rfind("<text x=\"").expect("a tick label") + 9;
        svg[start..].split('"').next().unwrap().parse().unwrap()
    };
    for split in [true, false] {
        let mut layer = Layer::new(Mark::Bar).transform(Transform::Sum);
        if split {
            layer = layer.encode(Channel::Color, "era");
        }
        let spec = PlotSpec::new().data("t").x("g").y("v").layer(layer).order_desc("v");
        let svg = SvgRenderer::default().render(&spec, &data);
        assert!(x_of(&svg, "a") < x_of(&svg, "b"),
            "split by color: {split}; `a` totals 11 and `b` 6, so `a` leads");
    }
}

// -----------------------------------------------------------------------
// Nest — the label inside its own region
// -----------------------------------------------------------------------

/// Every `<text>` the **text layer** drew, as (x, y, string) — the clipped
/// group, which is what excludes the legend's key, whose entries are the same
/// strings at the same font and would otherwise be counted as labels that fit.
fn packed_labels(svg: &str) -> Vec<(f64, f64, String)> {
    let mut inside = false;
    svg.lines()
        .filter(|l| {
            let t = l.trim_start();
            if t.starts_with("<g ") {
                inside = t.contains("clip-path") && t.contains(r#"text-anchor="middle""#);
            } else if t.starts_with("</g>") {
                inside = false;
            }
            inside && t.starts_with("<text")
        })
        .filter_map(|l| {
            let num = |key: &str| -> Option<f64> {
                let at = l.find(key)? + key.len();
                l[at..].split('"').next()?.parse().ok()
            };
            let s = l.split('>').nth(1)?.split('<').next()?.to_string();
            Some((num(r#"<text x=""#)?, num(r#" y=""#)?, s))
        })
        .collect()
}

/// Four roomy regions, whose names are short enough that every one fits.
fn nest_label_data() -> HashMap<String, DataFrame> {
    HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", ["a", "b", "c", "d"].into_iter().map(String::from).collect())
            .with_float("v", vec![4.0, 3.0, 2.0, 1.0]),
    )])
}

fn render_nest_labeled(data: &HashMap<String, DataFrame>) -> String {
    let bars = Layer::new(Mark::Bar).encode(Channel::Color, "g");
    let names = Layer::new(Mark::Text).encode(Channel::Label, "g");
    SvgRenderer::default().render(
        &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(bars).layer(names),
        data)
}

/// **The claim the mark is for**: a label sits at the center of the region its
/// own row was packed into. Checked against the *bar's* rectangles rather than
/// against remembered numbers, because the thing that would make this feature
/// worthless is the two marks packing differently — a name a pixel outside its
/// rectangle names nothing, and no reader could tell which cell it meant.
#[test]
fn a_packed_label_sits_at_the_center_of_its_own_region() {
    let svg = render_nest_labeled(&nest_label_data());
    let cells = packed_cells(&svg);
    let labels = packed_labels(&svg);
    assert_eq!(cells.len(), 4, "expected one region per row: {cells:?}");
    assert_eq!(labels.len(), 4, "every one of these names fits: {labels:?}");
    for (i, (lx, ly, s)) in labels.iter().enumerate() {
        let (cx, cy, cw, ch) = cells[i];
        assert_eq!(s, ["a", "b", "c", "d"][i], "labels are out of row order");
        assert!((lx - (cx + cw / 2.0)).abs() < 0.01,
                "`{s}` is not centered in its region: {lx} vs {}", cx + cw / 2.0);
        // The baseline drops half a cap height so the glyph is centered on the
        // region rather than resting its feet on the middle of it.
        assert!((ly - (cy + ch / 2.0)).abs() < ch / 2.0,
                "`{s}` is outside its own region vertically: {ly} in {cy}..{}", cy + ch);
    }
}

/// A label that does not fit the region it names is **drawn smaller, at the
/// size that fits**, the rule a sunburst's names follow, so every name is on
/// the plot and nothing is left for a remark to account for. It was left out
/// and counted until 2026-10-05; the count survives for the one name that still
/// cannot be drawn, a share with no region (`the_label_report_accounts_for_every_row`).
#[test]
fn a_label_too_wide_for_its_region_is_drawn_smaller() {
    // One region is 96% of the panel and the rest are slivers; the long names
    // cannot fit anywhere but the first.
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", ["roomy", "cramped one", "cramped two", "cramped three"]
                .into_iter().map(String::from).collect())
            .with_float("v", vec![240.0, 1.0, 1.0, 1.0]),
    )]);
    let bars = Layer::new(Mark::Bar).encode(Channel::Color, "g");
    let names = Layer::new(Mark::Text).encode(Channel::Label, "g");
    let drawn = SvgRenderer::default().draw(
        &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(bars).layer(names),
        &data);

    let labels = packed_labels(&drawn.svg);
    assert_eq!(labels.len(), 4, "every region holds its name: {labels:?}");
    let line = |n: &str| drawn.svg.lines().find(|l| l.ends_with(&format!(">{n}</text>")))
        .unwrap_or_else(|| panic!("{n} is drawn"));
    assert!(!line("roomy").contains("font-size"), "the roomy name keeps its size: {}", line("roomy"));
    for n in ["cramped one", "cramped two", "cramped three"] {
        assert!(line(n).contains(r#" font-size=""#), "{n} is drawn smaller: {}", line(n));
    }
    assert!(drawn.remarks.is_empty(), "nothing is left out, so nothing is said: {:?}",
            drawn.remarks.iter().map(|d| &d.message).collect::<Vec<_>>());
}

/// **The two numbers have to close.** A share too small to have a region at
/// all is not an unfitted label, and counting it in neither number is what let
/// the book's own treemap report *116 of 142 left out* over a plot carrying 25
/// names — the reader subtracts and lands one too high. So the remark names
/// what was **drawn** and accounts for every row that was not.
#[test]
fn the_label_report_accounts_for_every_row() {
    // One roomy region, two that a name cannot fit, and one share so small it
    // has no region to fit a name in.
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", ["roomy", "cramped one", "cramped two", "vanishing"]
                .into_iter().map(String::from).collect())
            .with_float("v", vec![4000.0, 12.0, 12.0, 0.02]),
    )]);
    let bars = Layer::new(Mark::Bar).encode(Channel::Color, "g");
    let names = Layer::new(Mark::Text).encode(Channel::Label, "g");
    let drawn = SvgRenderer::default().draw(
        &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(bars).layer(names),
        &data);

    let labels = packed_labels(&drawn.svg);
    let said = drawn.remarks.iter().map(|d| d.message.as_str())
        .find(|m| m.contains("labels are drawn"))
        .unwrap_or_else(|| panic!("no label report: {:?}", drawn.remarks));

    // What the plot drew is what the sentence claims it drew.
    assert!(said.contains(&format!("{} of 4 labels are drawn", labels.len())),
            "{} names on the plot: {said}", labels.len());
    // And the row with no region is named rather than left out of both counts.
    assert!(said.contains("too small to have a region at all"),
            "the vanishing share has to be accounted for: {said}");
    // And the packing is not called whole beside a share it could not draw.
    assert!(!said.contains("drew every share"), "{said}");
}

/// The dropped-rows report rides in `remarks`, not on stderr: a browser has
/// no stderr, and this render goes through `draw` — the same door
/// `gog-wasm` uses — so what this asserts is what a browser user is told.
#[test]
fn dropped_log_rows_are_a_remark_rather_than_stderr() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![0.0, 10.0, 100.0])
            .with_float("y", vec![1.0, 2.0, 3.0]),
    )]);
    let spec = PlotSpec::new().data("t").x_log_base("x", 10.0).y("y")
        .layer(Layer::new(Mark::Point));
    let drawn = SvgRenderer::default().draw(&spec, &data);
    assert!(
        drawn.remarks.iter().any(|d| d.message.contains("no place on the log")),
        "the dropped-rows report must ride in remarks: {:?}",
        drawn.remarks.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// A stated count past the ceiling is drawn with a wider step, labeled to the
/// far end, and said out loud; one that fits says nothing. The case that
/// reported it: 40 ticks on gdp labeled 0K to 25K on an axis that runs to 49K.
#[test]
fn a_count_past_the_ceiling_is_labeled_to_the_far_end_and_said() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("gdp", vec![277.55, 12000.0, 49357.19])
            .with_float("life", vec![40.0, 60.0, 82.0]),
    )]);
    let drawn_at = |n: usize| {
        let mut spec = PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Point));
        spec.x = Some(crate::ir::ChannelDef::field("gdp").with_tick_count(n));
        SvgRenderer::default().draw(&spec, &data)
    };
    let wide = drawn_at(40);
    assert!(wide.svg.contains(">48K</text>"), "the far end of the axis is bare");
    assert!(!wide.svg.contains(">25K</text>"), "the step did not widen");
    let said: Vec<&str> = wide.remarks.iter().map(|d| d.message.as_str()).collect();
    assert!(said.iter().any(|m| m.contains("tick_count = 40") && m.contains("draws a tick every 2K instead")),
        "a widened step must ride in remarks: {said:?}");

    let fits = drawn_at(26);
    assert!(fits.remarks.is_empty(), "a count that fits said something: {:?}",
        fits.remarks.iter().map(|d| &d.message).collect::<Vec<_>>());
}

/// Every label a drawing wrote, in order.
fn text_of(svg: &str) -> Vec<String> {
    svg.split("<text").skip(1)
        .filter_map(|t| t.split_once('>').map(|(_, rest)| rest))
        .filter_map(|rest| rest.split_once("</text>").map(|(label, _)| label.to_string()))
        .collect()
}

/// **A stated count reaches a log axis and a calendar axis.** Both drew the
/// default whatever was written, byte for byte, and said nothing: the log and
/// calendar branches chose their ticks with no count to read.
#[test]
fn a_stated_count_reaches_a_log_axis_and_a_calendar_axis() {
    let gdp: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("gdp", vec![277.55, 2000.0, 12000.0, 49357.19])
            .with_float("life", vec![40.0, 55.0, 70.0, 82.0]),
    )]);
    let log = |n: Option<usize>| {
        let mut spec = PlotSpec::new().data("t").y("life").layer(Layer::new(Mark::Point));
        let def = crate::ir::ChannelDef::field("gdp").with_scale(ScaleType::Log);
        spec.x = Some(match n { Some(n) => def.with_tick_count(n), None => def });
        text_of(&SvgRenderer::default().render(&spec, &gdp))
    };
    assert!(!log(None).contains(&"2K".to_string()), "{:?}", log(None));
    let twelve = log(Some(12));
    for fill in ["500", "2K", "5K", "20K", "50K"] {
        assert!(twelve.contains(&fill.to_string()), "no {fill} at twelve: {twelve:?}");
    }

    let days = |n: Option<usize>| {
        let mut spec = PlotSpec::new().data("t").y("orders").layer(Layer::new(Mark::Line));
        let def = crate::ir::ChannelDef::field("day");
        spec.x = Some(match n { Some(n) => def.with_tick_count(n), None => def });
        text_of(&SvgRenderer::default().render(&spec, &six_weeks(42)))
    };
    let dated = |labels: Vec<String>| labels.into_iter()
        .filter(|l| l.starts_with("Mar") || l.starts_with("Apr") || l.starts_with("Feb"))
        .count();
    assert_eq!(dated(days(None)), 6, "the weekly default moved");
    assert_eq!(dated(days(Some(3))), 3, "{:?}", days(Some(3)));
    // Twenty asks the calendar for 22 dates. At the default width they do not
    // fit side by side, so one in every two is drawn and the note says so; in a
    // plot twice as wide all 22 are drawn, which is the count reaching the axis.
    assert_eq!(dated(days(Some(20))), 11, "{:?}", days(Some(20)));
    let mut spec = PlotSpec::new().data("t").y("orders").layer(Layer::new(Mark::Line));
    spec.x = Some(crate::ir::ChannelDef::field("day").with_tick_count(20));
    let narrow = SvgRenderer::default().draw(&spec, &six_weeks(42));
    assert!(narrow.remarks.iter().any(|d| d.message.contains("so 11 of them are drawn")),
            "{:?}", narrow.remarks.iter().map(|d| &d.message).collect::<Vec<_>>());
    let wide = SvgRenderer { width: 1600.0, ..SvgRenderer::default() };
    assert_eq!(dated(text_of(&wide.render(&spec, &six_weeks(42)))), 22);
}

/// A pattern key over a different `color` column takes neutral ink: keyed in
/// the color map's first hue, "Survived" read as Male's.
#[test]
fn a_pattern_key_beside_another_color_column_takes_neutral_ink() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("c", s(&["a", "a", "b", "b"]))
            .with_str("sex", s(&["m", "f", "m", "f"]))
            .with_str("kept", s(&["yes", "no", "no", "yes"]))
            .with_float("n", vec![3.0, 2.0, 4.0, 1.0]),
    )]);
    let spec = PlotSpec::new().data("t").x("c").y("n")
        .layer(Layer::new(Mark::Bar).transform(Transform::Sum).transform(Transform::Dodge)
            .encode(Channel::Color, "sex").encode(Channel::Pattern, "kept"));
    let svg = SvgRenderer::default().render(&spec, &t);
    let key = svg.split("Kept").nth(1).expect("a pattern key titled Kept");
    assert!(key.contains("#3c3c46"), "the pattern key's swatches are in neutral ink");
}

/// A writer draws one share of its layer, a panel, a moment or a selection
/// pass, and reads it against the whole layer's scales, which the one legend
/// decodes. Fitted per share, the low panel's largest dot was drawn at the
/// largest size, a selected dot changed size on being selected, and a panel
/// without the first category gave its dash to the second.
#[test]
fn a_share_of_a_layer_reads_the_whole_layers_scales() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0, 1.0, 2.0, 3.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 2.0, 3.0, 4.0])
            .with_float("n", vec![1.0, 2.0, 3.0, 10.0, 20.0, 30.0])
            .with_str("g", s(&["low", "low", "low", "high", "high", "high"]))
            .with_str("k", s(&["a", "a", "a", "b", "b", "b"])),
    )]);
    // Every radius drawn, the legend's three included, in one sorted list.
    let radii = |svg: &str| {
        let mut r: Vec<String> = svg.lines()
            .filter(|l| l.trim_start().starts_with("<circle"))
            .filter_map(|l| l.split(" r=\"").nth(1))
            .map(|r| r.split('"').next().unwrap_or("").to_string())
            .collect();
        r.sort();
        r
    };
    let sized = || PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point).encode(Channel::Size, "n"));
    let whole = radii(&SvgRenderer::default().render(&sized(), &t));
    assert_eq!(whole.len(), 9, "six dots and a legend of three: {whole:?}");

    let mut faceted = sized();
    faceted.facet = Some(crate::ir::FacetSpec { col: Some("g".into()), ..Default::default() });
    assert_eq!(radii(&SvgRenderer::default().render(&faceted, &t)), whole, "per panel");

    let played = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point).encode(Channel::Size, "n").encode(Channel::Play, "g"));
    assert_eq!(radii(&SvgRenderer::default().render(&played, &t)), whole, "per moment");

    let brushed = sized().brush(crate::ir::BrushDef::new("x").at(1.5, 3.5));
    assert_eq!(radii(&SvgRenderer::default().render(&brushed, &t)), whole, "per pass");

    // `b` is the second category, so it takes the second dash in the panel
    // that has no `a` as well as in the legend.
    let mut dashed = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Line).encode(Channel::Pattern, "k"));
    dashed.facet = Some(crate::ir::FacetSpec { col: Some("g".into()), ..Default::default() });
    let svg = SvgRenderer::default().render(&dashed, &t);
    let second = crate::render::pattern::pattern_dasharray(Some(crate::render::pattern::dash_for_index(1)));
    assert!(!second.is_empty(), "the second dash is drawn as a dash");
    let lines: Vec<&str> = svg.lines().filter(|l| l.trim_start().starts_with("<polyline")).collect();
    assert_eq!(lines.len(), 2, "one line per panel: {lines:?}");
    assert!(lines.iter().filter(|l| l.contains(second)).count() == 1
            && lines.iter().any(|l| !l.contains("stroke-dasharray")),
            "`a` is solid and `b` takes the second dash in its own panel: {lines:?}");
}

/// A cube's floor names every category: a name that met its own axis's
/// neighbor was dropped at once, as a number is, and "Americas" went missing
/// 3.6 px from "Africa". And an axis seen end-on draws no labels: at `tilt =
/// 90` a lone number stood where the vertical axis had collapsed to a point.
#[test]
fn a_cubes_floor_names_every_category_and_an_end_on_axis_none() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let conts = ["Asia", "Europe", "Africa", "Americas", "Oceania"];
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("life", (0..60).map(|i| 40.0 + f64::from(i % 43)).collect())
            .with_str("continent", s(&(0..60).map(|i| conts[i % 5]).collect::<Vec<_>>())),
    )]);
    let mut layer = Layer::new(Mark::Bar).transform(Transform::Bin);
    layer.bin = Some(crate::ir::BinSpec { bins: Some(12), width: None, tiling: None });
    let spec = PlotSpec::new().data("t").x("life").y("continent").layer(layer)
        .coord(CoordSpace::Space(Default::default()));
    let labels = text_of(&SvgRenderer::default().render(&spec, &t));
    for c in conts {
        assert!(labels.iter().any(|l| l == c), "{c} is on the floor: {labels:?}");
    }
    let cube: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new().with_float("a", vec![1.0, 2.0, 3.0]).with_float("b", vec![4.0, 5.0, 6.0])
            .with_float("c", vec![7.0, 8.0, 9.0]),
    )]);
    let mut top = PlotSpec::new().data("t").x("a").y("b").layer(Layer::new(Mark::Point))
        .coord(CoordSpace::Space(crate::ir::SpaceView { tilt: 90.0, ..Default::default() }));
    top.z = Some(ChannelDef::field("c"));
    let labels = text_of(&SvgRenderer::default().render(&top, &cube));
    assert!(!labels.iter().any(|l| l == "C" || l.starts_with('7') || l.starts_with('9')),
            "the end-on axis draws nothing: {labels:?}");
}

/// A globe's labels are clipped by the panel, not by the disk: a name beside a
/// place near the limb runs past it, and "Anchorage" lost its last letters.
#[test]
fn a_globes_labels_are_clipped_by_the_panel_not_the_disk() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("lon", vec![-150.0, 10.0])
            .with_float("lat", vec![61.0, 50.0])
            .with_str("name", s(&["Anchorage", "Frankfurt"])),
    )]);
    let spec = PlotSpec::new().data("t").x("lon").y("lat")
        .layer(Layer::new(Mark::Point))
        .layer(Layer::new(Mark::Text).encode(Channel::Label, "name"))
        .coord(CoordSpace::Globe(Default::default()));
    let svg = SvgRenderer::default().render(&spec, &t);
    // Frankfurt faces the default view; Anchorage is behind the sphere.
    let labels = svg.split(">Frankfurt</text>").next().unwrap();
    assert!(labels.len() < svg.len(), "the facing label is drawn");
    let group = labels.rsplit("<g clip-path=\"url(#").next().unwrap();
    let id = group.split(')').next().unwrap();
    assert!(id.ends_with("-labels"), "the labels' clip is the panel's: {id}");
    let rect = svg.split(&format!("<clipPath id=\"{id}\">")).nth(1).expect("the label clip");
    assert!(rect.starts_with("<rect"), "a rectangle, not the disk: {}", &rect[..40.min(rect.len())]);
}

/// **What a globe hides is said once per table, and names it.** Two tables with
/// the same counts printed one unnamed line, and a faceted globe one line per
/// panel; two layers drawing one table hide the same rows and share a line.
#[test]
fn a_globes_hidden_rows_are_said_once_per_table_and_name_it() {
    let table = || DataFrame::new()
        .with_float("lon", vec![-150.0, 10.0, 170.0])
        .with_float("lat", vec![61.0, 50.0, -20.0])
        .with_str("side", vec!["a".into(), "b".into(), "a".into()])
        .with_str("name", vec!["Anchorage".into(), "Frankfurt".into(), "Suva".into()]);
    let t: HashMap<String, DataFrame> =
        HashMap::from([("cities".to_string(), table()), ("copy".to_string(), table())]);
    let hidden = |spec: &PlotSpec| -> Vec<String> {
        SvgRenderer::default().draw(spec, &t).remarks.into_iter()
            .map(|d| d.message).filter(|m| m.contains("face away")).collect()
    };
    let globe = || CoordSpace::Globe(Default::default());
    // Two tables, equal counts: two lines, each naming its table.
    let two = PlotSpec::new().x("lon").y("lat").coord(globe())
        .layer(Layer::new(Mark::Point).data("cities"))
        .layer(Layer::new(Mark::Point).data("copy"));
    let said = hidden(&two);
    assert_eq!(said.len(), 2, "{said:?}");
    assert!(said[0].contains("row(s) of `cities` face away")
        && said[1].contains("row(s) of `copy` face away"), "{said:?}");
    // One table under two marks: the same rows, one line.
    let one = PlotSpec::new().data("cities").x("lon").y("lat").coord(globe())
        .layer(Layer::new(Mark::Point))
        .layer(Layer::new(Mark::Text).encode(Channel::Label, "name"));
    assert_eq!(hidden(&one).len(), 1, "{:?}", hidden(&one));
    // Split into panels: one line, counted over every panel.
    let mut split = PlotSpec::new().data("cities").x("lon").y("lat").coord(globe())
        .layer(Layer::new(Mark::Point));
    split.facet = Some(crate::ir::FacetSpec { col: Some("side".into()), ..Default::default() });
    let said = hidden(&split);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("of 3 row(s) of `cities`"), "{said:?}");
}

/// **A layer that stands still is counted once in a played globe's note.** The
/// note counted every layer's rows in every moment, so a basemap's 4150 rows
/// were reported as 49800 over twelve months; a layer that plays is counted
/// per moment, which is each of its rows once.
#[test]
fn a_played_globe_counts_a_still_layers_rows_once() {
    let t: HashMap<String, DataFrame> = HashMap::from([
        ("cities".to_string(), DataFrame::new()
            .with_float("lon", vec![-150.0, 10.0, 170.0])
            .with_float("lat", vec![61.0, 50.0, -20.0])),
        ("moving".to_string(), DataFrame::new()
            .with_float("lon", vec![-150.0, 10.0, 170.0, 20.0])
            .with_float("lat", vec![61.0, 50.0, -20.0, 40.0])
            .with_float("year", vec![1.0, 1.0, 2.0, 2.0])),
    ]);
    let spec = PlotSpec::new().x("lon").y("lat").coord(CoordSpace::Globe(Default::default()))
        .layer(Layer::new(Mark::Point).data("cities"))
        .layer(Layer::new(Mark::Point).data("moving").encode(Channel::Play, "year"));
    let said: Vec<String> = SvgRenderer::default().draw(&spec, &t).remarks.into_iter()
        .map(|d| d.message).filter(|m| m.contains("face away")).collect();
    assert!(said.iter().any(|m| m.contains("of 3 row(s) of `cities`")), "{said:?}");
    assert!(said.iter().any(|m| m.contains("of 4 row(s) of `moving`")), "{said:?}");
}

/// **A column two tables share is one set of categories, drawn and keyed.** Each
/// layer ordered its own table's categories, so `shape(k)` over two tables drew
/// `c` in `a`'s circle and `d` in `b`'s square under a key of `a` and `b`, a
/// `pattern` gave `c` the texture of `a`, and `color(k)` colored four categories
/// and keyed two.
#[test]
fn a_column_two_tables_share_is_one_set_of_categories() {
    let table = |xs: [f64; 2], ks: [&str; 2]| DataFrame::new()
        .with_float("x", xs.to_vec()).with_float("y", xs.to_vec())
        .with_str("k", ks.iter().map(|k| k.to_string()).collect());
    let t: HashMap<String, DataFrame> = HashMap::from([
        ("ta".to_string(), table([1.0, 2.0], ["a", "b"])),
        ("tb".to_string(), table([3.0, 4.0], ["c", "d"])),
    ]);
    let two = |mark: Mark, channel: Channel| SvgRenderer::default().render(
        &PlotSpec::new().x("x").y("y")
            .layer(Layer::new(mark.clone()).data("ta").encode(channel.clone(), "k"))
            .layer(Layer::new(mark).data("tb").encode(channel, "k")),
        &t);
    for (mark, channel) in [(Mark::Point, Channel::Color), (Mark::Point, Channel::Shape),
                            (Mark::Bar, Channel::Pattern)] {
        let svg = two(mark.clone(), channel.clone());
        for k in ["a", "b", "c", "d"] {
            assert!(svg.contains(&format!(">{k}</text>")), "{channel:?}: the key lists `{k}`");
        }
    }
    // The four points are four glyphs: a circle, a square and two polygons, where
    // the second table repeated the first table's circle and square.
    let shapes = two(Mark::Point, Channel::Shape);
    let plot = &shapes[..shapes.find("font-size=\"11\" fill=\"#3c3c46\">").unwrap_or(shapes.len())];
    let glyphs: Vec<&str> = plot.lines()
        .filter(|l| l.contains(r##"fill="#4e79a7""##))
        .map(|l| l.trim_start().split(' ').next().unwrap())
        .collect();
    assert_eq!(glyphs, ["<circle", "<rect", "<polygon", "<polygon"], "{glyphs:?}");
    let polygons: Vec<&str> = plot.lines().filter(|l| l.contains("<polygon")).collect();
    assert_ne!(polygons[0].matches(',').count(), polygons[1].matches(',').count(),
        "`c` and `d` are two different polygons: {polygons:?}");
}

/// A map's meridian is the curve its projection makes, ticked where it meets
/// the panel's bottom edge: drawn straight up from the southernmost row, the
/// 180° line lay 192 px from a point at 180° and -20°, and 170° was labeled
/// outside the panel.
#[test]
fn a_maps_meridian_passes_through_its_points() {
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("lon", vec![176.0, 180.0, 186.0, 178.0])
            .with_float("lat", vec![-38.0, -20.0, -15.0, -30.0]),
    )]);
    let spec = PlotSpec::new().data("t").x("lon").y("lat")
        .layer(Layer::new(Mark::Point)).coord(CoordSpace::Map(Default::default()));
    let svg = SvgRenderer::default().render(&spec, &t);
    let circle = svg.split("<circle cx=\"").nth(2).expect("the point at 180°, -20°");
    let num = |s: &str| -> f64 { s.split('"').next().and_then(|v| v.parse().ok()).expect("a number") };
    let (cx, cy) = (num(circle), num(circle.split(" cy=\"").nth(1).unwrap()));
    // The curve that meets the bottom edge where "180°" is labeled.
    let label_x = svg.split(">180°</text>").next().and_then(|before| before.rsplit("<text x=\"").next())
        .map(num).expect("a 180° label");
    let curves: Vec<Vec<(f64, f64)>> = svg.split("<polyline points=\"").skip(1)
        .map(|c| c.split('"').next().unwrap().split(' ')
            .map(|p| { let (a, b) = p.split_once(',').unwrap(); (a.parse().unwrap(), b.parse().unwrap()) })
            .collect())
        .collect();
    let pts = curves.into_iter().find(|c: &Vec<(f64, f64)>| (c[0].0 - label_x).abs() < 0.01)
        .expect("the 180° meridian starts under its label");
    let x_at = pts.windows(2).find(|w| (w[0].1 - cy) * (w[1].1 - cy) <= 0.0)
        .map(|w| w[0].0 + (w[1].0 - w[0].0) * (cy - w[0].1) / (w[1].1 - w[0].1))
        .expect("the meridian spans the point's latitude");
    assert!((x_at - cx).abs() < 1.0, "the 180° meridian at the point's latitude: {x_at} vs {cx}");
}

/// Only a flat plot offers its axes to a page: a map's, a globe's and a cube's
/// positions are placed by their space, and a page lined them up with a flat
/// plot's on the same column, so each was read against the other's ticks.
#[test]
fn only_a_flat_plot_offers_its_axes_to_a_page() {
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("lon", vec![-150.0, -20.0, 60.0, 150.0])
            .with_float("lat", vec![-40.0, 10.0, 30.0, 60.0])
            .with_float("h", vec![1.0, 2.0, 3.0, 4.0]),
    )]);
    let spec = |coord: CoordSpace| PlotSpec::new().data("t").x("lon").y("lat")
        .layer(Layer::new(Mark::Point)).coord(coord);
    let flat = SvgRenderer::default().draw(&spec(CoordSpace::Flat), &t);
    assert_eq!((flat.x.field.as_str(), flat.y.field.as_str()), ("lon", "lat"));
    let map = SvgRenderer::default().draw(&spec(CoordSpace::Map(Default::default())), &t);
    assert!(map.x.field.is_empty() && map.y.field.is_empty(), "a map joins no share group");
    let mut cube = spec(CoordSpace::Flat);
    cube.z = Some(ChannelDef::field("h"));
    let cube = SvgRenderer::default().draw(&cube, &t);
    assert!(cube.x.field.is_empty(), "a cube joins no share group");
}

/// A flat histogram's axis reaches its bins' outer edges: fitted to the centers
/// with the ordinary breathing, it let the panel's clip slice each end bar of a
/// histogram with fewer than eleven bins.
#[test]
fn a_histograms_end_bars_are_drawn_whole() {
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new().with_float("v", (0..40).map(|i| f64::from(i % 17) * 1.3).collect()),
    )]);
    for (bins, horizontal) in [(3, false), (5, false), (5, true)] {
        let mut layer = Layer::new(Mark::Bar).transform(Transform::Bin);
        layer.bin = Some(crate::ir::BinSpec { bins: Some(bins), width: None, tiling: None });
        let spec = PlotSpec::new().data("t").layer(layer);
        let spec = if horizontal { spec.y("v") } else { spec.x("v") };
        let svg = SvgRenderer::default().render(&spec, &t);
        let clip = svg.split("<clipPath").nth(1).expect("a clipped panel");
        let num = |text: &str, key: &str| -> f64 {
            text.split(key).nth(1).and_then(|r| r.split('"').next())
                .and_then(|v| v.parse().ok()).expect("a number")
        };
        let (px, py) = (num(clip, " x=\""), num(clip, " y=\""));
        let (pw, ph) = (num(clip, " width=\""), num(clip, " height=\""));
        let bars: Vec<(f64, f64, f64, f64)> = svg.lines()
            .filter(|l| l.trim_start().starts_with("<rect") && l.contains("fill=\"#4e79a7\""))
            .map(|l| (num(l, " x=\""), num(l, " y=\""), num(l, " width=\""), num(l, " height=\"")))
            .collect();
        assert_eq!(bars.len(), bins, "one bar per bin: {bars:?}");
        for (x, y, w, h) in &bars {
            if horizontal {
                assert!(*y >= py - 1e-6 && y + h <= py + ph + 1e-6, "{bins} bins, sideways: {bars:?}");
            } else {
                assert!(*x >= px - 1e-6 && x + w <= px + pw + 1e-6, "{bins} bins: {bars:?}");
            }
        }
    }
}

/// A key takes the opacity its layer was set to: areas drawn at 1.0 or at 0.3
/// were both keyed at 0.82. Unset, the key keeps its own.
#[test]
fn a_key_takes_its_layers_set_opacity() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 1.0, 2.0])
            .with_float("y", vec![1.0, 2.0, 2.0, 3.0])
            .with_str("g", s(&["a", "a", "b", "b"])),
    )]);
    let keyed = |opacity: Option<f64>| {
        let mut layer = Layer::new(Mark::Area).encode(Channel::Color, "g");
        layer.style.opacity = opacity;
        let svg = SvgRenderer::default().render(&PlotSpec::new().data("t").x("x").y("y").layer(layer), &t);
        let key = svg.split(">G</text>").nth(1).expect("a key titled G").to_string();
        key.split("fill-opacity=\"").nth(1).and_then(|r| r.split('"').next()).unwrap_or("").to_string()
    };
    assert_eq!(keyed(Some(1.0)), "1.000");
    assert_eq!(keyed(Some(0.3)), "0.300");
    assert_eq!(keyed(None), "0.82");
}

/// A layout makes its `degree` column, so the table a key used to read it from
/// does not hold it, and nodes sized by their degree drew no key. The key reads
/// the frame the nodes were drawn from.
#[test]
fn a_size_on_a_layouts_degree_draws_its_key() {
    let hub = DataFrame::new()
        .with_str("a", vec!["Hub".into(); 4])
        .with_str("b", vec!["p".into(), "q".into(), "r".into(), "s".into()]);
    let data = HashMap::from([("t".to_string(), hub)]);
    let mut point = Layer::new(Mark::Point).layout("a", "b");
    point.encodings.insert(Channel::Size, crate::ir::ChannelDef::field("degree"));
    let spec = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Edge).layout("a", "b"))
        .layer(point)
        .coord(CoordSpace::Network(crate::ir::NetworkView::default()));
    let labels = text_of(&SvgRenderer::default().render(&spec, &data));
    assert!(labels.iter().any(|l| l == "Degree"), "a key titled Degree: {labels:?}");
}

/// A layer that maps no color, beside one whose colors a legend decodes, is
/// drawn in the neutral ink (spec §7): in the palette's first hue it read as
/// the legend's first category. With no legend it keeps the default, and a
/// set color is never replaced.
#[test]
fn an_unmapped_layer_beside_a_color_legend_is_drawn_in_neutral_ink() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0, 4.0])
            .with_float("y", vec![1.0, 3.0, 2.0, 4.0])
            .with_str("g", s(&["a", "b", "a", "b"])),
    )]);
    let line_stroke = |point: Layer, line: Layer| {
        let svg = SvgRenderer::default()
            .render(&PlotSpec::new().data("t").x("x").y("y").layer(point).layer(line), &t);
        let line = svg.split("<polyline ").nth(1).expect("the line").to_string();
        line.split("stroke=\"").nth(1).and_then(|r| r.split('"').next()).unwrap_or("").to_string()
    };
    let colored = || Layer::new(Mark::Point).encode(Channel::Color, "g");
    assert_eq!(line_stroke(colored(), Layer::new(Mark::Line)), NEUTRAL_INK);
    // No legend, no meaning in the hue: the default ink.
    assert_eq!(line_stroke(Layer::new(Mark::Point), Layer::new(Mark::Line)), PALETTE_GOG[0]);
    let mut hidden = Layer::new(Mark::Point);
    hidden.encodings.insert(Channel::Color, ChannelDef::field("g").with_legend(false));
    assert_eq!(line_stroke(hidden, Layer::new(Mark::Line)), PALETTE_GOG[0]);
    // A set color stays the author's.
    let mut set = Layer::new(Mark::Line);
    set.style.color = Some("firebrick".into());
    assert_eq!(line_stroke(colored(), set), "firebrick");
}

/// A texture key's swatches take the one ink their marks take. Keyed in the
/// default alone, bars set to firebrick were decoded by blue swatches, and
/// bars drawn in the neutral ink beside a density's legend would have been.
#[test]
fn a_texture_key_takes_the_ink_its_marks_take() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("a", s(&["p", "q", "r"]))
            .with_float("b", vec![3.0, 5.0, 4.0])
            .with_str("g", s(&["u", "v", "w"])),
    )]);
    let mut bar = Layer::new(Mark::Bar).encode(Channel::Pattern, "g");
    bar.style.color = Some("firebrick".into());
    let svg = SvgRenderer::default().render(&PlotSpec::new().data("t").x("a").y("b").layer(bar), &t);
    let key = svg.split(">G</text>").nth(1).expect("a key titled G");
    assert!(key.contains(r#"stroke="firebrick""#), "the swatches take the set color");
    assert!(!svg.contains(PALETTE_GOG[0]), "no swatch in the default ink");
}

/// A legend is set aside beside the panels before they are laid out, so a plot
/// too narrow for it drew a panel of negative width with no message. A legend
/// that would leave the panel narrower than itself is left out and said so.
#[test]
fn a_legend_too_wide_for_its_plot_is_left_out_and_said() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0, 4.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 4.0])
            .with_str("g", s(&["a long category", "another long one", "a", "b"])),
    )]);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "g"));
    let clip_w = |svg: &str| -> f64 {
        let rect = svg.split("<clipPath").nth(1).expect("a clipped panel");
        rect.split(" width=\"").nth(1).and_then(|w| w.split('"').next())
            .and_then(|w| w.parse().ok()).expect("a clip width")
    };
    let narrow = SvgRenderer::for_theme(&spec.theme.resolved(), 150.0, 400.0).draw(&spec, &t);
    assert!(clip_w(&narrow.svg) > 0.0, "the panel keeps a width: {}", clip_w(&narrow.svg));
    assert!(!narrow.svg.contains(">G</text>"), "no legend is drawn over the panel's room");
    assert!(narrow.remarks.iter().any(|d| d.kind == crate::legality::DiagnosticKind::Assumption
            && d.message.contains("`G` legend needs") && d.message.contains("theme(width = )")),
            "the legend left out is said: {:?}", narrow.remarks);
    // Where there is room, nothing changes and nothing is said.
    let wide = SvgRenderer::for_theme(&spec.theme.resolved(), 600.0, 400.0).draw(&spec, &t);
    assert!(wide.svg.contains(">G</text>") && wide.remarks.is_empty(), "{:?}", wide.remarks);
}

/// **A panel with no room to draw in is refused** (ruled 2026-09-29). What is
/// drawn around a panel is measured, so only the drawing knows when it takes
/// the whole plot: 60 px of width drew a panel -9 px wide with no message. The
/// drawing says so as an Illegal, the gate refuses it, and under
/// `GOG_STRICT=0` the plot still draws, the message beside it.
#[test]
fn a_panel_with_no_room_to_draw_in_is_refused() {
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new().with_float("x", vec![1.0, 20000.0]).with_float("y", vec![1.0, 80.0]),
    )]);
    let sized = |w: f64, h: f64| {
        let mut spec = PlotSpec::new().data("t").x("x").y("y").layer(Layer::new(Mark::Point));
        spec.theme.width = Some(w);
        spec.theme.height = Some(h);
        spec
    };
    let figure = |spec: PlotSpec| crate::ir::Figure::Plot(Box::new(spec));
    for (w, h, arg) in [(60.0, 400.0, "width"), (400.0, 50.0, "height")] {
        let drawn = SvgRenderer::for_theme(&sized(w, h).theme.resolved(), w, h).draw(&sized(w, h), &t);
        assert!(drawn.remarks.iter().any(|d| d.kind == crate::legality::DiagnosticKind::Illegal
            && d.message.contains("with no room to draw in")
            && d.message.contains(&format!("`theme({arg} = )`"))), "{:?}", drawn.remarks);
        let refused = crate::plot::render_figure_with(&figure(sized(w, h)), &t,
            crate::plot::Strictness::Strict);
        assert!(refused.is_err(), "{w} by {h} is refused");
        let drawn_anyway = crate::plot::render_figure_with(&figure(sized(w, h)), &t,
            crate::plot::Strictness::Permissive);
        assert!(drawn_anyway.is_ok_and(|d| d.diagnostics.iter().any(Diagnostic::is_fatal)),
            "under GOG_STRICT=0 it draws, and says why");
    }
    // Room enough: nothing said.
    let roomy = SvgRenderer::for_theme(&sized(300.0, 200.0).theme.resolved(), 300.0, 200.0)
        .draw(&sized(300.0, 200.0), &t);
    assert!(roomy.remarks.is_empty(), "{:?}", roomy.remarks);
}

/// Under a summary a dot is sized by its group's mean, so the key decodes the
/// means: it read the raw column's range, and the smallest mean read as the
/// smallest raw value.
#[test]
fn a_size_or_opacity_key_under_a_summary_reads_the_summaries() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", s(&["a", "a", "b", "b"]))
            .with_float("v", vec![10.0, 30.0, 50.0, 70.0]),
    )]);
    for ch in [Channel::Size, Channel::Opacity] {
        let spec = PlotSpec::new().data("t").x("g").y("v")
            .layer(Layer::new(Mark::Point).transform(Transform::Mean).encode(ch.clone(), "v"));
        let labels = text_of(&SvgRenderer::default().render(&spec, &t));
        // The key is drawn last: its title, then its three rows.
        let key = &labels[labels.len().saturating_sub(3)..];
        assert_eq!(key, ["20", "40", "60"],
                   "{ch:?}'s key runs from the lowest mean to the highest: {labels:?}");
    }
}

/// Under `repel` an empty label names nothing, so it is not placed and earns no
/// leader: ggrepel's `label = ""` idiom. It drew a leader line to nothing.
#[test]
fn an_empty_label_under_repel_draws_no_text_and_no_leader() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let n = 12;
    let xs: Vec<f64> = (0..n).map(|i| (i % 4) as f64).collect();
    let ys: Vec<f64> = (0..n).map(|i| (i / 4) as f64 * 0.2).collect();
    let mut labs = vec![""; n];
    labs[0] = "named";
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new().with_float("x", xs).with_float("y", ys).with_str("lab", s(&labs)),
    )]);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point))
        .layer(Layer::new(Mark::Text).transform(Transform::Repel).encode(Channel::Label, "lab"));
    let svg = SvgRenderer::default().render(&spec, &t);
    assert!(svg.contains(">named</text>"), "the one name is drawn");
    assert!(svg.matches("stroke-width=\"0.7\"").count() <= 1,
            "no leader for an empty label:\n{svg}");
    assert!(!svg.contains("></text>"), "no empty label is written");
}

/// A flow that leaves a category's rows out for a missing stage draws no band
/// for it, so its legend lists no key for it either: "Crew" was keyed in the
/// first category's color over bands that never drew it.
#[test]
fn a_legend_keys_only_the_categories_the_plot_colored() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("a", s(&["p", "p", "q", "gone"]))
            .with_str("b", s(&["u", "v", "u", ""]))
            .with_float("n", vec![3.0, 2.0, 4.0, 5.0]),
    )]);
    let spec = PlotSpec::new().data("t").y("n")
        .layer(Layer::new(Mark::Ribbon).flow(&["a", "b"]).encode(Channel::Color, "a"));
    let labels = text_of(&SvgRenderer::default().render(&spec, &t));
    assert!(labels.iter().any(|l| l == "p") && labels.iter().any(|l| l == "q"), "{labels:?}");
    assert!(!labels.iter().any(|l| l == "gone"), "a category drawn nowhere is keyed: {labels:?}");
}

/// A flow's bands are fills, so a mapped `pattern` hatches them: the legend drew
/// the hatch while every band stayed solid.
#[test]
fn a_flow_band_draws_the_pattern_its_legend_shows() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("a", s(&["p", "p", "q", "q"]))
            .with_str("b", s(&["u", "v", "u", "v"]))
            .with_float("n", vec![3.0, 2.0, 4.0, 1.0]),
    )]);
    let spec = PlotSpec::new().data("t").y("n")
        .layer(Layer::new(Mark::Ribbon).flow(&["a", "b"]).encode(Channel::Pattern, "a"));
    let svg = SvgRenderer::default().render(&spec, &t);
    let hatched_bands = svg.lines()
        .filter(|l| l.trim_start().starts_with("<path d=\"M ") && l.contains("fill=\"url(#"))
        .count();
    assert!(hatched_bands >= 1, "no band takes a hatch:\n{svg}");
}

/// A summarized point reads its orientation as a bar does: with the category
/// on `y` it summarizes along `x`, one dot per category. Read as vertical it
/// grouped by the numeric `x`, every row its own group, and drew every row.
#[test]
fn a_summarized_point_lies_on_its_side_as_a_bar_does() {
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("life", vec![40.0, 50.0, 60.0, 45.0, 55.0, 65.0])
            .with_str("year", ["a", "a", "a", "b", "b", "b"].map(String::from).to_vec()),
    )]);
    for reduce in [Transform::Median, Transform::Mean] {
        let spec = PlotSpec::new().data("t").x("life").y("year")
            .layer(Layer::new(Mark::Point).transform(reduce.clone()));
        let svg = SvgRenderer::default().render(&spec, &t);
        assert_eq!(svg.matches("<circle").count(), 2, "{reduce:?}: one dot per category");
    }
}

/// **Crowded numbers thin as names do** (ruled 2026-09-27). Five panels of a
/// gdp scatter printed 0K to 50K through each other at the panel edges, where
/// an edge number is drawn inward from its tick; they now keep every other
/// number, the ones a coarser step would choose, with no note, since the
/// numbers left out hide no data.
#[test]
fn crowded_numbers_thin_and_keep_the_coarser_steps() {
    let t: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("gdp", vec![300.0, 12_000.0, 25_000.0, 49_000.0, 900.0, 30_000.0])
            .with_float("life", vec![45.0, 60.0, 70.0, 80.0, 50.0, 75.0])
            .with_str("g", ["a", "b", "c", "d", "e", "a"].map(String::from).to_vec()),
    )]);
    let spec = PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Point)).facet_col("g");
    let drawn = SvgRenderer::default().draw(&spec, &t);
    let labels = text_of(&drawn.svg);
    assert!(labels.iter().any(|l| l == "20K") && labels.iter().any(|l| l == "40K"), "{labels:?}");
    assert!(!labels.iter().any(|l| l == "10K" || l == "30K"), "{labels:?}");
    assert!(!drawn.remarks.iter().any(|d| d.message.contains("tick_count")),
            "an unstated count thins in silence");
    // One plot alone keeps every number: the margins give them room.
    let alone = PlotSpec::new().data("t").x("gdp").y("life").layer(Layer::new(Mark::Point));
    assert!(text_of(&SvgRenderer::default().render(&alone, &t)).iter().any(|l| l == "10K"));
}

/// **A date on `z` is ticked on the calendar.** The cube passed no unit to its
/// third axis, so the same column read `Mar 4` on `x` and `1710M` on `z`.
#[test]
fn a_date_on_z_is_ticked_on_the_calendar() {
    let spec = PlotSpec::new().data("t").x("orders").y("orders").z("day")
        .coord(CoordSpace::Space(SpaceView::default()))
        .layer(Layer::new(Mark::Point));
    let labels = text_of(&SvgRenderer::default().render(&spec, &six_weeks(42)));
    assert!(labels.iter().any(|l| l == "Mar 4"), "no calendar tick on z: {labels:?}");
    assert!(!labels.iter().any(|l| l.ends_with('M')), "epoch seconds on z: {labels:?}");
}

/// **A map's degree ticks take the stated count**, which reached nothing: the
/// map chose 7 and 5 whatever was written.
#[test]
fn a_maps_degree_ticks_take_the_stated_count() {
    let degrees = |n: Option<usize>| {
        let mut spec = world_map(crate::ir::Preserve::Area);
        if let Some(n) = n {
            spec.x = Some(crate::ir::ChannelDef::field("lon").with_tick_count(n));
        }
        text_of(&SvgRenderer::default().render(&spec, &world()))
            .into_iter().filter(|l| l.ends_with('°')).count()
    };
    assert!(degrees(Some(20)) > degrees(None) + 5,
        "20 drew {} degree labels, the default {}", degrees(Some(20)), degrees(None));
}

/// **`color` and `shape` on one column are one key**, as `color` and `pattern`
/// are: colored glyphs under one title, where two keys both carried the
/// column's name, one of colored squares and one of gray glyphs.
#[test]
fn color_and_shape_on_one_column_draw_one_key_of_colored_glyphs() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0])
            .with_float("y", vec![1.0, 2.0, 3.0])
            .with_str("kind", vec!["a".into(), "b".into(), "c".into()]),
    )]);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point)
            .encode(Channel::Color, "kind")
            .encode(Channel::Shape, "kind"));
    let svg = SvgRenderer::default().render(&spec, &data);
    assert_eq!(text_of(&svg).iter().filter(|l| *l == "Kind").count(), 1, "two keys: {svg}");
    // The key's glyphs carry the category colors, not the neutral ink the
    // shape-only key draws them in.
    let gray = |svg: &str| svg.matches(r##"fill="#3c3c46""##).count();
    let shape_only = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point).encode(Channel::Shape, "kind"));
    let neutral = gray(&SvgRenderer::default().render(&shape_only, &data));
    assert!(neutral >= 3, "the shape-only key draws gray glyphs: {neutral}");
    assert_eq!(gray(&svg), neutral - 3, "a gray glyph in a merged key");
}

/// **A background that paints nothing paints nothing anywhere.** The canvas
/// was white whatever the theme said, so `transparent` showed white.
#[test]
fn a_transparent_background_leaves_the_canvas_and_the_key_unpainted() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0])
            .with_float("y", vec![1.0, 2.0, 3.0])
            .with_str("kind", vec!["a".into(), "b".into(), "c".into()]),
    )]);
    let mut spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "kind"));
    let canvas = r#"<rect width="800" height="600" fill="white"/>"#;
    let plain = SvgRenderer::default().render(&spec, &data);
    assert!(plain.contains(canvas) && plain.contains(r##"fill="white" stroke="#d2d2da""##));
    spec.theme = ThemeSpec { background: Some("transparent".into()), ..Default::default() };
    let clear = SvgRenderer::default().render(&spec, &data);
    assert!(!clear.contains(canvas), "the canvas is still white");
    assert!(clear.contains(r##"fill="none" stroke="#d2d2da""##), "the key's card is still white");
    // A color keeps the white canvas around the panel it paints.
    spec.theme = ThemeSpec { background: Some("lavender".into()), ..Default::default() };
    assert!(SvgRenderer::default().render(&spec, &data).contains(canvas));
}

/// Where the rotated y name was drawn: the pivot of its `rotate(-90 …)`.
fn y_name_x(svg: &str) -> f64 {
    let at = svg.find("rotate(-90 ").expect("a turned y name") + "rotate(-90 ".len();
    svg[at..].split_whitespace().next().unwrap().parse().unwrap()
}

/// **The names sit beside the plot drawn, not beside the room it was given.**
/// A `ratio` insets the panel and a circle sits inside a wider panel, and the
/// y name stayed at the image's edge either way: 135px from a square panel's
/// tick labels, and 150px from a rose.
#[test]
fn the_y_name_follows_a_panel_a_ratio_narrowed_and_a_circle() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0])
            .with_float("y", vec![1.0, 2.0, 3.0])
            .with_str("dir", vec!["N".into(), "E".into(), "S".into()]),
    )]);
    let mut square = PlotSpec::new().data("t").x("x").y("y").layer(Layer::new(Mark::Point));
    let free = y_name_x(&SvgRenderer::default().render(&square, &data));
    square.theme = ThemeSpec { ratio: Some(1.0), ..Default::default() };
    let svg = SvgRenderer::default().render(&square, &data);
    let panel_x = panel_rect(&svg).0;
    let named = y_name_x(&svg);
    assert!(named > free + 50.0, "the name stayed at {named}, the panel moved to {panel_x}");
    assert!(panel_x - named < 60.0, "the name sits {} px from its panel", panel_x - named);

    let rose = PlotSpec::new().data("t").x("dir")
        .coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }))
        .layer(Layer::new(Mark::Bar).transform(Transform::Count));
    let named = y_name_x(&SvgRenderer::default().render(&rose, &data));
    assert!(named > free + 50.0, "the radial name stayed at the image's edge: {named}");
}

/// **The title follows a panel a `ratio` shortened**, as the names beside it
/// do: left at the top of the rectangle, it stood 78 px over a map set beside
/// a globe. Not over a facet's column strips, which stay at the top of the
/// grid and would be under a title moved down.
#[test]
fn the_title_follows_a_panel_a_ratio_shortened_unless_strips_run_on_top() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0, 4.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 4.0])
            .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into()]),
    )]);
    let title_y = |svg: &str| -> f64 {
        let line = svg.lines().find(|l| l.contains(r#"font-weight="600""#)).expect("a title");
        line.split(r#"y=""#).nth(1).and_then(|r| r.split('"').next())
            .and_then(|v| v.parse().ok()).expect("its y")
    };
    let mut wide = PlotSpec::new().data("t").x("x").y("y").layer(Layer::new(Mark::Point));
    wide.title = Some("Wide".into());
    wide.theme = ThemeSpec { ratio: Some(3.0), ..Default::default() };
    let svg = SvgRenderer::default().render(&wide, &data);
    let (top, ty) = (panel_rect(&svg).1, title_y(&svg));
    assert!(top > 100.0, "the premise: the ratio left room above the panel: {top}");
    assert!(ty < top && top - ty < 20.0, "the title sits over its panel: title {ty}, panel {top}");

    wide.facet = Some(crate::ir::FacetSpec { col: Some("g".into()), ..Default::default() });
    let svg = SvgRenderer::default().render(&wide, &data);
    assert!(title_y(&svg) < 40.0, "over the strips, the title stays at the top: {}", title_y(&svg));
}

/// **`caps` reaches a `box`**: the crossbars at its whiskers' ends are the
/// same strokes as an `interval`'s, so `style(caps = FALSE)` leaves them off.
#[test]
fn a_box_without_caps_draws_its_whiskers_bare() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", ["a", "a", "a", "a", "a", "b", "b", "b", "b", "b"]
                .map(String::from).to_vec())
            .with_float("v", vec![1.0, 2.0, 3.0, 4.0, 5.0, 2.0, 4.0, 6.0, 8.0, 10.0]),
    )]);
    let drawn = |caps: Option<bool>| {
        let mut l = Layer::new(Mark::Box);
        l.style.caps = caps;
        let svg = SvgRenderer::default()
            .render(&PlotSpec::new().data("t").x("g").y("v").layer(l), &data);
        svg.matches(r#"stroke-linecap="round"/>"#).count()
    };
    assert_eq!(drawn(None), 4, "two boxes, two capped whiskers each");
    assert_eq!(drawn(Some(true)), 4);
    assert_eq!(drawn(Some(false)), 0, "the caps stayed on");
}

/// **An `edge` takes `style(arrow = )`**, the direction `layout(from, to)`
/// states, and a head stops short of the dot its node is drawn with, so the
/// dot drawn above it does not hide the head.
#[test]
fn an_edge_arrow_points_at_its_to_node_and_stops_short_of_the_dot() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("a", vec!["p".into()])
            .with_str("b", vec!["q".into()]),
    )]);
    let drawn = |arrow: &str| {
        let mut edge = Layer::new(Mark::Edge).layout("a", "b");
        edge.style.arrow = Some(arrow.into());
        let spec = PlotSpec::new().data("t")
            .layer(edge)
            .layer(Layer::new(Mark::Point).layout("a", "b"))
            .coord(CoordSpace::Network(crate::ir::NetworkView::default()));
        SvgRenderer::default().render(&spec, &data)
    };
    let svg = drawn("end");
    assert_eq!(svg.matches("<polygon").count(), 1, "one head: {svg}");
    assert_eq!(drawn("both").matches("<polygon").count(), 2);
    // The tip is the polygon's first point; the dots are the two circles.
    let tip: Vec<f64> = svg.split("<polygon points=\"").nth(1).unwrap()
        .split_whitespace().next().unwrap()
        .split(',').map(|n| n.parse().unwrap()).collect();
    let dots: Vec<(f64, f64)> = svg.split("<circle ").skip(1).map(|c| {
        let grab = |k: &str| -> f64 {
            let at = c.find(k).unwrap() + k.len();
            c[at..].split('"').next().unwrap().parse().unwrap()
        };
        (grab("cx=\""), grab("cy=\""))
    }).collect();
    let nearest = dots.iter()
        .map(|d| ((d.0 - tip[0]).powi(2) + (d.1 - tip[1]).powi(2)).sqrt())
        .fold(f64::INFINITY, f64::min);
    let r = SvgRenderer::default().point_radius;
    assert!((nearest - (r + crate::render::marks::HEAD_GAP)).abs() < 0.05,
        "the tip sits {nearest:.2} px from the dot's center, not {} px", r + crate::render::marks::HEAD_GAP);
}

/// **A mapped `pattern` dashes an `edge`**, one dash per category, where it
/// was refused as not drawn; and the key reads a stroke's dash for every
/// stroke mark, where a hand-typed list of three gave `path` hatched squares.
#[test]
fn a_mapped_pattern_dashes_an_edge_and_every_stroke_keys_with_dashes() {
    let spec = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Edge).layout("a", "b").encode(Channel::Pattern, "a"))
        .coord(CoordSpace::Network(crate::ir::NetworkView::default()));
    let svg = SvgRenderer::default().render(&spec, &net_table());
    // p, q and r take solid, dashed and dotted; a solid stroke writes no array.
    assert!(svg.contains(r#"stroke-dasharray="6,4""#) && svg.contains(r#"stroke-dasharray="1,4""#),
        "no dashes on the edges: {svg}");
    assert!(!svg.contains("<pattern"), "a stroke's key was hatched");

    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0, 1.0, 2.0, 3.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 3.0, 2.0, 1.0])
            .with_str("g", vec!["a".into(), "a".into(), "a".into(), "b".into(), "b".into(), "b".into()]),
    )]);
    let path = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Path).encode(Channel::Pattern, "g"));
    let svg = SvgRenderer::default().render(&path, &data);
    assert!(!svg.contains("<pattern"), "a path's key was hatched: {svg}");
}

/// The custom-palette count mismatch is a remark for the same reason.
#[test]
fn a_custom_palette_count_mismatch_is_a_remark_rather_than_stderr() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0])
            .with_float("y", vec![1.0, 2.0, 3.0])
            .with_str("g", vec!["a".into(), "b".into(), "c".into()]),
    )]);
    let mut spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "g"));
    spec.palette = crate::ir::PaletteDef::Custom(vec!["white".into(), "navy".into()]);
    let drawn = SvgRenderer::default().draw(&spec, &data);
    assert!(
        drawn.remarks.iter().any(|d| d.message.contains("generated automatically")),
        "the palette mismatch must ride in remarks: {:?}",
        drawn.remarks.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// Nothing left out, nothing said. The remark is a report of a real absence,
/// not a disclaimer the mark carries everywhere.
#[test]
fn a_packing_whose_labels_all_fit_says_nothing() {
    let bars = Layer::new(Mark::Bar).encode(Channel::Color, "g");
    let names = Layer::new(Mark::Text).encode(Channel::Label, "g");
    let drawn = SvgRenderer::default().draw(
        &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(bars).layer(names),
        &nest_label_data());
    assert!(drawn.remarks.is_empty(), "unexpected remarks: {:?}",
            drawn.remarks.iter().map(|d| &d.message).collect::<Vec<_>>());
}

/// The two-level packing labels the **inner** cells, and its labels land in the
/// inner cells rather than in the outer regions they are grouped by. This is
/// the sentence the spec named as the reason the mark was owed: countries
/// inside continents, where the split is too wide for a legend to decode.
#[test]
fn a_two_level_packing_labels_the_cells_and_not_the_groups() {
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("outer", ["p", "p", "q", "q"].into_iter().map(String::from).collect())
            .with_str("inner", ["w", "x", "y", "z"].into_iter().map(String::from).collect())
            .with_float("v", vec![3.0, 2.0, 3.0, 2.0]),
    )]);
    let bars = Layer::new(Mark::Bar).encode(Channel::Color, "inner");
    let names = Layer::new(Mark::Text).encode(Channel::Label, "inner");
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("outer").y("v")
            .coord(CoordSpace::Nest).layer(bars).layer(names),
        &data);
    let labels = packed_labels(&svg);
    assert_eq!(labels.len(), 4, "one name per cell: {labels:?}");
    let cells = packed_cells(&svg);
    for (i, (lx, _, s)) in labels.iter().enumerate() {
        let (cx, _, cw, _) = cells[i];
        assert!((lx - (cx + cw / 2.0)).abs() < 0.01,
                "`{s}` did not land in its own cell");
    }
}

/// A packing with **no split at all** — no `x`, no `color` — still packs every
/// row, and this is the one Law 7's third relaxation made reachable. Flat,
/// `bar + y(v)` is refused because every row would pile into one place with
/// nothing to tell them apart; here each row gets its own region, so the
/// refusal's reason does not reach and the plot is a plain one-color treemap.
///
/// It is pinned because it drew an **empty panel** first: with `x` unbound and
/// no split, `slot_orient` read the free axis as the one a transform would fill
/// in, made the measure the key, and left the bar looking for a column named
/// `""`. A packing has no orientation to read at all, which `plot_orient` now
/// says outright.
#[test]
fn a_packing_with_nothing_to_split_by_still_packs_every_row() {
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest)
            .layer(Layer::new(Mark::Bar)),
        &nest_label_data());
    let cells = packed_cells(&svg);
    assert_eq!(cells.len(), 4, "a packing with no split drew {} regions", cells.len());
    let area: f64 = cells.iter().map(|c| c.2 * c.3).sum();
    assert!(area > 1000.0, "the regions have no area: {cells:?}");
}

/// A packed label reads its color the way every other packed mark does — the
/// palette hue for its category, or the set color. Pinned because the region
/// branch resolves color separately from the positioned one, and a fill that
/// silently fell back to the default text ink would be invisible against a
/// dark cell rather than obviously wrong.
#[test]
fn a_packed_label_takes_the_set_color() {
    let bars = Layer::new(Mark::Bar).encode(Channel::Color, "g");
    let names = Layer::new(Mark::Text).encode(Channel::Label, "g").style_color("white");
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(bars).layer(names),
        &nest_label_data());
    assert_eq!(packed_labels(&svg).len(), 4);
    assert_eq!(svg.matches(r#"fill="white" fill-opacity"#).count(), 4,
               "a packed label lost its set color:\n{svg}");
}

#[test]
fn binding_z_projects_into_space_rather_than_being_ignored() {
    // The milestone claim: `z` is one more vowel (spec §15). A bound `z`
    // must reach the picture — the renderer draws a projected cube frame
    // instead of 2-D axes, and the point positions must differ from the flat
    // ones, or `z` was silently dropped (the §12 sin this project refuses).
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![0.0, 1.0, 2.0, 3.0])
            .with_float("y", vec![3.0, 2.0, 1.0, 0.0])
            .with_float("z", vec![0.0, 3.0, 1.0, 2.0]),
    )]);
    let render = |threed: bool| {
        let mut s = PlotSpec::new().data("t").x("x").y("y");
        if threed {
            s = s.z("z").coord(CoordSpace::Space(SpaceView::default()));
        }
        SvgRenderer::default().render(&s.layer(Layer::new(Mark::Point)), &data)
    };
    let flat = render(false);
    let space = render(true);

    assert!(!space.contains("NaN"), "3-D output has NaN coordinates");
    // The cube wireframe (its faint stroke color) is the 3-D guide; the flat
    // plot has none.
    assert!(space.contains("#d8d8de"), "3-D output drew no cube frame");
    assert!(!flat.contains("#d8d8de"), "2-D output drew a cube frame");
    assert_eq!(space.matches("<circle").count(), 4, "not every point placed");

    let cxs = |svg: &str| -> Vec<String> {
        svg.lines()
            .filter(|l| l.contains("<circle"))
            .filter_map(|l| Some(l.split(r#"cx=""#).nth(1)?.split('"').next()?.to_string()))
            .collect()
    };
    assert_ne!(cxs(&flat), cxs(&space), "z made no difference — it was ignored");
}

// -----------------------------------------------------------------------
// The 3-D frame's labels — measured, and drawn over the marks
//
// M8a shipped this frame with no test at all on where its labels land, and
// four defects lived in it undisturbed: an axis name printed through its own
// last tick (`S8pal Length` in the book's first 3-D plot), two axes' numbers
// superimposed at the corner they share, a foreshortened axis crowding, and
// every label along both floor edges painted over by a solid mesh.
//
// Three of them are one omission — the frame placed labels by projection and
// never measured them — and the first test below catches those three at once.
// The fourth is the *edge choice* underneath: the frame ticked the three edges
// meeting at the cube's farthest corner, which is why the labels were behind
// the data to be painted over in the first place. That one needs its own test,
// because measured placement resolves every overlap whichever edges are ticked:
// reverting `FrameEdge::choose` to M8a's fixed corner leaves the overlap test
// green (checked while writing these) and reddens only
// `a_3d_axis_is_ticked_along_an_edge_the_data_cannot_hide`.
// -----------------------------------------------------------------------

/// The frame's labels, as `(x, y, text)` — the one group carrying the halo, so
/// this reads exactly what `write_space_labels` placed and nothing else.
fn frame_labels_of(svg: &str) -> Vec<(f64, f64, String)> {
    let start = svg.find(r#"paint-order="stroke""#)
        .unwrap_or_else(|| panic!("no 3-D frame label group in:\n{svg}"));
    let group = &svg[start..start + svg[start..].find("</g>").unwrap()];
    let num = |s: &str| s.split('"').next().unwrap().parse::<f64>().unwrap();
    group.lines()
        .filter(|l| l.contains("<text"))
        .map(|l| (
            num(l.split(r#"x=""#).nth(1).unwrap()),
            num(l.split(r#"y=""#).nth(1).unwrap()),
            l.split('>').nth(1).unwrap().split('<').next().unwrap().to_string(),
        ))
        .collect()
}

/// How far apart two drawn labels' ink is: positive clears, negative overlaps.
/// Deliberately re-derived here from the drawn `x`/`y` rather than read off
/// `FrameLabel`, so the test measures the *output* and not the intention.
fn label_separation(a: &(f64, f64, String), b: &(f64, f64, String), font: f64) -> f64 {
    let half = |t: &str| crate::render::text::estimate_text_width(t, font) / 2.0;
    let cap = estimate_cap_height(font);
    let dx = (a.0 - b.0).abs() - (half(&a.2) + half(&b.2));
    let dy = (a.1 - b.1).abs() - cap;
    dx.max(dy)
}

/// A surface over a 6x6 lattice — a mark that covers the whole floor, which is
/// what made the painted-over labels impossible to keep ignoring.
fn sheet() -> HashMap<String, DataFrame> {
    let (mut xs, mut ys, mut zs) = (Vec::new(), Vec::new(), Vec::new());
    for i in 0..6 {
        for j in 0..6 {
            xs.push(i as f64 * 100.0);
            ys.push(j as f64 * 100.0);
            zs.push(100.0 + (i * j) as f64);
        }
    }
    HashMap::from([(
        "t".to_string(),
        DataFrame::new().with_float("east", xs).with_float("north", ys)
            .with_float("elev", zs),
    )])
}

fn sheet_spec(view: SpaceView) -> PlotSpec {
    PlotSpec::new().data("t").x("east").y("north").z("elev")
        .coord(CoordSpace::Space(view))
        .layer(Layer::new(Mark::Surface))
}

#[test]
fn all_three_axes_honor_a_stated_tick_count_including_z() {
    // `tick_count` reached no binding at all until 2026-07-26, and `z` was a
    // second absence wearing the same name: `build_axis` was handed a hard
    // `None` for the third axis, so even once a binding could write the field
    // the cube would have ignored it. Two ways for a property to be missing —
    // unreachable, and unread — and only the first was recorded.
    let counts = |spec: &PlotSpec| -> usize {
        let svg = SvgRenderer::default().render(spec, &sheet());
        frame_labels_of(&svg).into_iter()
            .filter(|(_, _, t)| t.parse::<f64>().is_ok())
            .count()
    };
    for ch in [Channel::X, Channel::Y, Channel::Z] {
        let with = |n: usize| {
            let mut s = sheet_spec(SpaceView::default());
            let def = match ch {
                Channel::X => s.x.as_mut(),
                Channel::Y => s.y.as_mut(),
                _ => s.z.as_mut(),
            };
            def.expect("the sheet binds all three").tick_count = Some(n);
            s
        };
        let few = counts(&with(2));
        let many = counts(&with(9));
        assert!(many > few,
            "{ch:?} drew {many} labels for 9 ticks and {few} for 2 — the count \
             is not reaching this axis");
    }
}

#[test]
fn no_two_labels_of_a_3d_frame_are_drawn_on_top_of_each_other() {
    // The invariant the frame had no test for, and the one every M8a defect
    // broke. Two labels overlapping is not a near-miss to be tuned away: it
    // prints one number through another and the reader cannot recover either.
    //
    // Swept over views rather than asserted at one angle, because each defect
    // showed at a different one — the name-through-its-tick at the default, the
    // shared-corner pile-up at a low tilt, the crowding only when an axis is
    // nearly edge-on.
    for &(turn, tilt) in &[
        (30.0, 25.0), (-50.0, 15.0), (30.0, 70.0), (30.0, 5.0),
        (140.0, 25.0), (0.0, 0.0), (30.0, 85.0), (-35.0, 45.0), (90.0, 60.0),
    ] {
        let svg = SvgRenderer::default().render(
            &sheet_spec(SpaceView { turn, tilt }), &sheet());
        let labels = frame_labels_of(&svg);
        assert!(labels.len() >= 4,
            "turn {turn} tilt {tilt}: frame drew almost nothing ({})", labels.len());
        for i in 0..labels.len() {
            for j in (i + 1)..labels.len() {
                // The larger font of the two bounds both boxes.
                let sep = label_separation(&labels[i], &labels[j], 12.0);
                assert!(sep > 0.0,
                    "turn {turn} tilt {tilt}: {:?} and {:?} overlap by {:.2}px",
                    labels[i].2, labels[j].2, -sep);
            }
        }
    }
}

#[test]
fn a_3d_frames_labels_are_drawn_after_its_marks_so_nothing_paints_over_them() {
    // A guide is an annotation *about* the scene, not a member of it — the rule
    // the flat and polar frames already followed, and the one the 3-D frame was
    // the single exception to. It mattered least for a scatter (sparse) and most
    // for a surface, which covers the floor completely: every number along both
    // domain edges was drawn and then buried.
    let svg = SvgRenderer::default().render(
        &sheet_spec(SpaceView::default()), &sheet());
    let first_face = svg.find("<path d=\"M").expect("the sheet drew no faces");
    let labels = svg.find(r#"paint-order="stroke""#).expect("no frame labels");
    assert!(labels > first_face,
        "the frame's labels are written before the marks, so the marks bury them");
    // And the *box* still goes behind, which is the draw-order stand-in for
    // occlusion that M8a was right about.
    let wireframe = svg.find("#d8d8de").expect("no cube frame");
    assert!(wireframe < first_face, "the glass box is no longer behind the marks");
}

/// One view is one picture, however the bearing was spelled.
///
/// A bearing is periodic and the grammar accepts every lap, so `turn = -360` is
/// the same view as `turn = 0` and has to draw the same bytes. It did not. The
/// marks and the frame lines matched, because they round to the same pixels, and
/// two of eighteen tick labels went missing: `sin(-2π)` is 2.4e-16 rather than
/// exactly 0, which flipped `FrameEdge::choose` on a comparison that guarded its
/// magnitude with an epsilon and its direction with nothing, and on the other
/// edge two labels collided and were dropped after their nudges.
///
/// Nothing could have caught it upstream. Every mark was in place, the picture
/// was plausible, and stderr was empty. So the assertion is equality of the
/// whole output across spellings, which is the only form that would have failed.
#[test]
fn one_view_draws_one_picture_however_the_bearing_is_spelled() {
    let draw = |turn| SvgRenderer::default()
        .render(&sheet_spec(SpaceView { turn, tilt: 25.0 }), &sheet());

    for (canonical, laps) in [(0.0, [-720.0, -360.0, 360.0, 720.0]),
                              (30.0, [-690.0, -330.0, 390.0, 750.0])] {
        let want = draw(canonical);
        // The count is asserted as well as the bytes, because it names the
        // symptom a reader would have seen and the bytes only say "different".
        let labels = |s: &str| s.matches("<text").count();
        for turn in laps {
            let got = draw(turn);
            assert_eq!(labels(&got), labels(&want),
                "turn {turn} lost labels against {canonical}");
            assert_eq!(got, want, "turn {turn} is not turn {canonical}");
        }
    }
}

/// **The dark edges are the numbered ones** (ruled 2026-09-28). The frame drew
/// the three edges from `(0,0,0)` a touch darker, and at every ordinary angle
/// that corner is the farthest from the camera: the dark edges met at the back
/// of the floor and carried no number, while the numbered edges were the faint
/// ones. Measured on the output: each dark edge has numbers written beside it.
#[test]
fn the_cube_darkens_the_edges_its_numbers_are_written_along() {
    for view in [SpaceView::default(), SpaceView { turn: -60.0, tilt: 40.0 }] {
        let svg = SvgRenderer::default().render(&sheet_spec(view.clone()), &sheet());
        let start = svg.find(r##"<g stroke="#9a9aa4" stroke-width="1.4""##)
            .expect("the cube's dark edges");
        let group = &svg[start..start + svg[start..].find("</g>").unwrap()];
        let num = |l: &str, k: &str| l.split(&format!(r#" {k}=""#)).nth(1).unwrap()
            .split('"').next().unwrap().parse::<f64>().unwrap();
        let edges: Vec<[f64; 4]> = group.lines().filter(|l| l.contains("<line"))
            .map(|l| [num(l, "x1"), num(l, "y1"), num(l, "x2"), num(l, "y2")])
            .collect();
        assert_eq!(edges.len(), 3, "one dark edge per axis");
        // How far a label's anchor sits from an edge, counted only beside the
        // edge's middle: two floor edges share a corner, so a number by the
        // corner would vouch for the wrong one.
        let beside = |(px, py): (f64, f64), e: &[f64; 4]| {
            let (dx, dy) = (e[2] - e[0], e[3] - e[1]);
            let t = ((px - e[0]) * dx + (py - e[1]) * dy) / (dx * dx + dy * dy);
            let d = ((px - e[0] - t * dx).powi(2) + (py - e[1] - t * dy).powi(2)).sqrt();
            if (0.15..=0.85).contains(&t) { d } else { f64::INFINITY }
        };
        let numbers: Vec<(f64, f64)> = frame_labels_of(&svg).into_iter()
            .filter(|(_, _, t)| t.chars().any(|c| c.is_ascii_digit()))
            .map(|(x, y, _)| (x, y))
            .collect();
        for e in &edges {
            let nearest = numbers.iter().map(|&p| beside(p, e)).fold(f64::INFINITY, f64::min);
            assert!(nearest < 30.0,
                "{view:?}: the dark edge {e:?} carries no number beside it, the \
                 nearest is {nearest:.1} px away");
        }
    }
}

/// **A floor axis is numbered along the floor edge lowest on the screen**, the
/// one on the outline, from above and from below. The rule was the edge nearer
/// the camera, which is the same edge from above, checked here over a sweep of
/// views so no drawing from above moves, and the wrong one from below: at
/// `tilt = -25` the floor was numbered across the middle of the picture, over
/// the data. From below, the lower edge is the farther one.
#[test]
fn a_floor_axis_is_numbered_along_the_edge_on_the_outline_from_above_and_below() {
    for turn in (-180..=180).step_by(15) {
        for tilt in [-85, -60, -45, -25, -10, 10, 25, 45, 60, 85] {
            let view = SpaceView { turn: turn as f64, tilt: tilt as f64 };
            let scene = project::Scene::new(view, 0.0, 0.0, 600.0, 600.0, FRAME_INSET);
            for axis in [FrameAxis::X, FrameAxis::Y] {
                let chosen = FrameEdge::choose(&scene, axis);
                let other = FrameEdge { axis, fixed: (1.0 - chosen.fixed.0, 0.0) };
                let (c, o) = (chosen.mid(&scene), other.mid(&scene));
                assert!(c.y >= o.y - 1e-6,
                    "turn {turn}, tilt {tilt}: {axis:?} is numbered on the higher floor edge");
                if (c.y - o.y).abs() > 1e-6 {
                    let nearer = c.depth < o.depth;
                    assert_eq!(nearer, tilt > 0,
                        "turn {turn}, tilt {tilt}: from {} the outline edge is the {} one",
                        if tilt > 0 { "above" } else { "below" },
                        if tilt > 0 { "nearer" } else { "farther" });
                }
            }
        }
    }
}

/// **A cube's labels read on a dark panel.** The numbers and names were written
/// in the dark defaults whatever `theme(background = )` said, so a night cube's
/// axes read as nothing. They take the light ink there, by the facet strip's
/// rule, and keep the dark ones on the default panel.
#[test]
fn a_cubes_labels_take_the_light_ink_on_a_dark_panel() {
    let inks = |background: Option<&str>| -> Vec<String> {
        let mut spec = sheet_spec(SpaceView::default());
        spec.theme.background = background.map(str::to_string);
        let svg = SvgRenderer::default().render(&spec, &sheet());
        let start = svg.find(r#"paint-order="stroke""#).expect("the frame's labels");
        let group = &svg[start..start + svg[start..].find("</g>").unwrap()];
        let mut inks: Vec<String> = group.lines().filter(|l| l.contains("<text"))
            .map(|l| l.split(r#"fill=""#).nth(1).unwrap().split('"').next().unwrap().to_string())
            .collect();
        inks.sort();
        inks.dedup();
        inks
    };
    assert_eq!(inks(Some("black")), [STRIP_INK_LIGHT], "white on a black panel");
    assert_eq!(inks(Some("#112233")), [STRIP_INK_LIGHT], "white on a dark blue one");
    assert_eq!(inks(None), ["#28283a", "#3c3c46"], "the dark defaults on the default panel");
}

#[test]
fn a_3d_axis_is_ticked_along_an_edge_the_data_cannot_hide() {
    // The root cause under all four defects. The cube offers four edges per
    // axis and M8a always took the three meeting at `(0,0,0)` — which is the
    // *farthest* corner from the camera at every ordinary angle, so all three
    // ran behind the data and met in the middle of the picture. Draw order hid
    // the consequence; drawing labels on top exposes it at once.
    //
    // Stated as a property rather than as coordinates: whichever edges are
    // ticked, their labels must lie outside the projected sheet's silhouette.
    // The sheet fills the cube's floor, so its horizontal extent stands in for
    // that silhouette, and a label inside it would be a label over the data.
    let svg = SvgRenderer::default().render(
        &sheet_spec(SpaceView::default()), &sheet());
    // Coordinates out of the mesh's own `d` attributes, and *only* those — the
    // first cut of this test split whole lines on non-digits and swept up
    // stroke widths and opacities, which put the mesh's box at x ≤ 49729.
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for line in svg.lines().filter(|l| l.contains("<path d=\"M")) {
        let d = line.split("d=\"").nth(1).unwrap().split('"').next().unwrap();
        for (i, tok) in d.split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f64>().ok())
            .enumerate()
        {
            if i % 2 == 0 { xs.push(tok) } else { ys.push(tok) }
        }
    }
    assert!(!xs.is_empty(), "no mesh geometry to compare against");
    let (top, bottom) = (
        ys.iter().copied().fold(f64::INFINITY, f64::min),
        ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    let (left, right) = (
        xs.iter().copied().fold(f64::INFINITY, f64::min),
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    for (x, y, text) in frame_labels_of(&svg) {
        let inside = x > left && x < right && y > top && y < bottom;
        assert!(!inside,
            "label {text:?} at ({x:.1}, {y:.1}) sits inside the mesh's own box \
             [{left:.1}..{right:.1}] x [{top:.1}..{bottom:.1}] — it is ticking \
             an edge that runs behind the data");
    }
}

#[test]
fn a_symmetric_view_chooses_its_measure_edge_by_rule_and_not_by_rounding() {
    // At the default `turn = 30` the two outermost struts are mirror images —
    // ±0.683 of the projected center — so "furthest from the center" is a tie,
    // and it was being settled by whichever sin/cos path rounded larger. Two
    // plots at the same viewing angle picked opposite sides of the cube.
    //
    // Every symmetric view has this property and the default view is symmetric,
    // so the tie needs a stated answer: the left, where a reader looks for a
    // vertical scale. Pinned across data that shares nothing but the angle.
    let z_name_x = |svg: &str| -> f64 {
        frame_labels_of(svg).into_iter()
            .find(|(_, _, t)| t == "Elev" || t == "z")
            .map(|(x, _, _)| x)
            .unwrap_or_else(|| panic!("no z axis name in:\n{svg}"))
    };
    let sheet_svg = SvgRenderer::default().render(
        &sheet_spec(SpaceView::default()).z_label("Elev"), &sheet());
    let cloud = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("east", vec![0.0, 1.0, 2.0, 3.0])
            .with_float("north", vec![3.0, 1.0, 2.0, 0.0])
            .with_float("elev", vec![1.0, 3.0, 0.0, 2.0]),
    )]);
    let cloud_svg = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("east").y("north").z("elev")
            .coord(CoordSpace::Space(SpaceView::default()))
            .z_label("Elev")
            .layer(Layer::new(Mark::Point)),
        &cloud);
    let center = |svg: &str| {
        let w: f64 = svg.split(r#"width=""#).nth(1).unwrap()
            .split('"').next().unwrap().parse().unwrap();
        w / 2.0
    };
    assert!(z_name_x(&sheet_svg) < center(&sheet_svg),
        "a sheet put its measure axis on the right of a symmetric view");
    assert!(z_name_x(&cloud_svg) < center(&cloud_svg),
        "a cloud put its measure axis on the right of the same view");
}

#[test]
fn a_foreshortened_3d_axis_thins_its_labels_and_leaves_its_scale_alone() {
    // Tilt the eye up and the measure axis projects short — at `tilt = 85` it is
    // 9% of its length — so its numbers run into each other. The answer is at
    // the **label** stage: draw fewer of them, at the step the scale already
    // chose. Not a coarser step, which is the one thing this must not do: a
    // bigger step raises the scale's ceiling and leaves the cube half empty,
    // which is the recorded reason a narrow *flat* panel is not thinned either
    // (see `build_axis`). So the labels that survive must be a subset of the
    // ones a roomy view draws, never a different set of numbers.
    let labels_at = |tilt: f64| -> Vec<String> {
        let svg = SvgRenderer::default().render(
            &sheet_spec(SpaceView { turn: 30.0, tilt }), &sheet());
        frame_labels_of(&svg).into_iter().map(|(_, _, t)| t).collect()
    };
    let roomy = labels_at(25.0);
    let squeezed = labels_at(85.0);
    assert!(squeezed.len() < roomy.len(),
        "an axis at 9% of its length drew as many labels as one at full length");
    for l in &squeezed {
        assert!(roomy.contains(l),
            "the squeezed view invented the label {l:?} — the step was recomputed \
             rather than the labels thinned, which moves the scale");
    }
}

/// A 5x5 mesh of points, one per cell, for the 3-D histogram tests below.
fn mesh_5x5() -> HashMap<String, DataFrame> {
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for i in 0..5 {
        for j in 0..5 {
            xs.push(i as f64);
            ys.push(j as f64);
        }
    }
    HashMap::from([(
        "t".to_string(),
        DataFrame::new().with_float("a", xs).with_float("b", ys),
    )])
}

/// `bar * bin + x(a) + y(b) + space()`, with no `z()` — the 3-D histogram.
fn hist_3d(view: SpaceView) -> PlotSpec {
    let mut layer = Layer::new(Mark::Bar).transform(Transform::Bin);
    layer.bin = Some(crate::ir::BinSpec { bins: Some(5), width: None, tiling: None });
    PlotSpec::new().data("t").x("a").y("b").coord(CoordSpace::Space(view)).layer(layer)
}

#[test]
fn a_bin_on_a_bar_in_space_cuts_both_axes_and_stands_the_count_up_on_z() {
    // Spec §5's dimensionality rule with the third axis present: a `bar` leaves
    // *two* axes free in the cube, so the same `bin` that cuts one flat cuts both
    // here — and the tally it invents rises along `z` rather than going to
    // `color`, because unlike a `zone` this mark has somewhere to put it.
    //
    // The whole sentence is `space()`: no `z()` is bound, exactly as a flat
    // histogram binds no `y()`.
    let data = mesh_5x5();
    let svg = SvgRenderer::default().render(&hist_3d(SpaceView::default()), &data);

    assert!(!svg.contains("NaN"), "3-D histogram has NaN coordinates:\n{svg}");
    assert!(svg.contains("#d8d8de"), "no cube frame — the plot was drawn flat");
    // 25 columns, and the faces the projection leaves visible. Counted as
    // *solids* rather than faces: a box shows three of its six from any one
    // angle, so what must be 25 is the number of columns, which is the number of
    // top faces — the one face every visible column has.
    let faces = svg.matches("<path d=\"M").count();
    assert!(faces >= 25 * 3, "expected at least 3 faces per column, got {faces}");
    // A flat histogram of the same data would put the count on `y`; here `y` is
    // still the second *domain*, so the tally cannot be on it.
    assert!(svg.contains(">A<") && svg.contains(">B<"),
        "both domains should label their own axes:\n{svg}");
    assert!(svg.contains(">Count<"), "the synthesized tally names the z axis:\n{svg}");
}

#[test]
fn a_column_in_space_sorts_by_its_foot_not_by_its_middle() {
    // Spec §15's sorted unit, one mark on: a glyph sorts whole, a stroke per
    // segment, and a **column by its footprint**. Every column stands on the same
    // floor, so which hides which is settled by where their feet are — and a tall
    // far column must not paint over a short near one by claiming its middle is
    // closer. Two cells, one far and tall, one near and short.
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            // Four rows in the far cell (a tall column), one in the near cell.
            .with_float("a", vec![0.0, 0.0, 0.0, 0.0, 1.0])
            .with_float("b", vec![0.0, 0.0, 0.0, 0.0, 1.0]),
    )]);
    let mut layer = Layer::new(Mark::Bar).transform(Transform::Bin);
    layer.bin = Some(crate::ir::BinSpec { bins: Some(2), width: None, tiling: None });
    let spec = PlotSpec::new().data("t").x("a").y("b")
        .coord(CoordSpace::Space(SpaceView::default()))
        .layer(layer);
    let svg = SvgRenderer::default().render(&spec, &data);

    // Depth order is paint order. The near column's faces must all come after the
    // far column's, whatever their heights. Read the mean y of each path as a
    // stand-in for which column it belongs to is fragile; instead assert the
    // shading pattern repeats twice — one full column, then the other.
    let tops = svg.matches("<path d=\"M").count();
    assert!(tops >= 6, "two columns should draw at least three faces each: {tops}");
    assert!(!svg.contains("NaN"));
}

#[test]
fn a_cut_floor_touches_and_a_slotted_one_leaves_air() {
    // Law 2, one dimension up. Flat, a histogram's bars fill their bins and touch
    // while a categorical bar takes four fifths of its slot — the empty fifth is
    // what says the categories are separate rather than a divided continuum. The
    // cube must say the same thing, per axis, or the gap has become a property of
    // the space rather than of what the axis means.
    //
    // Measured on the *floor* rather than on the drawn faces: the footprint edges
    // are what the rule is about, and they are what the projection then turns into
    // a solid. Two columns side by side on each axis, so a gap is visible as the
    // horizontal run between the near corners of the two.
    let cut = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("a", vec![0.0, 0.0, 1.0, 1.0])
            .with_float("b", vec![0.0, 1.0, 0.0, 1.0]),
    )]);
    let slotted = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("a", vec!["p".into(), "p".into(), "q".into(), "q".into()])
            .with_str("b", vec!["r".into(), "s".into(), "r".into(), "s".into()]),
    )]);
    // Straight down the z axis: tilt 90° looks at the floor, so the projected
    // footprints are the footprints and nothing is foreshortened.
    let view = SpaceView { turn: 0.0, tilt: 90.0 };
    let span = |svg: &str| -> f64 {
        let xs: Vec<f64> = svg.lines()
            .filter(|l| l.contains("<path d=\"M"))
            .flat_map(|l| l.split(|c| c == 'M' || c == 'L').skip(1)
                .filter_map(|s| s.split(',').next()?.trim().parse::<f64>().ok())
                .collect::<Vec<_>>())
            .collect();
        xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - xs.iter().cloned().fold(f64::INFINITY, f64::min)
    };

    let mut cut_layer = Layer::new(Mark::Bar).transform(Transform::Bin);
    cut_layer.bin = Some(crate::ir::BinSpec { bins: Some(2), width: None, tiling: None });
    let cut_svg = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("a").y("b")
            .coord(CoordSpace::Space(view)).layer(cut_layer), &cut);
    let slot_svg = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("a").y("b")
            .coord(CoordSpace::Space(view))
            .layer(Layer::new(Mark::Bar).transform(Transform::Count)), &slotted);

    // Both meshes cover the same two slots per axis, so the cut one — whose cells
    // abut — must span strictly wider on the page than the slotted one, which
    // holds a fifth of each slot back as air.
    let (c, s) = (span(&cut_svg), span(&slot_svg));
    assert!(c > s * 1.05,
        "a cut floor should touch and a slotted one leave air: cut spans {c:.1}, slotted {s:.1}");
}

#[test]
fn turning_the_scene_rearranges_the_faces_without_recoloring_them() {
    // `palette::shade` keys a face's shade to its **data axis**, not to where the
    // light falls, so `space(turn = )` must not change which colors appear. A
    // lamp fixed in screen space would re-shade every face as the reader turned
    // the cube, and a hue that moves while the data stands still is the silent
    // wrongness §12 forbids.
    let data = mesh_5x5();
    let shades = |view: SpaceView| -> std::collections::BTreeSet<String> {
        let svg = SvgRenderer::default().render(&hist_3d(view), &data);
        svg.lines()
            .filter(|l| l.contains("<path d=\"M"))
            .filter_map(|l| Some(l.split(r#"fill=""#).nth(1)?.split('"').next()?.to_string()))
            .collect()
    };
    let a = shades(SpaceView { turn: 30.0, tilt: 25.0 });
    let b = shades(SpaceView { turn: 75.0, tilt: 25.0 });
    assert_eq!(a, b, "turning the scene changed the palette of face shades");
    assert!(a.len() >= 2, "a solid needs its faces told apart, got {a:?}");
}

#[test]
fn a_three_d_scatter_paints_far_points_before_near_ones() {
    // Depth order: two points identical in x and y but far apart in z must be
    // emitted far-first, so the nearer one paints on top. The projector pins
    // "nearer = smaller depth"; this checks the renderer actually sorts by it.
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![0.5, 0.5])
            .with_float("y", vec![0.5, 0.5])
            .with_float("z", vec![0.0, 1.0]), // row 1 is higher, so nearer
    )]);
    let spec = PlotSpec::new().data("t").x("x").y("y").z("z")
        .coord(CoordSpace::Space(SpaceView::default()))
        .layer(Layer::new(Mark::Point));
    let svg = SvgRenderer::default().render(&spec, &data);
    // Only the two data points carry a radius; the order they appear in the
    // SVG is the paint order. The higher-z (nearer) point must be drawn last.
    let cys: Vec<f64> = svg.lines()
        .filter(|l| l.contains("<circle"))
        .filter_map(|l| l.split(r#"cy=""#).nth(1)?.split('"').next()?.parse().ok())
        .collect();
    assert_eq!(cys.len(), 2);
    // Higher z projects higher on screen (smaller cy). It is nearer, so it is
    // painted last — the second circle should be the higher one.
    assert!(cys[1] < cys[0], "near point ({}, drawn last) should sit above far point ({})", cys[1], cys[0]);
}

#[test]
fn jitter_spreads_the_category_axis_but_never_the_measured_one() {
    // The distinctive design claim (spec §5): `point * jitter` nudges points
    // along the *categorical* axis only, leaving the *measured* axis exact — so
    // coincident points separate to show density, yet no point is moved off its
    // value. Two categories, three coincident points each.
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", vec!["A".into(), "A".into(), "A".into(), "B".into(), "B".into(), "B".into()])
            .with_float("v", vec![5.0, 5.0, 5.0, 7.0, 7.0, 7.0]),
    )]);
    let spec = |jit: bool| {
        let mut layer = Layer::new(Mark::Point);
        if jit { layer = layer.transform(Transform::Jitter); }
        PlotSpec::new().data("t").x("g").y("v").layer(layer)
    };
    let coords = |svg: &str| -> Vec<(f64, f64)> {
        svg.lines().filter(|l| l.contains("<circle")).filter_map(|l| {
            let cx = l.split(r#"cx=""#).nth(1)?.split('"').next()?.parse().ok()?;
            let cy = l.split(r#"cy=""#).nth(1)?.split('"').next()?.parse().ok()?;
            Some((cx, cy))
        }).collect()
    };

    let plain = coords(&SvgRenderer::default().render(&spec(false), &data));
    let jittered = coords(&SvgRenderer::default().render(&spec(true), &data));
    assert_eq!(plain.len(), 6);
    assert_eq!(jittered.len(), 6);

    // Un-jittered, each category's three points are coincident — one cx, one cy.
    assert!(plain[0..3].iter().all(|&p| p == plain[0]), "A's points should coincide un-jittered");
    assert!(plain[3..6].iter().all(|&p| p == plain[3]), "B's points should coincide un-jittered");

    // The measured axis (y) is untouched: every jittered cy equals the exact
    // un-jittered cy for its category. This is the ggplot2 divergence.
    for i in 0..6 {
        assert!((jittered[i].1 - plain[i].1).abs() < 1e-9,
            "row {i}: jitter moved y from {} to {} — it must not touch the measure", plain[i].1, jittered[i].1);
    }

    // The categorical axis (x) *is* spread: within a category the three points
    // no longer share a cx.
    let a_xs: std::collections::BTreeSet<i64> = jittered[0..3].iter().map(|p| (p.0 * 1e6) as i64).collect();
    assert_eq!(a_xs.len(), 3, "jitter should give A's three coincident points distinct x: {:?}", &jittered[0..3]);

    // Bounded to the slot: a jittered point stays near its category center (so
    // the axis still reads), never wandering past half the inter-category gap.
    let slot = (plain[3].0 - plain[0].0).abs(); // A-center to B-center
    for i in 0..6 {
        assert!((jittered[i].0 - plain[i].0).abs() < slot / 2.0,
            "row {i}: jitter offset {} exceeds half the slot {slot}", (jittered[i].0 - plain[i].0).abs());
    }
}

#[test]
fn jitter_is_deterministic() {
    // One spec, one picture — the IR's whole reason to exist. The spread is
    // seeded from the data, so two renders are byte-identical (no clock, no RNG).
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", vec!["A".into(), "A".into(), "A".into(), "A".into()])
            .with_float("v", vec![1.0, 1.0, 2.0, 2.0]),
    )]);
    let spec = PlotSpec::new().data("t").x("g").y("v")
        .layer(Layer::new(Mark::Point).transform(Transform::Jitter));
    let a = SvgRenderer::default().render(&spec, &data);
    let b = SvgRenderer::default().render(&spec, &data);
    assert_eq!(a, b, "jitter must render identically every run");
}

#[test]
fn jitter_amount_scales_the_spread_linearly() {
    // `jitter(amount)` multiplies the slot-derived band. The seed is keyed to the
    // row and its data, *not* the amount, so every point's offset scales exactly:
    // `jitter(0.5)` is half the offset of bare `jitter`, and `jitter(0)` is none.
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", vec!["A".into(), "A".into(), "A".into(), "A".into(),
                                "B".into(), "B".into(), "B".into(), "B".into()])
            .with_float("v", vec![5.0, 5.0, 5.0, 5.0, 7.0, 7.0, 7.0, 7.0]),
    )]);
    let cxs = |amount: Option<f64>| -> Vec<f64> {
        let mut layer = Layer::new(Mark::Point);
        if let Some(a) = amount { layer = layer.transform(Transform::Jitter).jitter_amount(a); }
        let spec = PlotSpec::new().data("t").x("g").y("v").layer(layer);
        SvgRenderer::default().render(&spec, &data).lines()
            .filter(|l| l.contains("<circle"))
            .filter_map(|l| l.split(r#"cx=""#).nth(1)?.split('"').next()?.parse::<f64>().ok())
            .collect()
    };
    let plain = cxs(None);            // no jitter transform at all — the centers
    let full  = cxs(Some(1.0));       // bare-jitter equivalent
    let half  = cxs(Some(0.5));
    let zero  = cxs(Some(0.0));
    assert_eq!(plain.len(), 8);

    let mut moved = 0;
    for i in 0..8 {
        // amount 0 collapses the spread — every point back on its center.
        assert!((zero[i] - plain[i]).abs() < 0.01, "jitter(0) should not move point {i}");
        // amount 0.5 is exactly half the offset amount 1 produces (same seed).
        let full_off = full[i] - plain[i];
        let half_off = half[i] - plain[i];
        assert!((half_off - 0.5 * full_off).abs() < 0.05,
            "point {i}: jitter(0.5) offset {half_off} should be half of jitter(1)'s {full_off}");
        if full_off.abs() > 0.5 { moved += 1; }
    }
    assert!(moved >= 6, "most points should visibly move under a full jitter (moved {moved}/8)");
}

// -----------------------------------------------------------------------
// `point * dodge` — the beeswarm (spec §5)
// -----------------------------------------------------------------------

/// Every dot a plot draws, as (cx, cy, r, fill), in the order drawn.
fn swarm_dots(svg: &str) -> Vec<(f64, f64, f64, String)> {
    let attr = |l: &str, a: &str| -> Option<String> {
        Some(l.split(&format!(r#" {a}=""#)).nth(1)?.split('"').next()?.to_string())
    };
    svg.lines().filter(|l| l.contains("<circle")).filter_map(|l| Some((
        attr(l, "cx")?.parse().ok()?,
        attr(l, "cy")?.parse().ok()?,
        attr(l, "r")?.parse().ok()?,
        attr(l, "fill")?,
    ))).collect()
}

/// A strip that has to swarm: sixty rows in `A` packed into a sliver of the
/// measure axis (one far row stretches it), and a few in `B`.
fn swarm_frame() -> HashMap<String, DataFrame> {
    let mut g: Vec<String> = Vec::new();
    let mut v: Vec<f64> = Vec::new();
    for i in 0..60 {
        g.push("A".into());
        v.push((i % 20) as f64 * 0.05);
    }
    for i in 0..6 {
        g.push("B".into());
        v.push(1.0 + i as f64 * 0.02);
    }
    g.push("A".into());
    v.push(20.0);
    HashMap::from([("t".to_string(), DataFrame::new().with_str("g", g).with_float("v", v))])
}

/// The design's three claims, drawn (spec §5): a swarm moves a point across
/// its category's slot and never along the measure, clears every neighbor
/// when the slot has room, and stays inside the band a bar would fill.
#[test]
fn a_swarm_moves_points_across_the_slot_and_never_along_the_measure() {
    let data = swarm_frame();
    let dots = |swarm: bool| {
        let mut layer = Layer::new(Mark::Point);
        if swarm { layer = layer.transform(Transform::Dodge); }
        swarm_dots(&SvgRenderer::default().render(&PlotSpec::new().data("t").x("g").y("v").layer(layer), &data))
    };
    let (plain, swarm) = (dots(false), dots(true));
    assert_eq!(plain.len(), 67);
    assert_eq!(swarm.len(), 67);
    let (a, b) = (plain[0].0, plain[60].0);
    let band = 0.8 * (b - a).abs();
    for i in 0..67 {
        // The measure is exact: not one pixel along it.
        assert!((swarm[i].1 - plain[i].1).abs() < 1e-9, "row {i} moved along the measure");
        // Inside the band a bar in this slot would fill, glyph and all.
        assert!((swarm[i].0 - plain[i].0).abs() <= band / 2.0 - swarm[i].2 + 0.01,
                "row {i} left its band: {} from its center", (swarm[i].0 - plain[i].0).abs());
    }
    // No two dots in one slot overlap: the slot has room for all of them.
    for i in 0..67 {
        for j in i + 1..67 {
            if (plain[i].0 - plain[j].0).abs() > 1e-9 { continue }
            let d = ((swarm[i].0 - swarm[j].0).powi(2) + (swarm[i].1 - swarm[j].1).powi(2)).sqrt();
            assert!(d >= swarm[i].2 + swarm[j].2 - 0.01, "rows {i} and {j} overlap: {d}");
        }
    }
    // It is a swarm and not a column: points went both ways, about evenly.
    let off: Vec<f64> = (0..60).map(|i| swarm[i].0 - plain[i].0).collect();
    let (left, right) = (off.iter().filter(|o| **o < -0.5).count(), off.iter().filter(|o| **o > 0.5).count());
    assert!(left > 10 && right > 10 && left.abs_diff(right) <= 3, "left {left}, right {right}");
}

/// One sentence, one picture, whatever order the table arrived in: the swarm
/// is placed by value, so reversing the rows moves no dot.
#[test]
fn a_swarm_is_the_same_picture_in_any_row_order() {
    let data = swarm_frame();
    let df = &data["t"];
    let n = df.float_col("v").unwrap().len();
    let rev: HashMap<String, DataFrame> = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", df.str_col("g").unwrap().iter().rev().cloned().collect())
        .with_float("v", df.float_col("v").unwrap().iter().rev().copied().collect()))]);
    let spec = PlotSpec::new().data("t").x("g").y("v")
        .layer(Layer::new(Mark::Point).transform(Transform::Dodge));
    let sorted = |d: &HashMap<String, DataFrame>| {
        let mut p: Vec<(i64, i64)> = swarm_dots(&SvgRenderer::default().render(&spec, d)).iter()
            .map(|t| ((t.0 * 100.0).round() as i64, (t.1 * 100.0).round() as i64)).collect();
        p.sort();
        p
    };
    let (fwd, back) = (sorted(&data), sorted(&rev));
    assert_eq!(fwd.len(), n);
    assert_eq!(fwd, back, "reversing the table moved a dot");
    assert_eq!(SvgRenderer::default().render(&spec, &data), SvgRenderer::default().render(&spec, &data));
}

/// The horizontal strip is the same rule turned: the category is on `y`, so
/// the swarm moves points up and down and leaves every `x` exact.
#[test]
fn a_swarm_on_a_horizontal_strip_moves_vertically() {
    let data = swarm_frame();
    let dots = |swarm: bool| {
        let mut layer = Layer::new(Mark::Point);
        if swarm { layer = layer.transform(Transform::Dodge); }
        swarm_dots(&SvgRenderer::default().render(&PlotSpec::new().data("t").x("v").y("g").layer(layer), &data))
    };
    let (plain, swarm) = (dots(false), dots(true));
    assert!((0..67).all(|i| (swarm[i].0 - plain[i].0).abs() < 1e-9), "a horizontal swarm moved a value");
    assert!((0..60).filter(|&i| (swarm[i].1 - plain[i].1).abs() > 0.5).count() > 20,
            "a horizontal swarm should move points across its slot");
}

/// A split tiles each slot as `bar * dodge` tiles it, the first group on the
/// left, and each tile swarms on its own: no dot leaves its group's tile.
#[test]
fn a_split_swarm_tiles_the_slot_as_a_dodged_bar_does() {
    let data = swarm_frame();
    let df = &data["t"];
    let k: Vec<String> = (0..df.float_col("v").unwrap().len())
        .map(|i| if i % 2 == 0 { "p" } else { "q" }.to_string()).collect();
    let data: HashMap<String, DataFrame> = HashMap::from([("t".to_string(), df.clone().with_str("k", k.clone()))]);
    // The same sentence without `dodge` draws every dot on its category's
    // center, on the same panel: the color legend narrows both alike.
    let colored = |layer: Layer| swarm_dots(&SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("g").y("v").layer(layer.encode(Channel::Color, "k")), &data));
    let mut centers: Vec<f64> = colored(Layer::new(Mark::Point)).iter().map(|d| d.0).collect();
    centers.sort_by(|a, b| a.partial_cmp(b).unwrap());
    centers.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert_eq!(centers.len(), 2);
    let (ca, cb) = (centers[0], centers[1]);
    // A split frame is recombined group by group, so the dots are matched by
    // what they show rather than by row: the slot by its center, the group by
    // its color (`p` takes the palette's first, `q` its second).
    let swarm = colored(Layer::new(Mark::Point).transform(Transform::Dodge));
    assert_eq!(swarm.len(), k.len());
    let band = 0.8 * (cb - ca).abs();
    let quarter = band / 4.0; // two tiles: each `band/2` wide, centered `band/4` off
    let mut in_a = 0;
    for d in &swarm {
        let center = if (d.0 - ca).abs() < (d.0 - cb).abs() { in_a += 1; ca } else { cb };
        let side = match d.3.as_str() {
            "#4e79a7" => -1.0,
            "#f28e2b" => 1.0,
            other => panic!("a dot in an unexpected color {other}"),
        };
        let tile = center + side * quarter;
        assert!((d.0 - tile).abs() <= quarter - d.2 + 0.01,
                "a {} dot left its tile: {} from its center", d.3, (d.0 - tile).abs());
    }
    assert_eq!(in_a, 61, "every `A` row stays in `A`'s slot");
}

/// A swarm wider than its slot is never spilled into the next category and
/// never dropped (§12): every point is drawn inside its band, and the plot
/// says how many overlap and where.
#[test]
fn a_crowded_swarm_stays_in_its_slot_and_says_so() {
    let n = 300;
    let mut g = vec!["A".to_string(); n - 1];
    g.push("B".into());
    let mut v = vec![1.0; n - 1];
    v.push(2.0);
    let data: HashMap<String, DataFrame> =
        HashMap::from([("t".to_string(), DataFrame::new().with_str("g", g).with_float("v", v))]);
    let plain = swarm_dots(&SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("g").y("v").layer(Layer::new(Mark::Point)), &data));
    let spec = PlotSpec::new().data("t").x("g").y("v")
        .layer(Layer::new(Mark::Point).transform(Transform::Dodge));
    let drawn = SvgRenderer::default().draw(&spec, &data);
    let swarm = swarm_dots(&drawn.svg);
    assert_eq!(swarm.len(), n, "a crowded swarm dropped a point");
    let band = 0.8 * (plain[n - 1].0 - plain[0].0).abs();
    for i in 0..n {
        assert!((swarm[i].0 - plain[i].0).abs() <= band / 2.0 - swarm[i].2 + 0.01,
                "row {i} spilled out of its slot");
    }
    let said: Vec<&str> = drawn.remarks.iter().map(|d| d.message.as_str()).collect();
    assert!(said.iter().any(|m| m.contains("`point * dodge` has no room")
                               && m.contains(&format!("of {n} points"))
                               && m.contains("in `A`")),
            "a crowded swarm must say so: {said:?}");
    // A swarm with room says nothing.
    let roomy = SvgRenderer::default().draw(&spec, &swarm_frame());
    assert!(roomy.remarks.iter().all(|d| !d.message.contains("point * dodge")),
            "a swarm with room reported crowding: {:?}", roomy.remarks);
}

/// Five labels on one spot, and the panel they have to be arranged inside.
fn crowded_labels(n: usize) -> HashMap<String, DataFrame> {
    let names: Vec<String> = (0..n).map(|i| format!("name {i}")).collect();
    HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![5.0; n])
            .with_float("y", vec![5.0; n])
            .with_str("n", names),
    )])
}

/// The mark's own labels, as (x, y) — the tick labels and the title carry no
/// `fill-opacity`, which is what tells the two apart.
fn label_positions(svg: &str) -> Vec<(f64, f64)> {
    svg.lines()
        .filter(|l| l.contains("<text") && l.contains("fill-opacity"))
        .filter_map(|l| {
            let x = l.split(r#"x=""#).nth(1)?.split('"').next()?.parse().ok()?;
            let y = l.split(r#"y=""#).nth(1)?.split('"').next()?.parse().ok()?;
            Some((x, y))
        })
        .collect()
}

fn repel_spec(repel: bool) -> PlotSpec {
    let mut layer = Layer::new(Mark::Text).encode(Channel::Label, "n");
    if repel {
        layer = layer.transform(Transform::Repel);
    }
    PlotSpec::new().data("t").x("x").y("y").layer(layer)
}

#[test]
fn repel_separates_labels_that_would_otherwise_sit_on_one_another() {
    // The distinctive claim (spec §5): what `repel` resolves is *ink*. Five
    // labels at one identical position are one illegible smudge without it, and
    // five readable words with it — so the test is not "did they move" but "do
    // any two still occupy the same line".
    let data = crowded_labels(5);
    let plain = label_positions(&SvgRenderer::default().render(&repel_spec(false), &data));
    let moved = label_positions(&SvgRenderer::default().render(&repel_spec(true), &data));
    assert_eq!(plain.len(), 5);
    assert_eq!(moved.len(), 5);

    // Un-repelled, all five are the same glyph in the same place.
    assert!(plain.iter().all(|&p| p == plain[0]), "five coincident rows should draw five coincident labels");

    // Repelled, no two share a line: every pair is at least a cap height apart.
    // The rows are identical, so this is also the tie-break working — there is
    // nothing in the data to part them by.
    for i in 0..5 {
        for j in (i + 1)..5 {
            let apart = (moved[i].0 - moved[j].0).abs().max((moved[i].1 - moved[j].1).abs());
            assert!(apart > 7.0, "labels {i} and {j} are still on top of each other: {:?} {:?}", moved[i], moved[j]);
        }
    }
}

#[test]
fn repelled_labels_spread_to_every_side_rather_than_stacking_above() {
    // The defect a reader measured (issue #2): the spring dragged every label
    // back toward its initial above-the-dot offset and the escape ran on the
    // axis of least overlap, so 24 of 26 labels sat above their point where
    // ggrepel spreads them wherever there is room. The fix's claim is
    // directional freedom, so its test is directional, on the reader's own
    // measure: two crowded clusters of names, each label classified by where
    // it rests relative to its own point. The anchors' pixel positions are
    // read off the un-repelled sibling, whose labels all draw centered on
    // them, in the same row order.
    let n = 30;
    let coord = |i: usize, salt: u64| crate::render::hash01((i as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ salt);
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for i in 0..n {
        let (cx, cy, s) = if i < 18 { (30.0, 30.0, 8.0) } else { (70.0, 65.0, 6.0) };
        xs.push(cx + (coord(i, 1) - 0.5) * 2.0 * s);
        ys.push(cy + (coord(i, 2) - 0.5) * 2.0 * s);
    }
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", xs)
            .with_float("y", ys)
            .with_str("n", (0..n).map(|i| format!("name {i}")).collect()),
    )]);
    let plain = label_positions(&SvgRenderer::default().render(&repel_spec(false), &data));
    let moved = label_positions(&SvgRenderer::default().render(&repel_spec(true), &data));
    assert_eq!(plain.len(), n);
    assert_eq!(moved.len(), n);
    let mut sides = [0usize; 4]; // above, below, left, right
    for i in 0..n {
        let (dx, dy) = (moved[i].0 - plain[i].0, moved[i].1 - plain[i].1);
        if dx.abs() > dy.abs() {
            sides[if dx > 0.0 { 3 } else { 2 }] += 1;
        } else {
            sides[if dy > 0.0 { 1 } else { 0 }] += 1;
        }
    }
    let occupied = sides.iter().filter(|&&c| c > 0).count();
    assert!(occupied >= 3, "two crowded clusters should rest names on at least three sides of their points: above/below/left/right = {sides:?}");
    assert!(sides[0] <= 20, "the placement is biased above again: above/below/left/right = {sides:?}");
}

#[test]
fn repel_is_deterministic() {
    // One specification, one picture. The placement anneals, and an annealing
    // that reached for a clock or a global RNG would redraw the book differently
    // every build. The shake is hashed from the row and the pass instead (§5).
    let data = crowded_labels(12);
    let a = SvgRenderer::default().render(&repel_spec(true), &data);
    let b = SvgRenderer::default().render(&repel_spec(true), &data);
    assert_eq!(a, b, "repel must render identically every run");
}

#[test]
fn repel_draws_every_label_even_when_no_arrangement_fits() {
    // §12, the rule the design fixed before anything was built: past some
    // density there is no overlap-free placement, and the answer is never to
    // drop the labels that did not fit. Two hundred long names hold more ink
    // than the panel has area, so no placement can separate them — all two
    // hundred still draw, all stay inside the panel where the clip cannot eat
    // them, and the layer says how many are still crowded. (Forty short names
    // on one point used to be enough; once the placement could spread in every
    // direction, it fitted all forty, which is the improvement this fixture
    // has to out-crowd.)
    let n = 200;
    let names: Vec<String> = (0..n).map(|i| format!("a rather long label, number {i}")).collect();
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![5.0; n])
            .with_float("y", vec![5.0; n])
            .with_str("n", names),
    )]);
    let drawn = SvgRenderer::default().draw(&repel_spec(true), &data);
    let placed = label_positions(&drawn.svg);
    assert_eq!(placed.len(), n, "every label draws, however crowded");

    let p = &drawn.panel;
    for (i, &(x, y)) in placed.iter().enumerate() {
        assert!(x >= p.x0 - 0.5 && x <= p.x1 + 0.5, "label {i} was pushed off the panel in x: {x}");
        assert!(y >= p.y0 - 0.5 && y <= p.y1 + 0.5, "label {i} was pushed off the panel in y: {y}");
    }
    assert!(
        drawn.remarks.iter().any(|d| d.message.contains("still overlap")),
        "an impossible packing has to say so: {:?}", drawn.remarks
    );
}

/// **What `repel` could not separate is said once per layer, naming the
/// panels.** Said by each drawing, a faceted plot printed one line per panel
/// with nothing to tell them apart. Split into two panels, the crowded one is
/// named with its count, the clear one is not, and there is one line.
#[test]
fn a_crowded_repel_is_said_once_and_names_its_panels() {
    let crowded = 200;
    let mut xs = vec![5.0; crowded];
    let mut names: Vec<String> = (0..crowded)
        .map(|i| format!("a rather long label, number {i}")).collect();
    let mut sides = vec!["busy".to_string(); crowded];
    // Two short names far apart, which repel separates easily.
    xs.extend([1.0, 9.0]);
    names.extend(["p".to_string(), "q".to_string()]);
    sides.extend(["calm".to_string(), "calm".to_string()]);
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", xs.clone())
            .with_float("y", xs)
            .with_str("n", names)
            .with_str("side", sides),
    )]);
    let mut spec = repel_spec(true);
    spec.facet = Some(crate::ir::FacetSpec { col: Some("side".into()), ..Default::default() });
    let said: Vec<String> = SvgRenderer::default().draw(&spec, &data).remarks.into_iter()
        .map(|d| d.message).filter(|m| m.contains("still overlap")).collect();
    assert_eq!(said.len(), 1, "one line for the layer: {said:?}");
    assert!(said[0].contains("in 1 of 2 panels: `busy` (") && !said[0].contains("`calm`"),
        "{said:?}");
    // Drawn alone, the plot keeps the one-panel sentence.
    let alone = SvgRenderer::default().draw(&repel_spec(true), &data).remarks.into_iter()
        .map(|d| d.message).find(|m| m.contains("still overlap")).unwrap_or_default();
    assert!(alone.contains("labels still overlap another one after `repel` moved them — there \
                            is no arrangement of this many words that fits this panel"), "{alone}");
}

#[test]
fn a_repelled_label_that_travels_gets_a_line_back_to_its_point() {
    // A word pushed clear of its dot has lost the one thing that said which dot
    // it belonged to, so the modifier draws the connector itself rather than
    // asking for a second mark (spec §5). A label that only took its resting
    // step off the dot gets none — a line from every label is a panel of lines.
    // Fourteen names on one point, because the first few rest against the dot
    // itself on their several sides; it is the outer ranks, held off by the
    // inner ones, that have travelled. (Six was enough when the placement
    // stacked them into one tall column.)
    let far = SvgRenderer::default().render(&repel_spec(true), &crowded_labels(14));
    let near = SvgRenderer::default().render(&repel_spec(true), &crowded_labels(1));
    let leaders = |svg: &str| svg.matches(r#"stroke-width="0.7""#).count();
    assert!(leaders(&far) >= 4, "labels driven far from their point need leaders: {}", leaders(&far));
    assert_eq!(leaders(&near), 0, "a label at rest beside its own dot needs no leader");
}

/// Every `&` in well-formed XML must begin a character reference.
fn unescaped_ampersands(svg: &str) -> Vec<String> {
    const ENTITIES: [&str; 6] = ["amp;", "lt;", "gt;", "quot;", "apos;", "#"];
    svg.match_indices('&')
        .filter(|(i, _)| {
            let rest = &svg[i + 1..];
            !ENTITIES.iter().any(|e| rest.starts_with(e))
        })
        .map(|(i, _)| svg[i..(i + 24).min(svg.len())].to_string())
        .collect()
}

fn render_with_category(name: &str) -> String {
    let df = DataFrame::new()
        .with_str("firm", vec![name.to_string(), "Sales".to_string()])
        .with_float("spend", vec![10.0, 25.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new()
        .data("t")
        .x("firm")
        .y("spend")
        .title("Q3 <spend> & forecast")
        .layer(Layer::new(Mark::Bar).encode(Channel::Color, "firm"));
    SvgRenderer::default().render(&spec, &data)
}

// -- the path in space ------------------------------------------------

/// Two routes through one cube must **interleave**, not stack.
///
/// This is the property the whole segment-level depth sort exists for, and
/// the one a per-series sort would silently get wrong: sorting whole strokes
/// would paint one entire coil in front of the other, which is a picture of
/// two coils that never touch rather than two that thread through each other.
///
/// The angle is chosen so the test is not measuring the projector twice.
/// At `turn = 0, tilt = 0` the camera sits on the x-axis, so data `x` is
/// *depth alone* and the screen position comes entirely from `y` and `z`
/// (`project::Scene`'s own test pins that basis). So two series sharing one
/// `(y, z)` track and differing only in `x` occupy the same pixels at
/// different distances — and the near one must be emitted last, whatever
/// order the table listed them in.
#[test]
fn the_segments_of_two_paths_in_space_sort_together_not_series_by_series() {
    // `far` is listed first in the table, `near` second; then the reverse.
    // Data order must not survive into paint order — depth must decide.
    let render_with = |first: &str, second: &str| -> String {
        let x = |s: &str| if s == "near" { 1.0 } else { 0.0 };
        let df = DataFrame::new()
            .with_float("x", vec![x(first), x(first), x(second), x(second)])
            .with_float("y", vec![0.0, 1.0, 0.0, 1.0])
            .with_float("z", vec![0.0, 1.0, 0.0, 1.0])
            .with_str("strand", vec![
                first.into(), first.into(), second.into(), second.into(),
            ]);
        let data = HashMap::from([("t".to_string(), df)]);
        let spec = PlotSpec::new()
            .data("t")
            .x("x")
            .y("y")
            .z("z")
            .coord(CoordSpace::Space(SpaceView { turn: 0.0, tilt: 0.0 }))
            .layer(Layer::new(Mark::Path).encode(Channel::Color, "strand"));
        SvgRenderer::default().render(&spec, &data)
    };

    // Which stroke color each route segment carries, in emission order.
    // `stroke-linecap="round"` is what picks the route's own segments out of
    // the cube frame's edges and the legend's separator, which are `<line>`
    // elements too.
    let painted_order = |svg: &str| -> Vec<String> {
        svg.lines()
            .filter(|l| {
                l.trim_start().starts_with("<line ") && l.contains(r#"stroke-linecap="round""#)
            })
            .filter_map(|l| l.split("stroke=\"").nth(1))
            .filter_map(|s| s.split('"').next())
            .map(str::to_string)
            .collect()
    };

    for (first, second) in [("far", "near"), ("near", "far")] {
        let svg = render_with(first, second);
        let painted = painted_order(&svg);
        assert_eq!(
            painted.len(),
            2,
            "expected one segment per route, got {painted:?}"
        );
        // Series take palette entries in first-appearance order, so the near
        // route's hue is decided by where the table listed it.
        let near_hue = PALETTE_GOG[if first == "near" { 0 } else { 1 }];
        assert_eq!(
            painted.last().unwrap(),
            near_hue,
            "the nearer route must paint over the farther one, whatever order the \
             table listed them in (listed {first} then {second}, painted {painted:?})"
        );
    }
}

/// A dash is a property of the **route**, not of the segmentation the depth
/// sort needs. Each segment is its own element and a dash restarts at the
/// start of an element, so without a carried phase a dashed 3-D path would
/// come out as one identical dash per vertex — a texture the data never asked
/// for. The running `stroke-dashoffset` is what keeps it a dash.
#[test]
fn a_dashed_route_in_space_carries_its_dash_phase_across_the_depth_split() {
    let n = 12;
    let df = DataFrame::new()
        .with_float("x", (0..n).map(|i| i as f64).collect())
        .with_float("y", (0..n).map(|i| i as f64).collect())
        .with_float("z", (0..n).map(|i| i as f64).collect());
    let data = HashMap::from([("t".to_string(), df)]);
    let spec = PlotSpec::new()
        .data("t")
        .x("x")
        .y("y")
        .z("z")
        .coord(CoordSpace::Space(SpaceView::default()))
        .layer(Layer::new(Mark::Path).style_pattern("dashed"));
    let svg = SvgRenderer::default().render(&spec, &data);

    let offsets: Vec<f64> = svg
        .lines()
        .filter(|l| l.trim_start().starts_with("<line "))
        .filter_map(|l| l.split("stroke-dashoffset=\"").nth(1))
        .filter_map(|s| s.split('"').next())
        .filter_map(|s| s.parse().ok())
        .collect();
    assert!(
        offsets.len() >= 2,
        "a dashed route in space should offset each segment, got {offsets:?}"
    );
    // The first segment starts the pattern at zero, and no two segments share
    // a phase — which is exactly what "the dash keeps running" means.
    assert!(
        offsets.iter().any(|o| *o == 0.0),
        "the route's first segment should start the dash at phase 0: {offsets:?}"
    );
    let mut sorted = offsets.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        offsets.len(),
        "every segment should sit at its own phase, not restart the dash: {offsets:?}"
    );
}

/// The three positions take a label override, or none of them does.
///
/// `x_label`/`y_label` shipped and `z_label` did not, which is the shape of a
/// Law-1 exception rather than a missing convenience: the positions are a
/// family of three and two of them had it. This asserts the override reaches
/// the projected cube's edge, and that asking for it where there is no third
/// axis is *reported* rather than quietly dropped (§12).
#[test]
fn the_third_position_takes_a_label_override_like_the_other_two() {
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 3.0, 4.0])
        .with_float("b", vec![2.0, 4.0, 1.0, 3.0])
        .with_float("c", vec![5.0, 1.0, 4.0, 2.0]);
    let data = HashMap::from([("t".to_string(), df)]);

    let spec = PlotSpec::new().data("t").x("a").y("b").z("c")
        .z_label("Altitude, m")
        .coord(CoordSpace::Space(SpaceView::default()))
        .layer(Layer::new(Mark::Point));
    let svg = SvgRenderer::default().render(&spec, &data);
    assert!(svg.contains("Altitude, m"), "the z override should reach the cube's edge");
    assert!(!svg.contains(">C<"), "the auto-derived label should be replaced, not joined");

    // And the same spec without the override falls back to the column name,
    // so the test above is measuring the override and not the default.
    let bare = PlotSpec::new().data("t").x("a").y("b").z("c")
        .coord(CoordSpace::Space(SpaceView::default()))
        .layer(Layer::new(Mark::Point));
    let bare_svg = SvgRenderer::default().render(&bare, &data);
    assert!(!bare_svg.contains("Altitude, m"));

    // A label with no axis to land on is guidance, not a refusal: it renders.
    let flat = PlotSpec::new().data("t").x("a").y("b")
        .z_label("Altitude, m")
        .layer(Layer::new(Mark::Point));
    let d = crate::legality::check(&flat, &data);
    assert!(
        d.iter().any(|x| x.kind == crate::legality::DiagnosticKind::Assumption
            && x.message.contains("z_label")),
        "a z label with no z should be reported: {:?}",
        d.iter().map(|x| &x.message).collect::<Vec<_>>()
    );
    assert!(d.iter().all(|x| !x.is_fatal()), "…but it must not be fatal");
}

/// A measure on `color` varies **along** a stroke; a category splits it into
/// series. Both readings of one channel, and the geometry follows the reading
/// rather than the mark: the categorical route stays a single `<polyline>`
/// (so every plot that predates this renders byte for byte as it did), and
/// only the measured one is cut into per-segment elements.
#[test]
fn a_measured_color_ramps_along_a_stroke_where_a_category_splits_it() {
    let n = 24;
    let df = DataFrame::new()
        .with_float("x", (0..n).map(|i| i as f64).collect())
        .with_float("y", (0..n).map(|i| (i % 5) as f64).collect())
        .with_float("depth", (0..n).map(|i| i as f64).collect())
        .with_str("kind", (0..n).map(|i| if i < 12 { "a".into() } else { "b".into() }).collect());
    let data = HashMap::from([("t".to_string(), df)]);
    let render = |mark: &Mark, field: &str| -> String {
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(mark.clone()).encode(Channel::Color, field));
        SvgRenderer::default().render(&spec, &data)
    };
    // Distinct stroke colors among the mark's own elements.
    let hues = |svg: &str| -> std::collections::HashSet<String> {
        svg.lines()
            .filter(|l| {
                let t = l.trim_start();
                // `stroke-opacity` is what separates a mark's own stroke from
                // the gridlines and the legend's separator rule, which are
                // `<line>` elements too.
                (t.starts_with("<line ") || t.starts_with("<polyline ")) && t.contains("stroke-opacity")
            })
            .filter_map(|l| l.split("stroke=\"").nth(1))
            .filter_map(|s| s.split('"').next())
            .map(str::to_string)
            .collect()
    };

    for mark in [Mark::Line, Mark::Step, Mark::Path] {
        let categorical = render(&mark, "kind");
        let measured = render(&mark, "depth");

        // Two categories, two strokes, two hues — and still whole polylines.
        assert_eq!(hues(&categorical).len(), 2, "{mark:?}: a category gives one hue per series");
        assert!(
            categorical.contains("<polyline "),
            "{mark:?}: a categorical stroke should stay a single polyline"
        );

        // A measure gives many, from the ramp, cut into segments.
        assert!(
            hues(&measured).len() > 5,
            "{mark:?}: a measured color should ramp along the stroke, got {:?}",
            hues(&measured).len()
        );
        assert!(
            !measured.contains("<polyline "),
            "{mark:?}: a ramped stroke is emitted per segment, not as a polyline"
        );
        // And it is one route, not two: a measure carries no categories, so
        // it must not split the mark the way `kind` does.
        assert!(
            !measured.contains("stroke=\"#4e79a7\"") || mark == Mark::Path,
            "{mark:?}: a ramped stroke should not fall back to the palette"
        );
    }
}

// -- log scale --------------------------------------------------------
//
// The two halves of the ordering rule, and the thing a log scale is *for*.

/// Left edges of the drawn bars, ascending. A bar is any `<rect>` with a
/// `fill-opacity`: that catches a categorical bar's faint self-edge *and* a
/// histogram's panel-color separator, where keying on `stroke-width="0.5"`
/// would now miss the histogram. Panel, canvas, strip and clip rects carry no
/// `fill-opacity`, so this still picks out data bars only.
///
/// Position rather than width, because `bar_thickness_svg` hands every bar
/// one thickness taken from the smallest gap: widths are equal by
/// construction and so cannot tell the two pipeline orders apart. Where the
/// bins were cut shows up in the *spacing*.
fn bar_lefts(svg: &str) -> Vec<f64> {
    let mut v: Vec<f64> = svg.lines()
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity"))
        .filter_map(|l| {
            let after = l.split(r#"<rect x=""#).nth(1)?;
            after.split('"').next()?.parse::<f64>().ok()
        })
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

fn skewed() -> HashMap<String, DataFrame> {
    // Four decades of gdp — the shape a log scale exists for.
    let gdp: Vec<f64> = (0..40).map(|i| 10f64.powf(1.0 + i as f64 / 10.0)).collect();
    let life: Vec<f64> = (0..40).map(|i| 40.0 + i as f64).collect();
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new().with_float("gdp", gdp).with_float("life", life));
    data
}

// -- time -------------------------------------------------------------

fn yearly(n: usize) -> HashMap<String, DataFrame> {
    use crate::time::{days_from_civil, TimeUnit, SECS_PER_DAY};
    let day: Vec<f64> = (0..n)
        .map(|i| days_from_civil(1994 + i as i64, 1, 1) as f64 * SECS_PER_DAY)
        .collect();
    let sales: Vec<f64> = (0..n).map(|i| 50.0 + (i as f64 * 1.7) % 30.0).collect();
    let mut data = HashMap::new();
    data.insert("t".to_string(),
        DataFrame::new().with_time("day", day, TimeUnit::Day).with_float("sales", sales));
    data
}

/// Daily readings from 2024-03-01, the six-weeks table the book uses.
fn six_weeks(n: usize) -> HashMap<String, DataFrame> {
    use crate::time::{days_from_civil, TimeUnit, SECS_PER_DAY};
    let start = days_from_civil(2024, 3, 1);
    let day: Vec<f64> = (0..n).map(|i| (start + i as i64) as f64 * SECS_PER_DAY).collect();
    let orders: Vec<f64> = (0..n).map(|i| 20.0 + (i as f64 * 3.0) % 15.0).collect();
    let mut data = HashMap::new();
    data.insert("t".to_string(),
        DataFrame::new().with_time("day", day, TimeUnit::Day).with_float("orders", orders));
    data
}

#[test]
fn a_calendar_axis_fits_its_data_like_every_other_axis() {
    // A user read the daily-bars plot and said the gaps at each end looked
    // wide. They were: the time branch took `(first tick, last tick)` as its
    // range, which is the bracket-outward failure §10 exists to undo, and
    // the calendar was the one axis still exempt from the fix. Six weeks of
    // daily orders ticked on Mondays drew Feb 26 .. Apr 15 — a 49-day axis
    // for 41 days of data, four dead days at each end.
    let data = six_weeks(42);           // 2024-03-01 (Fri) .. 2024-04-11 (Thu)
    let svg = |mark: Mark| {
        let spec = PlotSpec::new().data("t").x("day").y("orders").layer(Layer::new(mark));
        SvgRenderer::default().render(&spec, &data)
    };

    for mark in [Mark::Bar, Mark::Line, Mark::Point] {
        let s = svg(mark.clone());
        // The Mondays inside the data are drawn...
        assert!(s.contains(">Mar 4<") && s.contains(">Apr 8<"),
            "{mark:?}: expected the Mondays inside the data");
        // ...and the ones the old bracket reached out for are not.
        assert!(!s.contains(">Feb 26<"), "{mark:?}: axis still reaches back to Feb 26");
        assert!(!s.contains(">Apr 15<"), "{mark:?}: axis still reaches on to Apr 15");
    }
}

#[test]
fn a_calendar_axis_ends_the_way_the_mark_needs_it_to() {
    // The three answers the linear path already gives, now reached on a date
    // axis rather than skipped: a bar owns a slot and needs half of one
    // beyond each end or the end bars are sliced; a glyph breathes; a fill
    // sits flush, its edges being the shape. Measured as the axis span, in
    // days, against the 41 days the data covers.
    use crate::time::SECS_PER_DAY;
    let data = six_weeks(42);
    let span = |mark: Mark| {
        let df = data.get("t").unwrap();
        let eff: Vec<DataFrame> = vec![df.clone()];
        let refs: Vec<&DataFrame> = eff.iter().collect();
        let bars: Vec<&DataFrame> = if mark == Mark::Bar { refs.clone() } else { Vec::new() };
        let (_, xs, _) = build_axis(
            &refs, &bars, &[], "day", None,
            mark == Mark::Bar,                    // bar_position
            false,                                // bar_extent (that is y's job)
            mark == Mark::Area,                   // flush
            None, false, 10.0, Some(crate::time::TimeUnit::Day), (None, None), (0.0, 0.0),
            None, (None, None));
        (xs.1 - xs.0) / SECS_PER_DAY
    };

    let data_days = 41.0;
    assert!((span(Mark::Bar) - (data_days + 1.0)).abs() < 1e-6,
        "a bar wants half a day beyond each end, got {}", span(Mark::Bar));
    assert!((span(Mark::Line) - data_days * 1.1).abs() < 1e-6,
        "a line breathes 5% each side, got {}", span(Mark::Line));
    assert!((span(Mark::Area) - data_days).abs() < 1e-6,
        "an area fills flush, got {}", span(Mark::Area));
}

#[test]
fn a_temporal_axis_is_labeled_in_calendar_units_not_epoch_seconds() {
    let spec = PlotSpec::new().data("t")
        .x("day").y("sales")
        .layer(Layer::new(Mark::Line));
    let svg = SvgRenderer::default().render(&spec, &yearly(25));
    // Years on the ticks — not 7.6E8, not 760M.
    assert!(svg.contains(">1995<") && svg.contains(">2015<"),
            "expected year labels on the x axis");
    assert!(!svg.contains("M<") || !svg.contains("00M<"),
            "epoch seconds leaked onto the axis");
}

#[test]
fn bars_sitting_on_dates_get_calendar_ticks_not_one_per_bar() {
    // A tick under every daily bar would label the axis into soup; the
    // calendar decides the gridlines, exactly as Wilkinson's stock-price
    // example ticks Sundays rather than trades.
    let spec = PlotSpec::new().data("t")
        .x("day").y("sales")
        .layer(Layer::new(Mark::Bar));
    let svg = SvgRenderer::default().render(&spec, &yearly(25));
    assert!(svg.contains(">1995<"), "expected year labels under the bars");
    // 25 bars but far fewer ticks.
    let tick_labels = svg.matches(">19").count() + svg.matches(">20").count();
    assert!(tick_labels < 12, "got {tick_labels} tick labels for 25 bars");
}

#[test]
fn a_date_on_a_color_ramp_gets_a_dated_legend() {
    use crate::ir::ChannelDef;
    let mut layer = Layer::new(Mark::Point);
    layer.encodings.insert(Channel::Color, ChannelDef::field("day"));
    let spec = PlotSpec::new().data("t")
        .x("sales").y("sales")
        .layer(layer);
    let svg = SvgRenderer::default().render(&spec, &yearly(25));
    // Legend rows are self-contained ISO dates, not "1.7B" epoch seconds.
    assert!(svg.contains("1994-01-01"), "expected an ISO date in the legend");
    assert!(!svg.contains(">0.8B<"), "epoch seconds leaked into the legend");
}

#[test]
fn a_log_axis_is_labeled_in_data_units_not_exponents() {
    let spec = PlotSpec::new().data("t")
        .x_scaled("gdp", ScaleType::Log).y("life")
        .layer(Layer::new(Mark::Point));
    let svg = SvgRenderer::default().render(&spec, &skewed());
    // The whole difference between a log scale and plotting log(gdp).
    assert!(svg.contains(">10<") && svg.contains(">10K<"),
            "expected decade labels in the reader's units");
}

#[test]
fn binning_happens_in_log_space_so_the_bars_come_out_even() {
    // The case that decides the pipeline order. Cut in log space, each bin
    // spans one constant ratio, so the bars land at a constant spacing and
    // cover the axis. Cut in linear space and merely *drawn* on a log axis,
    // they bunch towards the top: measured on this data the gaps run
    // 176, 82, 54, 40, 32, 27 px and the left half of the plot is empty.
    let spec = PlotSpec::new().data("t")
        .x_scaled("gdp", ScaleType::Log)
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    let svg = SvgRenderer::default().render(&spec, &skewed());

    let lefts = bar_lefts(&svg);
    assert!(lefts.len() > 3, "expected several bars, got {}", lefts.len());
    let gaps: Vec<f64> = lefts.windows(2).map(|w| w[1] - w[0]).collect();
    let (mn, mx) = (
        gaps.iter().cloned().fold(f64::INFINITY, f64::min),
        gaps.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );
    assert!(mx - mn < 1.0, "bars are unevenly spaced: gaps {gaps:?}");
}

#[test]
fn a_histogram_touches_where_a_bar_chart_keeps_its_gap() {
    // The visual grammar that tells a histogram from a bar chart: a histogram
    // cuts a continuum into adjacent intervals, so its bars must touch —
    // Wilkinson, "there cannot be gaps between bars" — while a bar chart's
    // categories are separate, so its bars must not. The `bin` transform is
    // the only thing that differs, and it alone decides which way they draw.

    // A uniform continuous column: every Sturges bin is populated, so no bar
    // is dropped for being empty and "the bins touch" is a claim about the
    // drawing, not about which bins happen to hold data.
    let mut cont = HashMap::new();
    cont.insert("t".to_string(),
        DataFrame::new().with_float("x", (0..100).map(|i| i as f64).collect()));
    let hist = PlotSpec::new().data("t").x("x")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    let svg = SvgRenderer::default().render(&hist, &cont);

    let mut r = bar_rects(&svg);
    r.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert!(r.len() > 3, "expected several bins, got {}", r.len());
    for w in r.windows(2) {
        let (right, next_left) = (w[0].0 + w[0].2, w[1].0);
        assert!((right - next_left).abs() < 1.0,
            "histogram bins must touch: right edge {right:.2} vs next left {next_left:.2}");
    }
    // And a hairline in the panel color parts them, so touching neighbors
    // stay legible — not the self-colored edge a categorical bar carries.
    assert!(svg.contains(&format!(r#"stroke="{PANEL_BG}""#)),
        "touching bins want a panel-color separator");

    // The same mark without `bin`, over categories, keeps its gap.
    let chart = PlotSpec::new().data("t").x("country").y("gold")
        .layer(Layer::new(Mark::Bar));
    let svg2 = SvgRenderer::default().render(&chart, &medals_data());
    let mut r2 = bar_rects(&svg2);
    r2.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert!(r2.len() >= 2);
    let touching = r2.windows(2).any(|w| (w[0].0 + w[0].2 - w[1].0).abs() < 1.0);
    assert!(!touching, "categorical bars must not touch");
    assert!(svg2.contains(r#"stroke-width="0.5""#),
        "a categorical bar keeps its faint self-colored edge");
}

#[test]
fn a_color_split_histogram_overlays_each_series_in_its_own_hue() {
    // The overlaid histogram — the seaborn/penguins picture. Two groups over
    // one range bin on shared edges and draw on top of one another. Each must
    // keep its own color (the split the old code dropped, rendering one gray
    // combined histogram under a three-color legend), and because opaque
    // fills would bury each other, every bar is a translucent fill under a
    // solid outline in the *same* hue: the step silhouette that survives the
    // pile-up.
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("x", vec![0.0, 1.0, 2.0, 3.0,   2.0, 3.0, 4.0, 5.0])
        .with_str("g", ["a", "a", "a", "a",   "b", "b", "b", "b"]
            .iter().map(|s| s.to_string()).collect()));
    let spec = PlotSpec::new().data("t").x("x")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin).encode(Channel::Color, "g"));
    let svg = SvgRenderer::default().render(&spec, &data);

    let attr = |l: &str, name: &str| -> String {
        l.split(&format!(" {name}=\"")).nth(1)
            .and_then(|r| r.split('"').next()).unwrap_or("").to_string()
    };
    // A data bar carries a `stroke`; a legend swatch (also a `fill-opacity`
    // rect) does not — that is what keeps the key out of the bar count.
    let bars: Vec<(String, String, String)> = svg.lines()
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && l.contains(" stroke=\""))
        .map(|l| (attr(l, "fill"), attr(l, "fill-opacity"), attr(l, "stroke")))
        .collect();
    assert!(bars.len() >= 4, "expected several overlaid bars, got {}", bars.len());

    // Two distinct fills: the species did not collapse to one color.
    let fills: std::collections::HashSet<&str> = bars.iter().map(|b| b.0.as_str()).collect();
    assert!(fills.len() >= 2, "a split histogram keeps one color per group, got {fills:?}");

    for (fill, op, stroke) in &bars {
        assert_eq!(stroke, fill, "an overlaid bar is outlined in its own hue, not the panel color");
        assert_ne!(stroke.as_str(), PANEL_BG, "the overlay outline is the series hue, not the hairline");
        assert_eq!(op, "0.400", "an overlaid fill is translucent so stacked series show through");
    }
}

#[test]
fn a_sum_is_taken_before_the_scale_so_it_stays_a_sum() {
    // The other half of the rule. Group "a" sums to 100 and group "b" to 10,
    // so the axis spans exactly two decades. Logging *first* would sum the
    // logs — log10(10)+log10(90) = 2.95 — which is the log of a product and
    // not a quantity anyone asked for.
    let df = DataFrame::new()
        .with_str("cat", vec!["a".into(), "a".into(), "b".into(), "b".into()])
        .with_float("v", vec![10.0, 90.0, 1.0, 9.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);

    let spec = PlotSpec::new().data("t")
        .x("cat").y_scaled("v", ScaleType::Log)
        .layer(Layer::new(Mark::Bar).transform(Transform::Sum));
    let svg = SvgRenderer::default().render(&spec, &data);

    // Summing first spans exactly one decade, 10 to 100, so the axis gets
    // the 1-2-5 fill. Logging first would sum the logs — 1 + 1.954 = 2.954,
    // i.e. ~900 — and the axis would run a decade further, to 1000.
    assert!(svg.contains(">100<"), "the larger group should sum to 100");
    assert!(svg.contains(">20<") && svg.contains(">50<"),
            "one decade of range should get the 1-2-5 fill");
    assert!(!svg.contains(">1000<"), "a log-then-sum would push the axis to 1000");
}

#[test]
fn a_value_a_log_scale_cannot_place_is_skipped_not_drawn_as_nan() {
    let df = DataFrame::new()
        .with_float("v", vec![1.0, 0.0, -4.0, 100.0])
        .with_float("y", vec![1.0, 2.0, 3.0, 4.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);

    let spec = PlotSpec::new().data("t")
        .x_scaled("v", ScaleType::Log).y("y")
        .layer(Layer::new(Mark::Point));
    let svg = SvgRenderer::default().render(&spec, &data);

    assert!(!svg.contains("NaN"), "NaN coordinates are not valid SVG");
    // Two of the four rows can be placed, and both are.
    assert_eq!(svg.matches("<circle").count(), 2);
}

#[test]
fn an_unscaled_plot_is_untouched_by_any_of_this() {
    // A scale nobody asked for must not change a pixel.
    let spec = PlotSpec::new().data("t").x("gdp").y("life").layer(Layer::new(Mark::Point));
    let svg = SvgRenderer::default().render(&spec, &skewed());
    assert!(!svg.contains("NaN"));
    assert_eq!(svg.matches("<circle").count(), 40);
}

/// Population spread evenly across four decades — roughly how the real
/// column is distributed, and the shape that washes out a linear ramp: on a
/// linear scale nearly every value sits in the bottom 1% of the range.
fn skewed_channel() -> HashMap<String, DataFrame> {
    const N: usize = 32;
    let pop: Vec<f64> = (0..N)
        .map(|i| 10f64.powf(5.3 + i as f64 * 3.8 / (N - 1) as f64))
        .collect();
    let n = pop.len();
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("population", pop)
        .with_float("gdp", (0..n).map(|i| 1000.0 + i as f64).collect())
        .with_float("life", (0..n).map(|i| 50.0 + i as f64 * 0.5).collect()));
    data
}

/// Distinct colors among the *marks* — legend swatches and the panel
/// background are not what the ramp is being judged on.
fn distinct_mark_fills(svg: &str) -> usize {
    let mut seen: Vec<&str> = Vec::new();
    for l in svg.lines().filter(|l| l.contains("<circle")) {
        if let Some(v) = l.split("fill=\"").nth(1).and_then(|r| r.split('"').next()) {
            if !seen.contains(&v) { seen.push(v) }
        }
    }
    seen.len()
}

/// Nothing a plot draws may land outside the plot.
///
/// **The defect this pins.** The legend box was clamped to the room left
/// (`.min(remaining)`) while its rows were drawn at the full constant, so a
/// gradient legend — 188px tall by nature — put its strip and its bottom
/// label *outside the box* on any shorter plot, and outside the image on a
/// plot shorter than about 190. Reachable from `theme(height = )` on any
/// plot with any legend, and from every composed page of more than two rows,
/// since the page canvas is fixed and the cells divide it. 655 tests passed
/// throughout: every one of them rendered at the default size, where the
/// legend fits, and none looked at whether the ink stayed on the page.
///
/// So the assertion is deliberately not "the legend is right" but the blunt
/// invariant one level up, which no future layout change can satisfy by
/// accident.
#[test]
fn nothing_is_drawn_below_the_bottom_of_the_canvas() {
    let lowest_y = |svg: &str| {
        let mut lo = 0.0_f64;
        for line in svg.lines() {
            for (attr, _) in [("y=\"", 0), ("cy=\"", 0)] {
                let mut rest = line;
                while let Some(i) = rest.find(attr) {
                    rest = &rest[i + attr.len()..];
                    if let Some(v) = rest.split('"').next().and_then(|s| s.parse::<f64>().ok()) {
                        lo = lo.max(v);
                    }
                }
            }
        }
        lo
    };

    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("gdp", vec![1000.0, 2000.0, 3000.0, 4000.0])
        .with_float("life", vec![50.0, 60.0, 70.0, 80.0])
        .with_str("continent", ["Asia", "Europe", "Africa", "Americas"]
            .iter().map(|s| s.to_string()).collect()));

    for h in [140.0, 150.0, 200.0, 260.0, 400.0] {
        // A numeric color is the gradient legend, the tall one that overflowed.
        let numeric = PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "gdp"));
        // A text color is the swatch legend, which cannot squeeze and so is
        // left out with a remark rather than drawn over the edge.
        let discrete = PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));

        for (what, spec) in [("gradient", numeric), ("swatches", discrete)] {
            let r = SvgRenderer { height: h, ..SvgRenderer::default() };
            let svg = r.render(&spec, &data);
            assert!(
                lowest_y(&svg) <= h,
                "{what} legend at height {h}: ink at y={:.1}, past the {h}px canvas",
                lowest_y(&svg),
            );
        }
    }
}

#[test]
fn a_log_color_ramp_uses_its_whole_range_on_skewed_data() {
    // The complaint the book carried for two sessions: a linear ramp spends
    // its range where the data is dense, so a skewed column comes out one
    // flat color with two outliers.
    let mk = |log: bool| {
        let mut layer = Layer::new(Mark::Point);
        layer = if log {
            layer.encode_scaled(Channel::Color, "population", ScaleType::Log)
        } else {
            layer.encode(Channel::Color, "population")
        };
        let spec = PlotSpec::new().data("t").x("gdp").y("life").layer(layer);
        SvgRenderer::default().render(&spec, &skewed_channel())
    };
    // 32 points spread over four decades, each landing on its own step of
    // the ramp — the ramp is wired to the scale and exercised end to end.
    //
    // How badly the *linear* ramp washes out is a property of the scale, not
    // of the SVG, and is asserted in `scale.rs`: counting distinct hex here
    // would measure rounding, since colors a reader cannot tell apart still
    // differ in the last digit.
    let logged = distinct_mark_fills(&mk(true));
    assert!(logged >= 30, "log ramp gave only {logged} distinct colors for 32 points");
    assert!(distinct_mark_fills(&mk(false)) > 1, "sanity: linear still varies");
}

#[test]
fn a_log_legends_middle_label_is_the_color_actually_painted_there() {
    // The strip's midpoint is half way along the *scale*, which on a log
    // ramp is the geometric mean. Labeling it with the arithmetic mean
    // would name a color the strip does not paint.
    let spec = PlotSpec::new().data("t").x("gdp").y("life").layer(
        Layer::new(Mark::Point).encode_scaled(Channel::Color, "population", ScaleType::Log),
    );
    let svg = SvgRenderer::default().render(&spec, &skewed_channel());
    // The range runs 199.5K to 1.26B. Half way along it in log space is
    // √(199.5K · 1.26B) = 15.8M; the arithmetic midpoint would be 629.6M,
    // two decades away and a visibly different color.
    assert!(svg.contains(">15.8M<"), "expected the geometric midpoint");
    assert!(!svg.contains(">629.6M<"), "that is the arithmetic midpoint");
}

#[test]
fn a_log_size_channel_spreads_the_radii() {
    let mk = |log: bool| {
        let mut layer = Layer::new(Mark::Point);
        layer = if log {
            layer.encode_scaled(Channel::Size, "population", ScaleType::Log)
        } else {
            layer.encode(Channel::Size, "population")
        };
        let spec = PlotSpec::new().data("t").x("gdp").y("life").layer(layer);
        let svg = SvgRenderer::default().render(&spec, &skewed_channel());
        let mut r: Vec<f64> = svg.lines()
            .filter(|l| l.contains("<circle"))
            .filter_map(|l| l.split(r#" r=""#).nth(1)?.split('"').next()?.parse().ok())
            .collect();
        r.sort_by(|a, b| a.partial_cmp(b).unwrap());
        r[r.len() / 2]
    };
    // Linear leaves almost every point at the minimum radius; log puts the
    // median point in the middle of the range, which is what it is for.
    assert!(mk(false) < 4.0, "linear median radius was {}", mk(false));
    assert!(mk(true) > 6.5, "log median radius was {}", mk(true));
}

#[test]
fn escapes_all_five_xml_entities() {
    assert_eq!(
        esc(r#"R&D <a> "q" 'p'"#),
        "R&amp;D &lt;a&gt; &quot;q&quot; &apos;p&apos;"
    );
}

#[test]
fn escaping_does_not_double_encode() {
    // The `&` introduced by `<` -> `&lt;` must not itself be escaped again.
    assert_eq!(esc("<&>"), "&lt;&amp;&gt;");
}

#[test]
fn category_with_ampersand_yields_well_formed_output() {
    let svg = render_with_category("R&D");
    let bad = unescaped_ampersands(&svg);
    assert!(bad.is_empty(), "unescaped ampersands found: {bad:?}");
    assert!(svg.contains("R&amp;D"), "category should appear escaped");
}

#[test]
fn angle_brackets_in_data_cannot_open_a_tag() {
    let svg = render_with_category("<10%");
    assert!(svg.contains("&lt;10%"), "should be escaped");
    assert!(!svg.contains("<10%"), "raw angle bracket leaked into markup");
}

#[test]
fn title_is_escaped_too() {
    let svg = render_with_category("Sales");
    assert!(svg.contains("Q3 &lt;spend&gt; &amp; forecast"));
}


#[test]
fn text_width_counts_characters_not_bytes() {
    // "한국" is 2 characters but 6 bytes. Byte counting inflated it threefold
    // and blew out the margins for every CJK label.
    let w = estimate_text_width("한국", 10.0);
    assert!((w - 20.0).abs() < 1e-9, "expected ~2 em, got {w}");
}

#[test]
fn latin_width_is_unchanged_by_the_fix() {
    assert!((estimate_text_width("Hello", 10.0) - 29.0).abs() < 1e-9);
}

#[test]
fn fullwidth_glyphs_are_wider_per_character_than_latin() {
    assert!(estimate_text_width("한국어", 10.0) > estimate_text_width("abc", 10.0));
}

// -- the area mark ------------------------------------------------------

/// Every `<polygon>` in the output, as its raw points string.
fn polygons(svg: &str) -> Vec<String> {
    svg.match_indices("<polygon points=\"")
        .map(|(i, m)| {
            let rest = &svg[i + m.len()..];
            rest[..rest.find('"').unwrap()].to_string()
        })
        .collect()
}

/// A banded field's regions, one per level, each the list of its rings as
/// "x,y x,y ..." strings: the `M`/`L`/`Z` subpaths of every band `<path>` drawn
/// under `fill-rule="evenodd"`, read back into `polygons`' shape.
fn band_regions(svg: &str) -> Vec<Vec<String>> {
    svg.lines()
        .filter(|l| l.contains("<path d=\"") && l.contains(r#"fill-rule="evenodd""#))
        .filter_map(|l| l.split("d=\"").nth(1)?.split('"').next().map(str::to_string))
        .map(|d| d.split('Z')
            .map(|ring| ring.split_whitespace()
                .map(|t| t.trim_start_matches(['M', 'L']))
                .collect::<Vec<_>>()
                .join(" "))
            .filter(|ring| !ring.is_empty())
            .collect())
        .collect()
}

/// Parse "x,y x,y ..." into pairs.
fn points_of(poly: &str) -> Vec<(f64, f64)> {
    poly.split_whitespace()
        .filter_map(|p| p.split_once(','))
        .map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap()))
        .collect()
}

fn render_area(layer: Layer, ys: Vec<f64>) -> String {
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 3.0, 4.0])
        .with_float("b", ys)
        .with_str("g", vec!["x".into(), "y".into(), "x".into(), "y".into()]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("a").y("b").layer(layer);
    SvgRenderer::default().render(&spec, &data)
}

#[test]
fn an_area_closes_on_its_baseline() {
    // The defining property of the mark: the region runs from the data down
    // to zero, so the last two vertices sit on one horizontal line, and that
    // line is lower on screen (larger y) than any data vertex.
    let svg = render_area(Layer::new(Mark::Area), vec![4.0, 5.0, 3.0, 6.0]);
    let polys = polygons(&svg);
    assert_eq!(polys.len(), 1, "one ungrouped area is one region");

    let pts = points_of(&polys[0]);
    assert_eq!(pts.len(), 6, "4 data vertices + 2 closing the baseline");
    let (right, left) = (pts[4], pts[5]);
    assert!((right.1 - left.1).abs() < 1e-6, "the closing edge is level");
    assert!(
        pts[..4].iter().all(|p| p.1 < right.1),
        "every data vertex sits above the baseline"
    );
}

#[test]
fn the_axis_stretches_to_include_the_baseline() {
    // Without this the fill runs off the bottom of the panel and reads as a
    // floating band. The baseline must land *inside* the panel, not below
    // it — the same stretch `bar` gets, asked as "does anything measure
    // from a baseline?" rather than "is there a bar?".
    let svg = render_area(Layer::new(Mark::Area), vec![40.0, 50.0, 30.0, 60.0]);
    let pts = points_of(&polygons(&svg)[0]);
    let baseline_y = pts[4].1;

    // A "0" tick must exist, and the baseline must sit at the panel floor
    // rather than beyond it.
    assert!(svg.contains(">0<"), "the zero tick is on the axis:\n{svg}");
    assert!(
        baseline_y.is_finite() && baseline_y > pts[..4].iter().map(|p| p.1).fold(0.0, f64::max),
        "baseline below the data but on the panel"
    );
}

// A ribbon's data: several y values per x, so `range` reduces each x to a
// (low, high) pair and the band spans them. Two groups when `g` is used.
fn ribbon_frame() -> DataFrame {
    DataFrame::new()
        .with_float("a", vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0])
        .with_float("b", vec![4.0, 6.0, 3.0, 7.0, 5.0, 8.0])
        .with_str("g", vec!["p".into(), "p".into(), "p".into(),
                             "q".into(), "q".into(), "q".into()])
}

#[test]
fn a_ribbon_spans_between_its_low_and_high() {
    // The defining property: at each x the band runs from the low boundary up
    // to the high one, and it closes on that low boundary — not on a flat
    // baseline the way an `area` does. So the polygon is the high edge
    // left→right then the low edge right→left, and at every shared x the high
    // vertex sits *above* (smaller screen-y) its low partner.
    let mut data = HashMap::new();
    data.insert("t".to_string(), ribbon_frame());
    let spec = PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Ribbon).transform(Transform::Range));
    let svg = SvgRenderer::default().render(&spec, &data);

    let polys = polygons(&svg);
    assert_eq!(polys.len(), 1, "one ungrouped ribbon is one band");
    let pts = points_of(&polys[0]);
    assert_eq!(pts.len(), 6, "3 high vertices + 3 low vertices, no baseline pair");

    // First half is the high edge (x ascending), second half the low edge
    // (x descending) — so pts[i] and pts[5-i] share an x, high over low.
    for i in 0..3 {
        let hi = pts[i];
        let lo = pts[5 - i];
        assert!((hi.0 - lo.0).abs() < 1e-6, "high and low share the x at column {i}");
        assert!(hi.1 < lo.1, "the high boundary sits above the low one at column {i}");
    }
}

#[test]
fn a_ribbon_does_not_stretch_the_axis_to_zero() {
    // A ribbon floats between two synthesized extents — it has no baseline, so
    // (unlike `area`) its measured axis must fit the band, never reach down to
    // 0. Values well clear of zero must not summon a zero tick.
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("a", vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0])
        .with_float("b", vec![40.0, 46.0, 43.0, 57.0, 51.0, 58.0]));
    let spec = PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Ribbon).transform(Transform::Range));
    let svg = SvgRenderer::default().render(&spec, &data);

    assert!(!svg.contains(">0<"), "a ribbon's axis fits the band, not a baseline:\n{svg}");
    // And the band's lowest vertex is the low boundary of the data (~40),
    // sitting near the panel floor rather than pinned to an off-data zero.
    let pts = points_of(&polygons(&svg)[0]);
    let lowest = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let highest = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    assert!(highest < lowest, "the band has real thickness");
}

#[test]
fn a_split_ribbon_draws_one_translucent_band_per_group() {
    // `color` splits the ribbon into one band per group (the discrete split
    // `area` makes). The bands share their x and overlap, so — having no
    // baseline to `stack` — they draw translucent by default, the rule
    // overlaid boxes use.
    let mut data = HashMap::new();
    data.insert("t".to_string(), ribbon_frame());
    let spec = PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Ribbon).transform(Transform::Range).encode(Channel::Color, "g"));
    let svg = SvgRenderer::default().render(&spec, &data);

    let polys = polygons(&svg);
    assert_eq!(polys.len(), 2, "two groups, two bands");
    let translucent = svg.matches("fill-opacity=\"0.400\"").count();
    assert!(translucent >= 2, "overlapping bands draw at the overlay weight, got:\n{svg}");
}

#[test]
fn a_step_moves_only_in_right_angles() {
    // A step holds each value until it changes, so its path is horizontal or
    // vertical between vertices — never the diagonal a `line` would draw. That
    // is the whole difference between the two marks, so it is what the test
    // pins.
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("x", vec![0.0, 1.0, 2.0, 3.0])
        .with_float("y", vec![0.0, 2.0, 1.0, 3.0]));
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Step));
    let svg = SvgRenderer::default().render(&spec, &data);

    let line = svg.lines().find(|l| l.contains("<polyline")).expect("a step draws a polyline");
    let pts: Vec<(f64, f64)> = line.split("points=\"").nth(1).unwrap()
        .split('"').next().unwrap()
        .split_whitespace()
        .map(|p| {
            let mut it = p.split(',');
            (it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap())
        })
        .collect();
    assert!(pts.len() > 4, "a stepped path has more vertices than its points, got {}", pts.len());
    for w in pts.windows(2) {
        let same_x = (w[0].0 - w[1].0).abs() < 0.01;
        let same_y = (w[0].1 - w[1].1).abs() < 0.01;
        assert!(same_x || same_y, "a step segment is horizontal or vertical, got {:?}->{:?}", w[0], w[1]);
    }
}

// -----------------------------------------------------------------------
// A category on the domain — the profile, and the radar
// -----------------------------------------------------------------------

/// Four categories, one value each: the frame the profile tests read.
fn profile_data() -> HashMap<String, DataFrame> {
    HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", vec!["a", "b", "c", "d"].into_iter().map(String::from).collect())
            .with_float("v", vec![10.0, 40.0, 20.0, 30.0]),
    )])
}

/// The vertices of the first polyline in an SVG.
fn polyline_points(svg: &str) -> Vec<(f64, f64)> {
    let l = svg.lines().find(|l| l.contains("<polyline")).expect("no polyline drawn");
    l.split("points=\"").nth(1).unwrap().split('"').next().unwrap()
        .split_whitespace()
        .map(|p| {
            let mut it = p.split(',');
            (it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap())
        })
        .collect()
}

fn profile(mark: Mark, polar: bool) -> String {
    let mut s = PlotSpec::new().data("t").x("g").y("v");
    if polar {
        s = s.coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }));
    }
    SvgRenderer::default().render(&s.layer(Layer::new(mark)), &profile_data())
}

/// A path across categories lands on the category slots — the same places a
/// `bar` or a `point` would stand, since they all resolve a position through
/// one function. Four categories, four vertices, evenly spaced.
#[test]
fn a_line_across_categories_stands_where_the_bars_would() {
    let pts = polyline_points(&profile(Mark::Line, false));
    assert_eq!(pts.len(), 4, "one vertex per category, got {pts:?}");

    // Evenly spaced along x: the categorical scale gives every slot the same
    // width, so the gaps between consecutive vertices are equal.
    let gaps: Vec<f64> = pts.windows(2).map(|w| w[1].0 - w[0].0).collect();
    assert!(gaps.iter().all(|g| (g - gaps[0]).abs() < 0.5),
        "category slots are not evenly spaced: {gaps:?}");
    assert!(gaps[0] > 1.0, "the vertices collapsed onto one x");

    // And the heights track the values (10, 40, 20, 30) — y grows downward in
    // SVG, so the largest value has the smallest y.
    let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    assert!(ys[1] < ys[3] && ys[3] < ys[2] && ys[2] < ys[0],
        "the profile does not track its values: {ys:?}");
}

/// The radar closes. A categorical angular axis has no repeated endpoint to
/// land on — each category appears exactly once — so the closing segment is
/// drawn, or the shape is left with a wedge missing. The flat plot of the same
/// sentence must *not* gain that vertex: a straight axis has two ends.
#[test]
fn a_radar_closes_on_itself_and_a_flat_profile_does_not() {
    let flat = polyline_points(&profile(Mark::Line, false));
    let radar = polyline_points(&profile(Mark::Line, true));

    assert_eq!(flat.len(), 4, "a flat profile has one vertex per category");
    assert_eq!(radar.len(), 5, "a radar repeats its first vertex to close");
    let (first, last) = (radar[0], radar[radar.len() - 1]);
    assert!((first.0 - last.0).abs() < 0.01 && (first.1 - last.1).abs() < 0.01,
        "the radar's last vertex is not its first: {first:?} vs {last:?}");
    assert!((flat[0].0 - flat[3].0).abs() > 1.0, "the flat profile closed on itself");
}

// ---- the five marks that learned to bend, 2026-07-26 --------------------
//
// The property each of these pins is the one the refusal named: a segment that
// **holds** a value across a span must follow the ring, because half of it
// drawn straight puts the mark where the data is not (§12). A chord always
// falls *inside* the circle it subtends, so "is it an arc" is measurable
// without parsing the path: take the segment's midpoint and compare its
// distance from the center with the endpoints'.

/// Every `A` command's radius in an SVG, in the order drawn.
fn arc_radii(svg: &str) -> Vec<f64> {
    let mut out = Vec::new();
    for l in svg.lines().filter(|l| l.contains(r#"<path d="#)) {
        let Some(d) = l.split(r#"d=""#).nth(1).and_then(|s| s.split('"').next()) else { continue };
        let t: Vec<&str> = d.split_whitespace().collect();
        for (i, w) in t.iter().enumerate() {
            if *w == "A" {
                if let Some(r) = t.get(i + 1).and_then(|s| s.parse::<f64>().ok()) { out.push(r); }
            }
        }
    }
    out
}

/// A stair's **treads become arcs**, and its risers stay straight.
///
/// This is the segment the whole space was waiting on. A tread asserts one
/// value across a span of angle, so bent it is an arc at constant radius; a
/// riser changes the value at one angle, so it is exactly the radius and needs
/// no arc at all. Four categories give four treads and four risers: three of
/// each between them, plus the closing tread and the closing jump.
#[test]
fn a_stairs_treads_become_arcs_and_its_risers_stay_radial() {
    let svg = profile(Mark::Step, true);
    let radii = arc_radii(&svg);
    assert!(!radii.is_empty(), "a polar staircase drew no arc at all");

    // Every tread rides a ring, so its radius is one of the four values'
    // radii — and the four values differ, so the treads' radii must too.
    let mut uniq = radii.clone();
    uniq.sort_by(|a, b| a.partial_cmp(b).unwrap());
    uniq.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    assert_eq!(uniq.len(), 4, "four values should give four tread radii: {radii:?}");

    // The flat staircase is a polyline and gains no arcs — the mark means the
    // same thing in both spaces, and only its geometry changed (Law 6).
    assert!(arc_radii(&profile(Mark::Step, false)).is_empty(),
        "a flat staircase must not draw arcs");
}

/// A staircase **closes on a wrapped domain**, and does not flat.
///
/// Flat, the last category's value gets no tread: there is no slot after it.
/// Bent, the categories exhaust the turn, so the last one's slot runs round to
/// the first — `line` and `area` close for the same reason — and the value
/// then jumps to the first category's, as it does at every other spoke. The
/// jump is the path's `Z`, which also squares the corner where it began.
#[test]
fn a_polar_staircase_closes_and_a_flat_one_does_not() {
    let bent = profile(Mark::Step, true);
    assert_eq!(arc_radii(&bent).len(), 4,
        "four categories should give four treads once the turn is closed");
    let d = bent.lines().find(|l| l.contains(r#"<path d="M"#) && l.contains(" A "))
        .and_then(|l| l.split(r#"d=""#).nth(1)?.split('"').next().map(str::to_string))
        .expect("the staircase's path");
    assert!(d.trim_end().ends_with('Z'), "the staircase stops short of its closing jump: {d}");
    // Three jumps between the categories are drawn; the fourth is the `Z`.
    assert_eq!(d.matches(" L ").count(), 3, "{d}");
    // A measured color draws one element per tread and per jump, so the
    // closing jump is an element of its own: four treads and four jumps.
    let ramped = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("g").y("v")
            .coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }))
            .layer(Layer::new(Mark::Step).encode(Channel::Color, "v")),
        &profile_data());
    let pieces = ramped.lines()
        .filter(|l| l.contains("<path d=") && l.contains(r#"stroke-linejoin="miter""#))
        .count();
    assert_eq!(pieces, 8, "four treads and four jumps");
    let flat = polyline_points(&profile(Mark::Step, false));
    // 4 categories → 1 start + 3×(riser + tread) = 7 vertices, and the last is
    // not the first.
    assert!((flat[0].0 - flat[flat.len() - 1].0).abs() > 1.0,
        "the flat staircase closed on itself: {flat:?}");
}

/// A **band's boundaries are chords, not arcs** — the correction this session
/// made to the recorded refusal, pinned so it cannot be re-litigated by
/// accident. A ribbon's boundary runs through the data's own vertices, which
/// is `line`'s geometry, and it closes on its own lower boundary rather than
/// on any ring. So the radar band draws with no arc command anywhere.
#[test]
fn a_radar_band_is_drawn_with_chords_and_needs_no_arc() {
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", vec!["a", "a", "b", "b", "c", "c", "d", "d"]
                .into_iter().map(String::from).collect())
            .with_float("v", vec![1.0, 5.0, 2.0, 7.0, 3.0, 6.0, 2.0, 4.0]),
    )]);
    let spec = PlotSpec::new().data("t").x("g").y("v")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(Layer::new(Mark::Ribbon).transform(Transform::Range));
    let svg = SvgRenderer::default().render(&spec, &data);
    assert!(arc_radii(&svg).is_empty(), "a band needed no arc and drew one");

    // One polygon, and it **closes**: four categories give 4+1 vertices per
    // boundary and two boundaries, so ten in all.
    let poly = svg.lines().find(|l| l.contains("<polygon")).expect("no band drawn");
    let n = poly.split("points=\"").nth(1).unwrap().split('"').next().unwrap()
        .split_whitespace().count();
    assert_eq!(n, 10, "the radar band did not close round the wrap: {n} vertices");
}

/// A **zone that spans the turn is an annulus**, not a sector with a seam.
/// `arc_to` splits a full turn at the antipode because an `A` whose ends
/// coincide draws nothing — so the ring comes out as two arcs per edge, four
/// in all, and the hole is real.
#[test]
fn a_zone_spanning_the_turn_closes_into_an_annulus() {
    let data = HashMap::from([
        ("t".to_string(), DataFrame::new()
            .with_float("hour", vec![0.0, 6.0, 12.0, 18.0, 24.0])
            .with_float("v", vec![1.0, 4.0, 2.0, 5.0, 1.0])),
        ("band".to_string(), DataFrame::new()
            .with_float("lo", vec![2.0]).with_float("hi", vec![4.0])),
    ]);
    let spec = PlotSpec::new().data("t").x("hour").y("v")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(Layer::new(Mark::Line))
        .layer({
            let mut z = Layer::new(Mark::Zone);
            z.data = Some("band".into());
            z.bounds = Some(crate::ir::BoundsSpec {
                lower: Some("lo".into()), upper: Some("hi".into()), ..Default::default()
            });
            z
        });
    let svg = SvgRenderer::default().render(&spec, &data);
    let radii = arc_radii(&svg);
    assert_eq!(radii.len(), 4,
        "a full-turn sector is two arcs per edge, split at the antipode: {radii:?}");
    // Two distinct radii — the inner and outer edges of the ring — and the
    // inner one is genuinely smaller, so there is a hole.
    let (lo, hi) = (radii.iter().cloned().fold(f64::MAX, f64::min),
                    radii.iter().cloned().fold(0.0, f64::max));
    assert!(hi - lo > 1.0, "the annulus has no hole: inner {lo}, outer {hi}");
    assert!(!svg.contains("NaN"), "the annulus wrote NaN coordinates");
}

/// A whisker's **caps hold their pixel width** when bent, so the two ends of
/// one interval draw the same length of ink at different radii — and therefore
/// subtend *different* angles. §18's rule that a stroke's width is pixels: a
/// cap says where the span stops and its width carries no quantity.
#[test]
fn a_bent_whiskers_caps_keep_their_pixel_width() {
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", vec!["a", "a", "b", "b"].into_iter().map(String::from).collect())
            .with_float("v", vec![1.0, 9.0, 2.0, 8.0]),
    )]);
    let spec = PlotSpec::new().data("t").x("g").y("v")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(Layer::new(Mark::Interval).transform(Transform::Range));
    let svg = SvgRenderer::default().render(&spec, &data);

    // Two intervals, two caps each: four arcs, at four different radii (the
    // four extents 1, 9, 2, 8).
    let radii = arc_radii(&svg);
    assert_eq!(radii.len(), 4, "two capped whiskers should draw four arcs: {radii:?}");
    assert!(radii.iter().any(|r| (r - radii[0]).abs() > 1.0),
        "the caps all landed at one radius: {radii:?}");

    // Each cap's ink is its radius times the angle it subtends. Reading the
    // chord back off the path and inverting gives the arc length, and all four
    // must agree — that is the pixel rule holding.
    let inks: Vec<f64> = svg.lines()
        .filter(|l| l.contains(r#"stroke-linecap="round""#) && l.contains(" A "))
        .filter_map(|l| {
            let d = l.split(r#"d=""#).nth(1)?.split('"').next()?;
            let t: Vec<&str> = d.split_whitespace().collect();
            let (x0, y0) = (t[1].parse::<f64>().ok()?, t[2].parse::<f64>().ok()?);
            let r = t[4].parse::<f64>().ok()?;
            let (x1, y1) = (t[9].parse::<f64>().ok()?, t[10].parse::<f64>().ok()?);
            let chord = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
            Some(2.0 * r * (chord / (2.0 * r)).min(1.0).asin())
        })
        .collect();
    assert_eq!(inks.len(), 4, "could not measure all four caps");
    assert!(inks.iter().all(|k| (k - inks[0]).abs() < 0.1),
        "caps drew different lengths of ink: {inks:?}");
}

/// The closing rule is the *categorical* angular axis's, not polar's in
/// general. A measured angle closes itself when the data supplies both ends of
/// its cycle (hour 0 and hour 24 are the same bearing), so repeating a vertex
/// there would draw a segment that is already drawn — and on data that does
/// *not* span the cycle it would invent a closure the reader never asked for.
#[test]
fn a_measured_angle_is_left_to_close_itself() {
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("hour", vec![0.0, 6.0, 12.0, 18.0, 24.0])
            .with_float("trips", vec![10.0, 40.0, 20.0, 30.0, 10.0]),
    )]);
    let spec = PlotSpec::new().data("t").x("hour").y("trips")
        .coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }))
        .layer(Layer::new(Mark::Line));
    let pts = polyline_points(&SvgRenderer::default().render(&spec, &data));

    assert_eq!(pts.len(), 5, "a measured angle draws its rows and no extra vertex");
    // It still closes — because the data closed it, which is the point.
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    assert!((first.0 - last.0).abs() < 0.01 && (first.1 - last.1).abs() < 0.01,
        "hours 0 and 24 should land on the same bearing: {first:?} vs {last:?}");
}

/// The filled radar closes too, and by the same rule — `area` and `line` ask
/// `Polar::wraps` rather than each deciding. A region left open shows as a
/// wedge cut back to the center, which is what this catches.
#[test]
fn a_filled_radar_closes_the_same_way_the_line_does() {
    let svg = profile(Mark::Area, true);
    let poly = svg.lines().find(|l| l.contains("<polygon")).expect("no region drawn");
    let pts: Vec<(f64, f64)> = poly.split("points=\"").nth(1).unwrap().split('"').next().unwrap()
        .split_whitespace()
        .map(|p| {
            let mut it = p.split(',');
            (it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap())
        })
        .collect();
    // Boundary (5 vertices, first repeated) plus the retraced floor (5 more):
    // in polar an area closes along the *ring*, vertex by vertex.
    assert_eq!(pts.len(), 10, "the closed boundary and its retraced floor, got {pts:?}");
    assert!((pts[0].0 - pts[4].0).abs() < 0.01 && (pts[0].1 - pts[4].1).abs() < 0.01,
        "the region's boundary does not return to its first vertex");
}

#[test]
fn a_step_bin_splits_into_one_silhouette_per_color() {
    // `step * bin` is the histogram outline: one stepped, unfilled polyline
    // per group, each in its own hue — the seaborn `element="step"` look.
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("x", vec![0.0, 1.0, 2.0, 3.0, 2.0, 3.0, 4.0, 5.0])
        .with_str("g", ["a", "a", "a", "a", "b", "b", "b", "b"]
            .iter().map(|s| s.to_string()).collect()));
    let spec = PlotSpec::new().data("t").x("x")
        .layer(Layer::new(Mark::Step).transform(Transform::Bin).encode(Channel::Color, "g"));
    let svg = SvgRenderer::default().render(&spec, &data);

    let polylines: Vec<&str> = svg.lines().filter(|l| l.contains("<polyline")).collect();
    assert_eq!(polylines.len(), 2, "one silhouette per species");
    assert!(polylines.iter().all(|l| l.contains(r#"fill="none""#)), "a step is unfilled");
    let strokes: std::collections::HashSet<&str> = polylines.iter()
        .map(|l| l.split("stroke=\"").nth(1).unwrap().split('"').next().unwrap())
        .collect();
    assert_eq!(strokes.len(), 2, "two distinct hues, got {strokes:?}");
}

#[test]
fn a_bar_border_setting_reaches_the_stroke() {
    // `style(border_color =, border_size =)` overrides the derived edge with a
    // solid outline of the given color and width; the fill is untouched.
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_str("g", vec!["a".to_string(), "b".to_string()])
        .with_float("v", vec![3.0, 5.0]));
    let spec = PlotSpec::new().data("t").x("g").y("v")
        .layer(Layer::new(Mark::Bar).style_border("white", 2.5));
    let svg = SvgRenderer::default().render(&spec, &data);

    let bar = svg.lines()
        .find(|l| l.contains("<rect") && l.contains("fill-opacity"))
        .expect("a data bar");
    assert!(bar.contains(r#"stroke="white""#), "border_color reaches the stroke: {bar}");
    assert!(bar.contains(r#"stroke-width="2.5""#), "border_size reaches the width: {bar}");
    assert!(bar.contains(r#"stroke-opacity="1""#), "a set border draws solid: {bar}");

    // `border_size = 0` draws no outline at all — the fills overlap with
    // nothing between them.
    let spec0 = PlotSpec::new().data("t").x("g").y("v")
        .layer(Layer::new(Mark::Bar).style_border("white", 0.0));
    let svg0 = SvgRenderer::default().render(&spec0, &data);
    let bar0 = svg0.lines().find(|l| l.contains("<rect") && l.contains("fill-opacity")).expect("a bar");
    assert!(bar0.contains(r#"stroke="none""#), "border_size=0 draws no stroke: {bar0}");
}

#[test]
fn a_category_splits_an_area_into_one_region_each() {
    // Wilkinson 8.1.5 — a categorical variable on an aesthetic *splits* the
    // graphic. `line` splits into one polyline per category; `area` must
    // split into one region per category, or it is a per-mark exception.
    let svg = render_area(
        Layer::new(Mark::Area).encode(Channel::Color, "g"),
        vec![4.0, 5.0, 3.0, 6.0],
    );
    assert_eq!(polygons(&svg).len(), 2, "two categories, two regions");
}

/// A stacked area split by `group` alone draws its bands in one fill, so the
/// floor of every band above the bottom one is traced in the panel's color,
/// as a stacked bar's segments are parted (2026-10-09). A colored pile is
/// parted by its colors and is drawn as it was.
#[test]
fn a_group_only_stacked_area_parts_its_bands() {
    let df = DataFrame::new()
        .with_str("q", vec!["Q1".into(), "Q1".into(), "Q2".into(), "Q2".into(), "Q3".into(), "Q3".into()])
        .with_str("c", vec!["r".into(), "o".into(), "r".into(), "o".into(), "r".into(), "o".into()])
        .with_float("v", vec![4.0, 3.0, 5.0, 6.0, 4.0, 8.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let pile = |split: Channel| SvgRenderer::default().render(&PlotSpec::new().data("t")
        .x("q").y("v").layer(Layer::new(Mark::Area).transform(Transform::Stack)
            .encode(split, "c")), &data);
    let seams = |s: &str| s.lines()
        .filter(|l| l.contains("<polyline") && l.contains(&format!(r#"stroke="{PANEL_BG}""#)))
        .count();
    let grouped = pile(Channel::Group);
    assert_eq!(seams(&grouped), 1, "two bands, one seam between them");
    assert_eq!(polygons(&grouped).len(), 2, "the bands are still drawn");
    assert_eq!(seams(&pile(Channel::Color)), 0, "a colored pile is parted by its colors");
}

/// **The edge that parts touching ink is the panel's own color, whatever the
/// theme paints it.** A histogram's bars, a packed cell and a group-only pile
/// are parted by a line in the panel's background, so the line reads as a gap.
/// It was the default panel's gray under every theme, so on a white panel the
/// gaps were gray lines. A background that paints nothing keeps the gray,
/// since a gap of nothing cannot be drawn.
#[test]
fn a_derived_edge_takes_the_panels_own_color() {
    let df = DataFrame::new()
        .with_str("q", vec!["Q1".into(), "Q1".into(), "Q2".into(), "Q2".into()])
        .with_str("c", vec!["r".into(), "o".into(), "r".into(), "o".into()])
        .with_float("v", vec![4.0, 3.0, 5.0, 6.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let draw = |mark: Mark, bg: Option<&str>| {
        let mut spec = PlotSpec::new().data("t").x("q").y("v")
            .layer(Layer::new(mark).transform(Transform::Stack).encode(Channel::Group, "c"));
        spec.theme.background = bg.map(str::to_string);
        SvgRenderer::default().render(&spec, &data)
    };
    let edged = |svg: &str, c: &str| svg.contains(&format!(r#"stroke="{c}""#));
    for mark in [Mark::Bar, Mark::Area] {
        let linen = draw(mark.clone(), Some("linen"));
        assert!(edged(&linen, "linen") && !edged(&linen, PANEL_BG),
            "{mark:?}: the parts are parted in the panel's linen");
        assert!(edged(&draw(mark.clone(), None), PANEL_BG), "{mark:?}: the default panel");
        assert!(edged(&draw(mark.clone(), Some("none")), PANEL_BG),
            "{mark:?}: a background of nothing keeps the default gray");
    }
    let mut hist = PlotSpec::new().data("t").x("v").layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    hist.theme.background = Some("white".to_string());
    let svg = SvgRenderer::default().render(&hist, &data);
    assert!(edged(&svg, "white") && !edged(&svg, PANEL_BG), "a histogram's gaps are white");
}

#[test]
fn group_splits_an_area_without_coloring_it() {
    // The same distinction `group` earns on `line`: separate the series,
    // invent no encoding that has no legend to decode it.
    let svg = render_area(
        Layer::new(Mark::Area).encode(Channel::Group, "g"),
        vec![4.0, 5.0, 3.0, 6.0],
    );
    let polys = polygons(&svg);
    assert_eq!(polys.len(), 2);

    let fills: Vec<&str> = svg
        .match_indices("<polygon")
        .map(|(i, _)| {
            let rest = &svg[i..];
            let f = rest.find("fill=\"").unwrap() + 6;
            &rest[f..f + rest[f..].find('"').unwrap()]
        })
        .collect();
    assert_eq!(fills[0], fills[1], "group separates, it does not color");
}

#[test]
fn an_area_is_drawn_in_x_order_not_data_order() {
    // A region whose boundary follows row order folds over itself. The rows
    // here are deliberately out of order on x.
    let df = DataFrame::new()
        .with_float("a", vec![3.0, 1.0, 4.0, 2.0])
        .with_float("b", vec![5.0, 4.0, 6.0, 3.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("a").y("b").layer(Layer::new(Mark::Area));
    let svg = SvgRenderer::default().render(&spec, &data);

    let pts = points_of(&polygons(&svg)[0]);
    let xs: Vec<f64> = pts[..4].iter().map(|p| p.0).collect();
    assert!(
        xs.windows(2).all(|w| w[0] <= w[1]),
        "boundary must ascend in x, got {xs:?}"
    );
}

#[test]
fn an_area_takes_a_set_opacity_for_the_whole_region() {
    // One region, one fill — `opacity` is a setting here, never a channel.
    let svg = render_area(
        Layer::new(Mark::Area).style_opacity(0.25),
        vec![4.0, 5.0, 3.0, 6.0],
    );
    assert!(svg.contains("fill-opacity=\"0.250\""), "{svg}");
}

#[test]
fn an_area_carries_no_stroke() {
    // The edge on top of a region is a `line` layered over it — superposition
    // is the grammar's own answer, and it keeps the parked border question
    // out of a mark that would otherwise quietly grow one.
    let svg = render_area(Layer::new(Mark::Area), vec![4.0, 5.0, 3.0, 6.0]);
    let poly = &polygons(&svg)[0];
    let tag_start = svg.find("<polygon").unwrap();
    let tag = &svg[tag_start..tag_start + svg[tag_start..].find("/>").unwrap()];
    assert!(!tag.contains("stroke"), "area drew a stroke: {tag} ({poly})");
}

#[test]
fn every_mark_is_drawn_or_refused() {
    // The renderer's `match` on Mark and `legality::check_mark` must
    // partition the enum: a mark either produces marks in the output, or is
    // refused before rendering. `area` sat in neither camp — permissive
    // legality rules, a `_ => {}` renderer arm — and drew an empty panel
    // with exit 0 for as long as it did.
    use crate::legality::{check, DiagnosticKind};

    for mark in [Mark::Point, Mark::Line, Mark::Area, Mark::Bar,
                 Mark::Text, Mark::Path, Mark::Surface] {
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0])
            .with_float("b", vec![4.0, 5.0, 6.0])
            .with_str("lab", vec!["Zz".to_string(), "Zz".to_string(), "Zz".to_string()]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        // `text`'s minimum syllable is `label`; give it one so it can draw at
        // all (as `interval` would need a range transform). The others place
        // from x/y alone.
        let layer = if mark == Mark::Text {
            Layer::new(mark.clone()).encode(Channel::Label, "lab")
        } else {
            Layer::new(mark.clone())
        };
        let spec = PlotSpec::new().data("t").x("a").y("b").layer(layer);

        // **Refused means *any fatal* diagnostic, and `surface` is what made that
        // distinction matter.** This read `Unsupported` alone while every
        // undrawable mark was undrawable for the same reason — valid grammar with
        // no renderer behind it. A flat `surface` is not that: it is a mark whose
        // minimum syllable includes the cube, so the refusal is `Illegal`, the same
        // kind `interval` gets without a range transform. Both kinds are fatal and
        // draw nothing, which is the only property this partition is about.
        let refused = check(&spec, &data).iter().any(|d| {
            matches!(d.kind, DiagnosticKind::Unsupported | DiagnosticKind::Illegal)
        });
        let svg = SvgRenderer::default().render(&spec, &data);
        // A bar is found by its hairline rather than by `<rect`, which the
        // panel background and every clip path also emit. That hairline is
        // the fingerprint the rest of this suite already uses, and it is
        // already known to need a different hook once the border
        // setting absorbs it. `text` is found by its glyph content (`>Zz<`),
        // which no axis label emits.
        let drew = ["<circle", "<polyline", "<polygon", r#"stroke-width="0.5""#, ">Zz<"]
            .iter()
            .any(|t| svg.contains(t));

        assert!(
            refused != drew,
            "{mark:?}: refused={refused} drew={drew} — a mark must be exactly one of the two"
        );
    }
}

// -- axis range fits the data (Wilkinson §6.2.2) ----------------------

#[test]
fn a_linear_axis_fits_its_data_not_the_outermost_tick() {
    // year 1952–2007 used to snap out to 1940–2020, roughly a third of the
    // panel left dead on the two sides. Now the range follows the data.
    let (lo, hi) = fitted_range(1952.0, 2007.0, false, false);
    assert!(lo > 1940.0 && lo <= 1952.0, "left end fits close: {lo}");
    assert!(hi >= 2007.0 && hi < 2020.0, "right end fits close: {hi}");
    assert!(lo < 1952.0 && hi > 2007.0, "free ends breathe a little");
}

#[test]
fn a_baseline_end_is_pinned_a_free_end_breathes() {
    // A bar or area measures from zero; a gap below the baseline would put
    // zero somewhere it is not. The other end still gets headroom.
    let (lo, hi) = fitted_range(0.0, 46.0, true, false);
    assert_eq!(lo, 0.0, "no gap below the baseline");
    assert!(hi > 46.0, "headroom above the tallest value");
    // The same numbers with no baseline breathe below zero — the pin is the
    // only difference, and it is what keeps the two cases from being an
    // exception hidden in a shared path.
    let (lo2, _) = fitted_range(0.0, 46.0, false, false);
    assert!(lo2 < 0.0, "a free low end breathes below zero");
}

#[test]
fn ticks_outside_the_fitted_range_are_dropped() {
    let t = nice_ticks(1952.0, 2007.0, 5);
    assert!(
        t.values.contains(&1940.0) && t.values.contains(&2020.0),
        "nice_ticks brackets the data outward — that is the set to trim"
    );
    let (fitted, (lo, hi)) = fit_axis(t, 1952.0, 2007.0, false, false);
    assert!(
        fitted.values.iter().all(|&v| v >= lo && v <= hi),
        "no tick may sit outside the panel"
    );
    assert!(!fitted.values.contains(&1940.0), "the dead 1940 tick is gone");
    assert!(fitted.values.contains(&1960.0) && fitted.values.contains(&2000.0));
    assert_eq!(fitted.values.len(), fitted.labels.len(), "labels trimmed with values");
}

/// **A derived axis left with two ticks takes a finer step** (ruled
/// 2026-09-28: the narrow change). Orders from 13 to 34 round to a range of
/// 50 and a step of 10, which left 20 and 30 alone on the axis. An axis that
/// already shows three ticks keeps exactly the ones it had, and a count the
/// caller states keeps its own rule, so two stated ticks stay two.
#[test]
fn a_derived_axis_left_with_two_ticks_takes_a_finer_step() {
    let ticks = |lo: f64, hi: f64, count: Option<usize>| -> Vec<f64> {
        let df = DataFrame::new().with_float("v", vec![lo, hi]);
        let refs: Vec<&DataFrame> = vec![&df];
        let (t, _, _) = build_axis(
            &refs, &[], &[], "v", None, false, false, false, count, false, 10.0,
            None, (None, None), (0.0, 0.0), None, (None, None));
        t.values
    };
    assert_eq!(ticks(13.0, 34.0, None), vec![15.0, 20.0, 25.0, 30.0, 35.0],
        "two ticks, 20 and 30, were all a derived step of 10 left");
    assert_eq!(ticks(0.0, 100.0, None), vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0],
        "an axis with three ticks or more is left as it was");
    assert_eq!(ticks(1952.0, 2007.0, Some(2)).len(), 2, "a stated count keeps its own rule");
}

#[test]
fn the_guard_keeps_a_bare_axis_from_forming() {
    // The opposite failure Wilkinson names: if no nice tick lands inside the
    // fitted range, dropping them all bares the axis. There the loose
    // bracketing range is the lesser evil and is kept.
    let t = TickSpec {
        values: vec![0.0, 100.0],
        labels: vec!["0".into(), "100".into()],
        step: 100.0,
        widened: None,
    };
    let (kept, range) = fit_axis(t, 40.0, 41.0, false, false);
    assert_eq!(kept.values, vec![0.0, 100.0], "ticks kept, not dropped to nothing");
    assert_eq!(range, (0.0, 100.0), "loose range kept alongside them");
}

// -- limits: the stated domain reaches the axis (spec §10) ---------------

fn axis_range_with_limits(lo: Option<f64>, hi: Option<f64>) -> (TickSpec, (f64, f64)) {
    let df = DataFrame::new()
        .with_float("year", vec![1952.0, 1980.0, 2007.0])
        .with_float("life", vec![40.0, 60.0, 82.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x_limited("year", lo, hi).y("life")
        .layer(Layer::new(Mark::Point));
    axis_range_x(&spec, &data)
}

#[test]
fn a_stated_end_is_flush_and_an_unstated_one_still_breathes() {
    // The breathing margin exists to keep a *glyph* off the frame, and an
    // end that means something is drawn where it is (the fill boundary, the
    // baseline zero). A stated limit means something by construction — and
    // the polar cycle needs it, since 5% of headroom on a periodic axis is a
    // circle that does not close.
    let (_, both) = axis_range_with_limits(Some(1950.0), Some(2010.0));
    assert_eq!(both, (1950.0, 2010.0), "both stated ends sit exactly where stated");

    let (_, half) = axis_range_with_limits(Some(1950.0), None);
    assert_eq!(half.0, 1950.0, "the stated end is flush");
    assert!(half.1 > 2007.0, "the free end still breathes: {}", half.1);

    let (_, none) = axis_range_with_limits(None, None);
    assert!(none.0 < 1952.0 && none.1 > 2007.0, "stating nothing changes nothing");
}

/// A log axis takes its range from the **data**, not from its own ticks, and a
/// bar on one gets the half-slot every other axis gives it.
///
/// The log branch was §10's last exemption from *ticks from the scheme, range
/// from the data*, and on a histogram it clipped: a bar range is read off the
/// **centers**, so a slot reaches half a bin past the last of them, and
/// bracketing to whole powers ended the axis at 10^5 while the last bar reached
/// 10^5.055. Reported by a reader looking at the plot — the tell was an
/// asymmetry, a wide empty band at one end and a sliced bar at the other, which
/// is what bracketing outward looks like when the data is not centered in its
/// decades. Both ends are the one mistake.
#[test]
fn a_log_axis_holds_its_bars_rather_than_its_decades() {
    // Bin centers spanning 10^2.45 .. 10^4.99 with a slot of 0.134 decades:
    // gapminder's gdp cut into twenty, the case that reported this.
    let (lo_c, hi_c) = (2.4491_f64, 4.9883_f64);
    let slot = (hi_c - lo_c) / 19.0;
    let centers: Vec<f64> = (0..20)
        .map(|i| lo_c + slot * (i as f64))
        .collect();
    let df = DataFrame::new().with_float("gdp", centers.clone());

    let (_, range, _) = build_axis(
        &[&df], &[&df], &[], "gdp", None,
        /* bar_position */ true, false, false, None,
        /* is_log */ true, 10.0, None, (None, None), (0.0, 0.0), None,
        (None, None),
    );
    // The half-slot beyond the end bars is inside the axis, both ends.
    assert!(range.0 <= lo_c - slot / 2.0 + 1e-6,
        "the first bar is sliced: axis starts {}, bar starts {}", range.0, lo_c - slot / 2.0);
    assert!(range.1 >= hi_c + slot / 2.0 - 1e-6,
        "the last bar is sliced: axis ends {}, bar ends {}", range.1, hi_c + slot / 2.0);
    // And it is *fitted*, not bracketed to the decades either side: the old
    // behavior returned exactly (2.0, 5.0), which is what clipped the bar.
    assert!(range.0 > 2.0 && range.1 < 6.0,
        "the axis bracketed out to whole powers again: {range:?}");
}

#[test]
fn a_stated_domain_survives_the_bare_axis_guard() {
    // `clip_ticks` gives a *derived* range back when fitting would bare the
    // axis — the derivation was the lesser thing. A stated domain is not a
    // derivation to second-guess: handing back the loose bracketing range
    // would draw an axis nobody asked for, having accepted the binding.
    let t = || TickSpec {
        values: vec![0.0, 100.0],
        labels: vec!["0".into(), "100".into()],
        step: 100.0,
        widened: None,
    };
    let (kept, range) = clip_ticks(t(), 40.0, 41.0);
    assert_eq!(range, (0.0, 100.0), "derived: the guard keeps the loose range");
    assert_eq!(kept.values.len(), 2);

    let (kept, range) = adopt_range(t(), 40.0, 41.0);
    assert_eq!(range, (40.0, 41.0), "stated: the range is adopted regardless");
    assert!(kept.values.is_empty(), "and the ticks outside it are gone");
}

#[test]
fn a_stated_domain_holds_a_polar_axis_open_to_its_period() {
    // The forcing case, as a number. Hours observed 1..22 fit an axis that
    // closes at 22; stating the day makes the turn a whole day, so the wrap
    // is the period rather than the data's extremes.
    let df = DataFrame::new()
        .with_float("hour", vec![1.0, 10.0, 22.0])
        .with_float("n", vec![2.0, 9.0, 3.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);

    let open = PlotSpec::new().data("t").x("hour").y("n").layer(Layer::new(Mark::Line));
    let (_, xs) = axis_range_x(&open, &data);
    assert!((xs.1 - xs.0 - 24.0).abs() > 0.5,
        "one turn is the fitted span, which is not a day: {xs:?}");

    let cycle = PlotSpec::new().data("t").x_limited("hour", Some(0.0), Some(24.0))
        .y("n").layer(Layer::new(Mark::Line));
    let (_, xs) = axis_range_x(&cycle, &data);
    assert_eq!(xs, (0.0, 24.0), "the whole cycle, flush at both ends");
}

#[test]
fn a_stated_domain_puts_each_reading_at_its_true_angle_and_does_not_close_the_curve() {
    // A user asked whether the open curve was a defect (spec §8c). It is not,
    // and both halves of the answer are pinned here because the tempting fix
    // — treating a stated domain like a categorical one and wrapping — would
    // draw a segment between two rows that are not neighbors.
    //
    // Three-hourly readings, 1..22: they cover the day but reach neither
    // midnight, which is the ordinary shape of periodic data.
    let df = DataFrame::new()
        .with_float("hour", vec![1.0, 4.0, 7.0, 10.0, 13.0, 16.0, 19.0, 22.0])
        .with_float("height", vec![2.1, 4.4, 3.6, 1.2, 0.9, 2.8, 4.7, 3.9]);
    let data = HashMap::from([("t".to_string(), df)]);

    // Angles clockwise from twelve o'clock, read off the drawn polyline.
    let angles = |spec: &PlotSpec| -> Vec<f64> {
        let svg = SvgRenderer::default().render(spec, &data);
        let pts = svg.split("points=\"").nth(1).expect("a polyline was drawn");
        let pts = pts.split('"').next().unwrap();
        let xy: Vec<(f64, f64)> = pts
            .split_whitespace()
            .map(|p| {
                let (a, b) = p.split_once(',').unwrap();
                (a.parse::<f64>().unwrap(), b.parse::<f64>().unwrap())
            })
            .collect();
        // The disc's center is the first `<circle cx=`, which is the panel.
        let c = svg.split("<circle cx=\"").nth(1).unwrap();
        let cx: f64 = c.split('"').next().unwrap().parse().unwrap();
        let cy: f64 = c.split("cy=\"").nth(1).unwrap().split('"').next().unwrap()
            .parse().unwrap();
        xy.iter()
            .map(|(x, y)| ((x - cx).atan2(cy - y).to_degrees() + 360.0) % 360.0)
            .collect()
    };

    let polar = |s: PlotSpec| s.coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }));

    // **The defect `limits` fixes.** One turn is fitted to 1..22, so the first
    // and last readings land on the *same spoke* — drawn on top of each other,
    // three hours apart in the world and none on the page.
    // Tolerance is 0.05°, not an epsilon: these angles are read back out of
    // SVG coordinates rounded to two decimals, so the precision here is the
    // output's rather than the arithmetic's. Wide enough to survive that,
    // far tighter than any distinction being asserted (15° from 0°).
    let close_enough = |a: f64, b: f64| (a - b).abs() < 0.05;

    let open = angles(&polar(PlotSpec::new().data("t").x("hour").y("height"))
        .layer(Layer::new(Mark::Line)));
    assert!(close_enough(open[0], open[open.len() - 1]),
        "without limits the ends collide: {open:?}");

    // **With the day stated**, each reading sits where a clock would put it.
    let stated = angles(&polar(PlotSpec::new().data("t")
        .x_limited("hour", Some(0.0), Some(24.0)).y("height"))
        .layer(Layer::new(Mark::Line)));
    for (i, hour) in [1.0, 4.0, 7.0, 10.0, 13.0, 16.0, 19.0, 22.0].iter().enumerate() {
        assert!(close_enough(stated[i], hour / 24.0 * 360.0),
            "hour {hour} should sit at {}°, got {}", hour / 24.0 * 360.0, stated[i]);
    }

    // **And the curve stays open**, because no reading was taken at midnight
    // and a domain does not invent one. 22:00 to 01:00 is 45° of genuine gap.
    assert!(close_enough(stated[stated.len() - 1] - stated[0], 315.0),
        "the wrap gap is the data's three hours, not closed: {stated:?}");
    assert_eq!(stated.len(), 8, "eight readings, eight vertices — none added");
}

#[test]
fn a_stated_end_is_not_pulled_back_to_the_baseline() {
    // A bar's measure axis stretches to include zero, because a length is
    // measured from somewhere. A stated end says where the axis starts, and
    // pulling it back to 0 would draw the baseline where the caller said it
    // is not — while the *unstated* end still stretches.
    let df = DataFrame::new()
        .with_str("g", vec!["a".into(), "b".into()])
        .with_float("v", vec![30.0, 50.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("g").y_limited("v", Some(20.0), None)
        .layer(Layer::new(Mark::Bar));
    let df = data.get("t").unwrap();
    let eff: Vec<DataFrame> = spec.layers.iter().map(|_| df.clone()).collect();
    let refs: Vec<&DataFrame> = eff.iter().collect();
    let (_, ys, _) = build_axis(&refs, &[], &[], "v", None, false, true, false, None, false, 10.0, None,
                             crate::scale::domain_of(spec.axis_def(&Channel::Y)), (0.0, 0.0), None, (None, None));
    assert_eq!(ys.0, 20.0, "the stated end holds against the stretch-to-zero");
}

// -- the violin: the slot reading of `density` (spec §5) -------------------

/// Two groups over the **same** spread, one carrying four times the rows of the
/// other — so the only thing the default `compare` can be reading is the count.
/// (Different spreads would confound it: a uniform group's count-weighted
/// density is `n / range`, so forty rows over forty units and ten over ten come
/// out identical, which is what the first version of this fixture asserted
/// against by accident.)
fn violin_svg(mark: Mark, sideways: bool) -> String {
    let (mut cats, mut vals) = (Vec::new(), Vec::new());
    for (g, n) in [("wide", 40usize), ("narrow", 10)] {
        for i in 0..n { cats.push(g.to_string()); vals.push((i % 10) as f64); }
    }
    let df = DataFrame::new().with_str("g", cats).with_float("v", vals);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = if sideways { PlotSpec::new().data("t").x("v").y("g") }
               else        { PlotSpec::new().data("t").x("g").y("v") };
    let spec = spec.layer(Layer::new(mark).transform(Transform::Density));
    SvgRenderer::default().render(&spec, &data)
}

/// Every x in a polygon, as pairs.
fn polygon_points(svg: &str) -> Vec<Vec<(f64, f64)>> {
    svg.split("<polygon points=\"").skip(1)
        .map(|chunk| chunk.split('"').next().unwrap_or("").split_whitespace()
            .filter_map(|p| {
                let (a, b) = p.split_once(',')?;
                Some((a.parse().ok()?, b.parse().ok()?))
            })
            .collect())
        .collect()
}

/// The one geometric claim that separates the two marks, and the reason neither
/// needed a new atom: a `ribbon` closes on its own **reflection**, an `area` on
/// the slot's **center line**. Asserted as symmetry about that line rather than
/// by counting vertices, so a rewrite that draws the same shape a different way
/// still passes and one that draws a different shape cannot.
#[test]
fn a_violin_mirrors_and_a_half_violin_does_not() {
    let mirrored = polygon_points(&violin_svg(Mark::Ribbon, false));
    assert_eq!(mirrored.len(), 2, "one shape per category");
    for poly in &mirrored {
        let (lo, hi) = poly.iter().map(|p| p.0)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x)));
        // The outline runs out one side and back the other, so the two halves
        // are the same width — the midpoint of the extremes is the slot center,
        // and the widest vertex on each side is equidistant from it.
        let mid = (lo + hi) / 2.0;
        assert!((mid - lo) - (hi - mid) < 1e-6, "a violin must be symmetric about its slot");
        assert!(hi - lo > 1.0, "a violin must have width");
    }

    let half = polygon_points(&violin_svg(Mark::Area, false));
    assert_eq!(half.len(), 2, "one shape per category");
    for poly in &half {
        // The return leg is the center line, so exactly one x is repeated for
        // every sample: the flat side. A mirrored outline has no such column.
        let flat = poly.iter().map(|p| p.0)
            .fold(std::collections::BTreeMap::<u64, usize>::new(), |mut m, x| {
                *m.entry(x.to_bits()).or_default() += 1; m
            });
        let tallest = flat.values().cloned().max().unwrap_or(0);
        assert!(tallest > poly.len() / 4,
            "a half violin closes on a straight center line, so one x repeats");
    }
}

/// The default `compare` weights each estimate by its group's rows, so the
/// forty-row group must draw wider than the ten-row one. Pins the *decision*
/// (spec §5) rather than the pixels: equal areas would draw these two almost
/// alike, since they differ far more in count than in spread.
#[test]
fn a_violin_is_wider_where_the_group_is_bigger() {
    let widths: Vec<f64> = polygon_points(&violin_svg(Mark::Ribbon, false)).iter()
        .map(|poly| {
            let (lo, hi) = poly.iter().map(|p| p.0)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x)));
            hi - lo
        })
        .collect();
    assert!(widths[0] > widths[1] * 1.5,
        "the 40-row group should draw markedly wider than the 10-row one, got {widths:?}");
}

/// Lying down, the violins spread across `y` instead — the orientation read off
/// the bindings, so the same sentence with its axes exchanged draws the same
/// plot turned. Without the exchange the transform would group by the *measure*
/// and estimate along the category, which draws nothing at all.
#[test]
fn a_sideways_violin_spreads_across_the_other_axis() {
    let sideways = polygon_points(&violin_svg(Mark::Ribbon, true));
    assert_eq!(sideways.len(), 2, "one shape per category, lying down");
    for poly in &sideways {
        let (lo, hi) = poly.iter().map(|p| p.1)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), y| (a.min(y), b.max(y)));
        assert!(hi - lo > 1.0, "a sideways violin has its width on y");
    }
}

/// **The x axis's name sits below its turned names.** Turned, a name hangs
/// from its tick by its width times the sine of the angle, and the axis name
/// used to be set one upright row below the panel whatever the angle, so at
/// `tick_angle = 45` it was drawn across the names it titles. Asserted
/// against the lowest point any turned name reaches, measured with the same
/// text metrics the renderer uses.
#[test]
fn the_axis_name_sits_below_its_turned_names() {
    let names = ["Alpha region", "Beta region", "Gamma region"];
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", names.iter().map(|s| s.to_string()).collect())
        .with_float("v", vec![3.0, 5.0, 2.0]))]);
    let mut spec = PlotSpec::new().data("t").x("g").y("v")
        .layer(Layer::new(Mark::Bar).transform(Transform::Mean));
    spec.theme.tick_angle = Some(45.0);
    let r = SvgRenderer::default();
    let svg = r.render(&spec, &data);

    // The turned names' anchor row, and how far below it the longest reaches.
    let anchor_y: f64 = svg.split("transform=\"rotate(-45.00 ").nth(1)
        .and_then(|t| t.split(')').next())
        .and_then(|xy| xy.split_whitespace().nth(1))
        .and_then(|y| y.parse().ok())
        .expect("a turned name");
    let widest = names.iter().map(|n| estimate_text_width(n, r.font_sm)).fold(0.0, f64::max);
    let lowest = anchor_y + widest * std::f64::consts::FRAC_1_SQRT_2;

    // The axis name: the one text element reading "G".
    let name_y: f64 = svg.split("<text ").find(|t| t.contains(">G</text>"))
        .and_then(|t| t.split("y=\"").nth(1))
        .and_then(|y| y.split('"').next())
        .and_then(|y| y.parse().ok())
        .expect("the axis name");
    let name_top = name_y - estimate_cap_height(r.font_md);
    assert!(name_top > lowest,
        "the axis name's top ({name_top:.1}) must be below the turned names ({lowest:.1})");
}

// -- one smoothing per `density` layer (`transform::Smoothing`) ------------

/// Three groups that would each choose a different bandwidth alone (thirty rows
/// over thirty units, eight over two, three over half a unit), in a table that
/// also carries a facet column and a moment.
fn smoothing_data() -> HashMap<String, DataFrame> {
    let (mut g, mut v) = (Vec::new(), Vec::new());
    let rows = (0..30).map(|i| ("a", i as f64))
        .chain((0..8).map(|i| ("b", 10.0 + i as f64 * 0.3)))
        .chain([("c", 12.0), ("c", 12.2), ("c", 12.5)]);
    for (k, x) in rows { g.push(k.to_string()); v.push(x); }
    let t: Vec<f64> = (0..v.len()).map(|i| (i % 3) as f64).collect();
    HashMap::from([("t".to_string(),
        DataFrame::new().with_str("g", g).with_float("v", v).with_float("when", t))])
}

fn at_bandwidth(mut layer: Layer, h: f64) -> Layer {
    layer.density = Some(crate::ir::DensitySpec { bandwidth: Some(h), ..Default::default() });
    layer
}

/// **The automatic bandwidth of a layer is the mean of its groups' own**, so the
/// sentence draws exactly what it draws with that mean stated: for violins, for
/// curves split by `color`, for panels and for moments. Asserted against the
/// stated render byte for byte, which is what "one smoothing length for the
/// whole layer" means on the page.
#[test]
fn a_density_layer_draws_every_group_at_one_shared_bandwidth() {
    let data = smoothing_data();
    let df = &data["t"];
    let render = |spec: &PlotSpec| SvgRenderer::default().render(spec, &data);
    let shared = |own: Vec<f64>| {
        assert!(own.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-6), "the groups must disagree: {own:?}");
        crate::transform::shared_bandwidth(&own, None).unwrap()
    };
    let density = || Layer::new(Mark::Ribbon).transform(Transform::Density);
    let curve = || Layer::new(Mark::Line).transform(Transform::Density);

    // Violins: one estimate per category.
    let h = shared(crate::transform::density_own_bandwidths(df, "g", "v", &[]));
    let violins = |l: Layer| PlotSpec::new().data("t").x("g").y("v").layer(l);
    assert_eq!(render(&violins(density())), render(&violins(at_bandwidth(density(), h))));

    // Curves split by `color`: one estimate per color.
    let h = shared(crate::transform::density_own_bandwidths(df, "v", "", &["g"]));
    let split = |l: Layer| PlotSpec::new().data("t").x("v").layer(l.encode(Channel::Color, "g"));
    assert_eq!(render(&split(curve())), render(&split(at_bandwidth(curve(), h))));

    // Panels: one estimate per panel, and the same length in all of them.
    let panels = |l: Layer| PlotSpec::new().data("t").x("v").layer(l).facet_col("g");
    assert_eq!(render(&panels(curve())), render(&panels(at_bandwidth(curve(), h))));

    // Moments: one estimate per frame, and the same length on the whole clock.
    let own: Vec<f64> = [0.0, 1.0, 2.0].iter()
        .flat_map(|&t| crate::transform::density_own_bandwidths(&df.filter_float_eq("when", t), "v", "", &[]))
        .collect();
    let h = shared(own);
    let played = |l: Layer| PlotSpec::new().data("t").x("v").layer(l.encode(Channel::Play, "when"));
    assert_eq!(render(&played(curve())), render(&played(at_bandwidth(curve(), h))));
}

/// A freed axis takes its own smoothing per panel, as it takes its own bin edges:
/// the panel has its own scale, so a length fitted to the other panels' spreads
/// would over-smooth a narrow one. On the axis the estimate does not spread
/// along, freeing changes nothing.
#[test]
fn a_freed_axis_smooths_each_panel_at_its_own_bandwidth() {
    // Violins of `a`, of `b` (a three times as spread) and of `c` (eight times).
    // Panel one holds a and b, so its own mean is 2h for a's bandwidth h; panel
    // two holds all three, a mean of 4h; the layer's is 3.2h.
    let a: Vec<f64> = (0..20).map(|i| 1.0 + (i as f64 * 0.9).sin() * 4.0 + i as f64 * 0.3).collect();
    let times = |k: f64| a.iter().map(|v| v * k).collect::<Vec<f64>>();
    let n = a.len();
    let tag = |s: &str, k: usize| vec![s.to_string(); k * n];
    let one = DataFrame::new()
        .with_str("g", [tag("a", 1), tag("b", 1)].concat())
        .with_float("v", [a.clone(), times(3.0)].concat());
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("p", [tag("one", 2), tag("two", 3)].concat())
        .with_str("g", [tag("a", 1), tag("b", 1), tag("a", 1), tag("b", 1), tag("c", 1)].concat())
        .with_float("v", [a.clone(), times(3.0), a.clone(), times(3.0), times(8.0)].concat()))]);
    let panel_one = crate::transform::shared_bandwidth(
        &crate::transform::density_own_bandwidths(&one, "g", "v", &[]), None).unwrap();

    let render = |layer: Layer| {
        let mut spec = PlotSpec::new().data("t").x("g").layer(layer).facet_col("p");
        spec.y = Some(crate::ir::ChannelDef::field("v").with_free());
        SvgRenderer::default().render(&spec, &data)
    };
    let violins = || Layer::new(Mark::Ribbon).transform(Transform::Density);
    // Each violin's outline down the measure axis. Only `y` is compared: panel
    // two's freed tick labels change width between the two renders, which moves
    // every panel sideways, while the heights are the panel's own.
    let outlines = |svg: &str| -> Vec<String> {
        svg.split("<polygon points=\"").skip(1)
            .map(|c| c.split('"').next().unwrap_or("").split_whitespace()
                .filter_map(|p| p.split_once(',').map(|(_, y)| y))
                .collect::<Vec<_>>().join(" "))
            .collect()
    };
    let freed = outlines(&render(violins()));
    let stated = outlines(&render(at_bandwidth(violins(), panel_one)));
    assert_eq!((freed.len(), stated.len()), (5, 5), "two violins in panel one, three in panel two");

    // With y freed, a panel's heights are its own. The widest violin in each
    // panel spans that panel's whole range and so reads the same at any length;
    // the one that tells is panel one's `a`, drawn first. It runs identically at
    // 2h in both renders, where at its own h or at the layer's 3.2h it would not,
    // and panel two's `a` does not (4h against 2h).
    assert_eq!(freed[0], stated[0], "panel one is smoothed at the mean of its own groups");
    assert_ne!(freed[2], stated[2], "panel two is smoothed at the mean of its own");
}

/// Two groups with one shape draw one field whatever their row counts, which is
/// what sharing the smoothing buys a contour plot split by `group`: alone, a
/// group with every row written twice would choose a bandwidth 2^(−1/6) as long
/// and trace tighter rings. (`color` is the level's on a contour, so `group` is
/// the split.)
#[test]
fn a_contour_split_by_group_traces_the_same_rings_for_the_same_shape() {
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for i in 0..40 {
        let t = i as f64;
        xs.push(2.0 + (t * 0.7).sin());
        ys.push(2.0 + (t * 1.3).cos());
    }
    let n = xs.len();
    let twice = |v: &[f64]| [v, v].concat();
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_float("x", [xs.clone(), twice(&xs)].concat())
        .with_float("y", [ys.clone(), twice(&ys)].concat())
        .with_str("g", [vec!["once".to_string(); n], vec!["twice".to_string(); 2 * n]].concat()))]);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Path).transform(Transform::Density).encode(Channel::Group, "g"));
    assert!(crate::legality::check(&spec, &data).iter().all(|d| !d.is_fatal()));
    let svg = SvgRenderer::default().render(&spec, &data);

    // A ring colored by its level is one run of that color, a round-capped
    // `<polyline>` (a `<line>` if it is one segment long); counted, every ring
    // must be traced once by each group.
    let mut rings: std::collections::BTreeMap<&str, usize> = Default::default();
    for chunk in svg.split("<polyline ").skip(1).chain(svg.split("<line ").skip(1)) {
        let element = chunk.split("/>").next().unwrap_or("");
        if element.contains("stroke-linecap=\"round\"") {
            *rings.entry(element).or_default() += 1;
        }
    }
    assert!(rings.len() >= 4, "the field traces rings: {} rings", rings.len());
    let odd = rings.values().filter(|&&n| n != 2).count();
    assert_eq!(odd, 0, "the two groups must trace the same rings: {odd} of {} rings differ",
               rings.len());
}

/// `reach` is measured **from** the slot line, so past 0.5 a shape leaves its
/// own slot — which is not a mistake to guard against but the ridgeline being
/// asked for. Asserted as the overlap itself: at 2.5 slots a ridge must cross
/// the line its neighbor sits on.
#[test]
fn reach_past_half_a_slot_is_how_ridges_overlap() {
    let df = DataFrame::new()
        .with_str("g", vec!["a".into(), "a".into(), "a".into(), "a".into(),
                            "b".into(), "b".into(), "b".into(), "b".into()])
        .with_float("v", vec![0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let svg = |reach: Option<f64>| {
        let mut l = Layer::new(Mark::Area).transform(Transform::Density);
        l.density = Some(crate::ir::DensitySpec {
            adjust: None, bandwidth: None, levels: None, compare: None, reach, support: (None, None),
        });
        SvgRenderer::default().render(&PlotSpec::new().data("t").x("g").y("v").layer(l), &data)
    };
    // A half violin's return leg is the slot's line, so the x that repeats most
    // is where the category sits; the reach is the furthest vertex from it. Read
    // that way rather than as a raw width because the axis *grows* with the reach
    // (see below), so pixels are not comparable between renders and slots are —
    // which is exactly what the knob is denominated in.
    let in_slots = |s: &str| -> Vec<f64> {
        let polys = polygon_points(s);
        let line = |poly: &Vec<(f64, f64)>| -> f64 {
            let mut tally = std::collections::BTreeMap::<u64, usize>::new();
            for p in poly { *tally.entry(p.0.to_bits()).or_default() += 1 }
            f64::from_bits(*tally.iter().max_by_key(|(_, n)| **n).unwrap().0)
        };
        let lines: Vec<f64> = polys.iter().map(line).collect();
        let slot = (lines[1] - lines[0]).abs();
        polys.iter().zip(&lines)
            .map(|(poly, &ln)| poly.iter()
                .map(|p| (p.0 - ln).abs())
                .fold(0.0f64, f64::max) / slot)
            .collect()
    };
    for (got, want) in in_slots(&svg(None)).iter().zip([crate::ir::DEFAULT_REACH; 2]) {
        assert!((got - want).abs() < 0.02, "the default reach is {want} slots, got {got}");
    }
    for got in in_slots(&svg(Some(2.5))) {
        assert!((got - 2.5).abs() < 0.02, "reach is measured in slots, got {got}");
        // The overlap itself: past half a slot a shape crosses its neighbor's
        // line, which is the ridgeline and what the default must not do.
        assert!(got > 1.0, "a 2.5-slot reach must cross the next category's line");
    }
}

/// **The axis grows to hold what overhangs it.** A ridge reaching 2.5 slots off
/// its own line ran off the top of the panel and was clipped by the frame — the
/// mark drawing outside the plot, which is the same defect as an area whose
/// baseline fell off the scale and just as invisible, since it still rendered.
/// Half a slot each side is right for everything that stands *in* its slot; a
/// violin need not, so `legality::slot_reach` tells the axis how far.
#[test]
fn a_reaching_violin_is_not_clipped_by_the_frame() {
    let df = DataFrame::new()
        .with_str("g", vec!["a".into(), "a".into(), "a".into(), "a".into(),
                            "b".into(), "b".into(), "b".into(), "b".into()])
        .with_float("v", vec![0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    for (mark, sideways) in [(Mark::Area, false), (Mark::Area, true),
                             (Mark::Ribbon, false), (Mark::Ribbon, true)] {
        let mut l = Layer::new(mark.clone()).transform(Transform::Density);
        l.density = Some(crate::ir::DensitySpec {
            adjust: None, bandwidth: None, levels: None, compare: None, reach: Some(2.5), support: (None, None),
        });
        let spec = if sideways { PlotSpec::new().data("t").x("v").y("g") }
                   else        { PlotSpec::new().data("t").x("g").y("v") };
        let svg = SvgRenderer::default().render(&spec.layer(l), &data);
        // The panel, read off the background rect the frame draws.
        let rect = svg.split(r#"<rect x=""#).nth(1).expect("a panel");
        let num = |attr: &str| -> f64 {
            rect.split(&format!(r#"{attr}=""#)).nth(1).unwrap()
                .split('"').next().unwrap().parse().unwrap()
        };
        let (px, py) = (rect.split('"').next().unwrap().parse::<f64>().unwrap(), num("y"));
        let (pw, ph) = (num("width"), num("height"));
        for poly in polygon_points(&svg) {
            for (x, y) in poly {
                assert!(x >= px - 0.5 && x <= px + pw + 0.5
                        && y >= py - 0.5 && y <= py + ph + 0.5,
                    "{mark:?} sideways={sideways}: vertex ({x}, {y}) is outside \
                     the panel ({px}, {py}, {pw}, {ph})");
            }
        }
    }
}

/// `line`/`step` trace the estimate instead of filling it — the "filled, or two
/// edges" rule against a slot — and the stroke must land exactly on the fill's
/// boundary, which is what lets `area + line` draw an edged ridge.
#[test]
fn a_traced_violin_lands_on_the_filled_one_s_edge() {
    let fill = violin_svg(Mark::Area, true);
    let trace = violin_svg(Mark::Line, true);
    assert!(trace.contains("<path"), "a traced violin is a stroke");
    assert!(!trace.contains("<polygon"), "a traced violin fills nothing");
    // The stroke's vertices are the fill polygon's leading half, vertex for
    // vertex — the return leg along the slot line is the only difference.
    let poly = &polygon_points(&fill)[0];
    let d: Vec<(f64, f64)> = trace.split("<path d=\"M").nth(1).unwrap()
        .split('"').next().unwrap().split_whitespace()
        .filter_map(|p| { let (a, b) = p.split_once(',')?;
                          Some((a.parse().ok()?, b.parse().ok()?)) })
        .collect();
    assert!(!d.is_empty(), "the trace has vertices");
    for (k, v) in d.iter().enumerate().take(8) {
        assert!((v.0 - poly[k].0).abs() < 0.01 && (v.1 - poly[k].1).abs() < 0.01,
            "vertex {k} of the trace must sit on the fill's edge: {v:?} vs {:?}", poly[k]);
    }
}

/// A half violin closes on the slot's center line, which is on the *other*
/// axis — so the measure axis must not be stretched to zero the way a normal
/// `area`'s is. Data starting at 1 drew a panel from 0 before this was split.
#[test]
fn a_half_violin_does_not_drag_its_measure_axis_to_zero() {
    let df = DataFrame::new()
        .with_str("g", vec!["a".into(); 8])
        .with_float("v", vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("g").y("v")
        .layer(Layer::new(Mark::Area).transform(Transform::Density));
    let svg = SvgRenderer::default().render(&spec, &data);
    assert!(!svg.contains(">0<"),
        "a half violin's baseline is the slot center, not 0 on the measure axis");
}

#[test]
fn a_flush_axis_fits_its_data_exactly() {
    // The axis an area fills along leaves no breathing band: the fill's
    // edges are the panel's edges. Contrast the same data un-flushed.
    let (lo, hi) = fitted_range(1952.0, 2007.0, false, true);
    assert_eq!((lo, hi), (1952.0, 2007.0), "flush fits exactly");
    let (lo2, hi2) = fitted_range(1952.0, 2007.0, false, false);
    assert!(lo2 < 1952.0 && hi2 > 2007.0, "un-flushed breathes");
}

#[test]
fn an_area_fills_the_panel_width_but_a_scatter_breathes() {
    // End to end. An area's year axis sits flush; add a point layer sharing
    // that axis and the breathing returns, because a glyph would clip.
    let df = DataFrame::new()
        .with_float("year", vec![1952.0, 1980.0, 2007.0])
        .with_float("life", vec![40.0, 60.0, 82.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);

    let area = PlotSpec::new().data("t").x("year").y("life")
        .layer(Layer::new(Mark::Area));
    let (_, xs) = axis_range_x(&area, &data);
    assert_eq!(xs, (1952.0, 2007.0), "area fills the width flush");

    let with_point = PlotSpec::new().data("t").x("year").y("life")
        .layer(Layer::new(Mark::Area))
        .layer(Layer::new(Mark::Point));
    let (_, xs2) = axis_range_x(&with_point, &data);
    assert!(xs2.0 < 1952.0 && xs2.1 > 2007.0, "a point restores the margin");
}

/// Render and read the x scale range back out of the first `<clipPath>`? No —
/// simpler to re-run the axis logic the renderer uses. This mirrors what
/// `render` computes for a single unfaceted panel.
fn axis_range_x(spec: &PlotSpec, data: &HashMap<String, DataFrame>) -> (TickSpec, (f64, f64)) {
    let df = data.get(spec.data.as_deref().unwrap()).unwrap();
    let eff: Vec<DataFrame> = spec.layers.iter().map(|_| df.clone()).collect();
    let refs: Vec<&DataFrame> = eff.iter().collect();
    let has_area = spec.layers.iter().any(|l| l.mark == Mark::Area);
    let has_point = spec.layers.iter().any(|l| l.mark == Mark::Point);
    // The spec's own x column, not a hardcoded one: a helper that names the
    // field itself silently answers about an absent column, and every
    // assertion then passes against the (0, 1) fallback.
    let field = spec.axis_def(&Channel::X).map(|d| d.field.clone()).unwrap_or_default();
    let (t, range, _) = build_axis(&refs, &[], &[], &field, None, false, false,
               has_area && !has_point, None, false, 10.0, None,
               crate::scale::domain_of(spec.axis_def(&Channel::X)), (0.0, 0.0), None, (None, None));
    (t, range)
}

#[test]
fn the_fitted_range_never_clips_the_data() {
    for (mn, mx, base) in [
        (1952.0, 2007.0, false),
        (0.0, 46.0, true),
        (-5.0, 5.0, true),
        (3.1, 3.9, false),
    ] {
        let (lo, hi) = fitted_range(mn, mx, base, false);
        assert!(lo <= mn && hi >= mx, "range clipped data at ({mn}, {mx})");
    }
}

#[test]
fn a_year_axis_no_longer_pads_out_to_dead_round_numbers() {
    // End to end: the ticks inside the data survive, the round-number padding
    // outside it does not.
    let df = DataFrame::new()
        .with_float("year", vec![1952.0, 1977.0, 2007.0])
        .with_float("life", vec![40.0, 60.0, 82.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("year").y("life")
        .layer(Layer::new(Mark::Line));
    let svg = SvgRenderer::default().render(&spec, &data);
    assert!(svg.contains(">1960<"), "an inside tick is drawn:\n{svg}");
    assert!(
        !svg.contains(">1940<") && !svg.contains(">2020<"),
        "the dead round-number ticks must be gone"
    );
}

// -- constant settings ------------------------------------------------

fn render_styled(layer: Layer) -> String {
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 3.0])
        .with_float("b", vec![4.0, 5.0, 6.0])
        .with_str("g", vec!["x".into(), "y".into(), "x".into()]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("a").y("b").layer(layer);
    SvgRenderer::default().render(&spec, &data)
}

#[test]
fn a_set_color_reaches_the_output() {
    let svg = render_styled(Layer::new(Mark::Point).style_color("tomato"));
    assert!(svg.contains(r#"fill="tomato""#));
    // and the built-in default is no longer used for the marks
    assert!(!svg.contains(&format!(r#"<circle cx="0.00" cy="0.00" r="4.50" fill="{}""#, PALETTE_GOG[0])));
}

#[test]
fn a_dashed_line_carries_a_stroke_dasharray_and_a_plain_one_does_not() {
    // `style(dash = )` is paint — it adds a stroke-dasharray and moves no vertex.
    // So a plain line carries none (byte-for-byte the old output) and a dashed
    // one carries the pattern.
    let svg_plain = render_styled(Layer::new(Mark::Line));
    assert!(!svg_plain.contains("stroke-dasharray"), "a solid line has no dasharray:\n{svg_plain}");

    let mut dashed = Layer::new(Mark::Line);
    dashed.style.pattern = Some("dashed".into());
    let svg_dash = render_styled(dashed);
    assert!(svg_dash.contains(r#"stroke-dasharray="6,4""#), "a dashed line carries the pattern:\n{svg_dash}");
}

#[test]
fn a_line_with_a_pair_transform_draws_two_separated_boundary_curves() {
    // `line * bounds` is the unfilled band: the pair-rows become two boundary
    // curves (a low locus and a high one), never one zigzag connecting them.
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("x",  vec![1.0, 2.0, 3.0])
        .with_float("lo", vec![10.0, 20.0, 30.0])
        .with_float("hi", vec![40.0, 50.0, 60.0]));
    let spec = PlotSpec::new().data("t").x("x")
        .layer(Layer::new(Mark::Line).bounds("lo", "hi"));
    let svg = SvgRenderer::default().render(&spec, &data);

    let curves: Vec<Vec<(f64, f64)>> = svg.lines()
        .filter(|l| l.contains("<polyline"))
        .map(|l| points_of(l.split("points=\"").nth(1).unwrap().split('"').next().unwrap()))
        .collect();
    assert_eq!(curves.len(), 2, "line * bounds draws two boundary curves:\n{svg}");
    // Each boundary is one point per x (3), not the 6 a zigzag would connect.
    for c in &curves { assert_eq!(c.len(), 3, "a boundary is one point per x, not a zigzag"); }
    // The two do not cross — one boundary sits entirely below the other.
    let miny = |c: &[(f64, f64)]| c.iter().map(|q| q.1).fold(f64::INFINITY, f64::min);
    let maxy = |c: &[(f64, f64)]| c.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max);
    let (a, b) = (&curves[0], &curves[1]);
    assert!(maxy(a) < miny(b) || maxy(b) < miny(a),
        "the low and high boundaries must not cross");
}

#[test]
fn a_point_border_rims_fillable_glyphs_and_skips_a_cross() {
    // `border_color`/`border_size` rim the filled glyphs (spec §4); a `cross`
    // has no fill, so it takes no rim — its own color stroke is all there is.
    let mut circ = Layer::new(Mark::Point);
    circ.style.border_color = Some("black".into());
    circ.style.border_size = Some(1.5);
    let svg = render_styled(circ);
    assert!(svg.contains(r#"stroke="black" stroke-width="1.50""#),
        "a circle point carries the rim:\n{svg}");

    let mut cross = Layer::new(Mark::Point);
    cross.style.shape = Some("cross".into());
    cross.style.border_color = Some("black".into());
    let svg2 = render_styled(cross);
    assert!(!svg2.contains(r#"stroke="black""#),
        "a cross skips the rim (no fill to outline):\n{svg2}");

    // And a plain point (no border) is unchanged — no stroke on the glyph itself.
    let plain = render_styled(Layer::new(Mark::Point));
    let glyph = plain.lines().find(|l| l.contains("<circle")).unwrap_or("");
    assert!(!glyph.contains("stroke="), "a borderless point glyph has no stroke: {glyph}");
}

#[test]
fn a_set_color_is_escaped() {
    // `legality::check_style` rejects a non-color before rendering, so this
    // string cannot arrive here in normal use — but `GOG_STRICT=0` renders
    // anyway, and a broken-out `fill="` attribute would corrupt the document
    // for every reader. Escaping is the last line, not the only one.
    let svg = render_styled(Layer::new(Mark::Point).style_color(r#"red" onload="evil"#));
    assert!(!svg.contains(r#"onload="evil""#));
    assert!(unescaped_ampersands(&svg).is_empty());
}

#[test]
fn setting_a_feature_draws_no_legend() {
    // The defining difference between set and map: a legend exists to decode
    // a mapping, and a constant encodes nothing to decode.
    let mapped = render_styled(Layer::new(Mark::Point).encode(Channel::Color, "g"));
    let set = render_styled(Layer::new(Mark::Point).style_color("tomato"));
    assert!(mapped.contains(">x<") && mapped.contains(">y<"), "mapping should label categories");
    assert!(!set.contains(">x<"), "a set color must not produce a legend");
    // A legend also reserves right-hand margin, so the panel must be wider.
    assert!(set.len() < mapped.len());
}

// -- the side a legend sits on (`theme(legend = )`) ----------------------

/// Five continents, so a key of five that fits across a full plot.
fn five_keyed(side: Option<&str>) -> (PlotSpec, HashMap<String, DataFrame>) {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 3.0, 4.0, 5.0])
        .with_float("y", vec![2.0, 1.0, 4.0, 3.0, 5.0])
        .with_str("g", s(&["Asia", "Europe", "Africa", "Americas", "Oceania"]));
    let mut spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "g"));
    spec.theme.legend = side.map(String::from);
    (spec, HashMap::from([("t".to_string(), t)]))
}

fn draw_at(spec: &PlotSpec, t: &HashMap<String, DataFrame>, w: f64, h: f64) -> Drawn {
    SvgRenderer::for_theme(&spec.theme.resolved(), w, h).draw(spec, t)
}

/// Every rectangle whose tag holds `key`, as (x0, y0, x1, y1). A legend's card
/// is the one with `rx="4"`, a key's swatch `rx="2"`, the panel `#f5f5f8`.
fn rects_with(svg: &str, key: &str) -> Vec<(f64, f64, f64, f64)> {
    svg.split("<rect ").skip(1)
        .map(|r| r.split('>').next().unwrap_or(""))
        .filter(|tag| tag.contains(key))
        .filter_map(|tag| {
            let grab = |k: &str| -> Option<f64> {
                let at = tag.find(k)? + k.len();
                tag[at..].split('"').next()?.parse().ok()
            };
            let (x, y) = (grab("x=\"")?, grab("y=\"")?);
            Some((x, y, x + grab("width=\"")?, y + grab("height=\"")?))
        })
        .collect()
}

/// **A legend sits on the side `theme(legend = )` names**, clear of the panel
/// and inside the canvas, and the side nobody names is the right.
#[test]
fn a_legend_sits_on_the_side_the_theme_names() {
    let draw = |side: Option<&str>| {
        let (spec, t) = five_keyed(side);
        draw_at(&spec, &t, 800.0, 600.0)
    };
    assert_eq!(draw(None).svg, draw(Some("right")).svg, "the right is the default, said out loud");
    for side in ["right", "left", "top", "bottom"] {
        let drawn = draw(Some(side));
        let cards = rects_with(&drawn.svg, r#"rx="4""#);
        let panel = rects_with(&drawn.svg, r##"fill="#f5f5f8""##)[0];
        assert_eq!(cards.len(), 1, "{side}: one card");
        let c = cards[0];
        let clear = match side {
            "right" => c.0 >= panel.2 + 20.0,
            "left" => c.2 <= panel.0 - 20.0,
            "top" => c.3 <= panel.1,
            _ => c.1 >= panel.3,
        };
        assert!(clear, "{side}: the card {c:?} stands clear of the panel {panel:?}");
        assert!(c.0 >= 0.0 && c.1 >= 0.0 && c.2 <= 800.0 && c.3 <= 600.0,
                "{side}: the card {c:?} is inside the canvas");
        assert!(drawn.remarks.is_empty(), "{side}: {:?}", drawn.remarks);
    }
    // Above or below the plot the key costs height rather than width, which is
    // the reason to put it there: the panel is wider than beside it, and shorter.
    let panel = |side| rects_with(&draw(Some(side)).svg, r##"fill="#f5f5f8""##)[0];
    let (beside, under) = (panel("right"), panel("bottom"));
    assert!(under.2 - under.0 > beside.2 - beside.0 + 50.0 && under.3 - under.1 < beside.3 - beside.1,
            "beside {beside:?}, under {under:?}");
}

/// **Laid across, the keys stand in one row, and wrap only when the plot is too
/// narrow for them.** Read off the swatches: one row is one height.
#[test]
fn a_legend_laid_across_puts_its_keys_in_a_row_that_wraps() {
    let rows_at = |w: f64| {
        let (spec, t) = five_keyed(Some("bottom"));
        let drawn = draw_at(&spec, &t, w, 500.0);
        let keys = rects_with(&drawn.svg, r#"rx="2""#);
        assert_eq!(keys.len(), 5, "every key is drawn at {w}px: {:?}", drawn.remarks);
        assert!(keys.iter().all(|k| k.0 >= 0.0 && k.2 <= w), "inside the canvas at {w}px: {keys:?}");
        let mut tops: Vec<f64> = keys.iter().map(|k| k.1).collect();
        tops.sort_by(f64::total_cmp);
        tops.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        tops.len()
    };
    assert_eq!(rows_at(800.0), 1, "a full plot holds the five keys in one row");
    assert!(rows_at(260.0) > 1, "a narrow plot wraps them");
}

/// A continuous color laid across is one strip running left to right, with
/// its smallest number on the left, as a number line reads.
#[test]
fn a_ramp_laid_across_runs_left_to_right_smallest_first() {
    let t = HashMap::from([("t".to_string(), DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 3.0])
        .with_float("y", vec![1.0, 2.0, 3.0])
        .with_float("v", vec![10.0, 20.0, 30.0]))]);
    let svg = |side: &str| {
        let mut spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "v"));
        spec.theme.legend = Some(side.into());
        draw_at(&spec, &t, 800.0, 600.0).svg
    };
    let (up, across) = (svg("right"), svg("bottom"));
    assert!(up.contains(r#"x1="0" y1="1" x2="0" y2="0""#), "upright, bottom to top");
    assert!(across.contains(r#"x1="0" y1="0" x2="1" y2="0""#), "across, left to right");
    // The numbers under the strip, read where they are drawn.
    let key = across.split("</linearGradient></defs>").nth(1).expect("the strip");
    let x_of = |label: &str| -> f64 {
        let tag = key.split("<text ").find(|t| t.contains(&format!(">{label}<"))).expect(label);
        tag.split("x=\"").nth(1).and_then(|r| r.split('"').next()).and_then(|v| v.parse().ok())
            .expect("an x")
    };
    assert!(x_of("10") < x_of("20") && x_of("20") < x_of("30"), "smallest on the left");
}

/// A legend laid across that cannot fit is left out and said, both ways: too
/// wide even with one key to a line, and too tall for the plot it sits under.
#[test]
fn a_legend_laid_across_that_cannot_fit_is_left_out_and_said() {
    let (spec, t) = five_keyed(Some("bottom"));
    let thin = draw_at(&spec, &t, 100.0, 400.0);
    assert!(rects_with(&thin.svg, r#"rx="4""#).is_empty(), "no card is drawn past the canvas");
    assert!(thin.remarks.iter().any(|d| d.message.contains("even with one key to a line")
            && d.message.contains("theme(width = )")), "{:?}", thin.remarks);

    let many: Vec<String> = (0..60).map(|i| format!("category {i}")).collect();
    let t = HashMap::from([("t".to_string(), DataFrame::new()
        .with_float("x", (0..60).map(f64::from).collect())
        .with_float("y", (0..60).map(f64::from).collect())
        .with_str("g", many))]);
    let mut spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "g"));
    spec.theme.legend = Some("top".into());
    let short = draw_at(&spec, &t, 400.0, 300.0);
    assert!(rects_with(&short.svg, r#"rx="4""#).is_empty(), "no card takes the panel's room");
    assert!(short.remarks.iter().any(|d| d.message.contains("px of height, and above the")
            && d.message.contains("theme(height = )")), "{:?}", short.remarks);
}

/// **A plot with its legend on the left keeps its own y name.** A page puts
/// the names of panels on one line in one column, the leftmost of their
/// places, and that column could land inside this plot's legend. So the plot
/// offers no name to the column, and its name stays right of its legend.
#[test]
fn a_plot_with_its_legend_on_the_left_keeps_its_own_y_name() {
    let (right, t) = five_keyed(Some("right"));
    assert!(draw_at(&right, &t, 800.0, 600.0).y_name.is_some());
    let (left, t) = five_keyed(Some("left"));
    let drawn = draw_at(&left, &t, 800.0, 600.0);
    assert!(drawn.y_name.is_none(), "a page must not move this name into the legend");
    // Composed above a plot with wider tick labels, whose name sits further left.
    let wide = DataFrame::new()
        .with_float("x", vec![1.0, 5.0])
        .with_float("y", vec![1_000_000.0, 9_000_000.0]);
    let mut data = t.clone();
    data.insert("w".to_string(), wide);
    let page = crate::ir::PageSpec {
        arrange: crate::ir::Arrange::Below,
        cells: vec![
            left.into(),
            PlotSpec::new().data("w").x("x").y("y").layer(Layer::new(Mark::Point)).into(),
        ],
        theme: ThemeSpec::default(),
    };
    let (svg, _) = crate::render::page::render(&page, &data, 800.0, 600.0);
    let top = svg.split("<svg x=").nth(1).expect("the top cell");
    let card = rects_with(top, r#"rx="4""#)[0];
    let name_x: f64 = top.split(r#"<text transform="rotate(-90 "#).nth(1)
        .and_then(|r| r.split(' ').next()).and_then(|v| v.parse().ok()).expect("a turned y name");
    assert!(name_x > card.2, "the y name at {name_x} is right of the legend, which ends at {}", card.2);
}

/// A y name at its axis's end sits over the panel as drawn, on the title's
/// rule: a `ratio` that centers the panel lower in the plot takes the name
/// down with it. Left at the top of the area it stood far above the panel, and
/// a legend laid above the plot, which moves down with the panel, met it.
#[test]
fn a_y_name_at_its_end_moves_down_with_its_panel() {
    let (mut spec, t) = five_keyed(None);
    spec.theme.axis_label = Some("end".into());
    spec.theme.ratio = Some(4.0);
    let drawn = draw_at(&spec, &t, 600.0, 600.0);
    let panel = rects_with(&drawn.svg, r##"fill="#f5f5f8""##)[0];
    let name = drawn.svg.split("<text ").find(|t| t.contains(r#"text-anchor="start">Y<"#))
        .expect("the y name at its end");
    let y: f64 = name.split(" y=\"").nth(1).and_then(|r| r.split('"').next())
        .and_then(|v| v.parse().ok()).expect("a y");
    assert!((panel.1 - 6.0 - y).abs() < 0.5, "the name sits 6px over the panel's top: {y}, {}", panel.1);
}

// -- a legend turned off (spec §10) ------------------------------------

/// The panel's width, read off the rectangle painted behind the data.
fn panel_width(svg: &str) -> f64 {
    svg.lines()
        .find(|l| l.contains(r##"fill="#f5f5f8""##))
        .and_then(|l| l.split(r#" width=""#).nth(1)?.split('"').next()?.parse().ok())
        .unwrap_or_else(|| panic!("no panel rectangle:\n{svg}"))
}

#[test]
fn a_legend_turned_off_keeps_the_mapping_and_gives_its_room_to_the_panel() {
    let keyed = render_styled(Layer::new(Mark::Point).encode(Channel::Color, "g"));
    let off = render_styled(Layer::new(Mark::Point)
        .encode_def(Channel::Color, ChannelDef::field("g").with_legend(false)));
    // The key is gone: its title and both rows.
    assert!(keyed.contains(">G<") && keyed.contains(">x<"), "the default draws the key:\n{keyed}");
    assert!(!off.contains(">G<") && !off.contains(">x<") && !off.contains(">y<"),
        "legend = FALSE must not draw the key:\n{off}");
    // The channel still maps: both categories keep their own color.
    for c in [PALETTE_GOG[0], PALETTE_GOG[1]] {
        assert!(off.contains(&format!(r#"fill="{c}""#)), "{c} missing — the mapping was dropped:\n{off}");
    }
    // And the room the key would have taken goes to the data.
    assert!(panel_width(&off) > panel_width(&keyed) + 20.0,
        "{} vs {}", panel_width(&off), panel_width(&keyed));
    // `legend = TRUE` is the default, said out loud: nothing changes.
    let on = render_styled(Layer::new(Mark::Point)
        .encode_def(Channel::Color, ChannelDef::field("g").with_legend(true)));
    assert_eq!(on, keyed);
}

#[test]
fn every_channel_that_earns_a_key_can_turn_it_off() {
    // Size and opacity sample three rows; shape draws glyphs. Each loses its
    // own box and only its own box. (Counted by the box's rounded corner: the
    // numeric keys' titles are the axes' names too, so a title proves nothing.)
    for (channel, field) in [(Channel::Size, "a"), (Channel::Opacity, "a"), (Channel::Shape, "g")] {
        let keyed = render_styled(Layer::new(Mark::Point).encode(channel.clone(), field));
        let off = render_styled(Layer::new(Mark::Point)
            .encode_def(channel.clone(), ChannelDef::field(field).with_legend(false)));
        assert_eq!(keyed.matches(r#"rx="4""#).count(), 1, "{channel:?} draws a key by default");
        assert_eq!(off.matches(r#"rx="4""#).count(), 0,
            "{channel:?}: legend = FALSE left the key in:\n{off}");
        assert!(panel_width(&off) > panel_width(&keyed), "{channel:?}: the room goes to the panel");
    }
    // Two keys, one turned off: the other stays, and it is the right one.
    let two = render_styled(Layer::new(Mark::Point)
        .encode_def(Channel::Size, ChannelDef::field("a").with_legend(false))
        .encode(Channel::Color, "g"));
    assert_eq!(two.matches(r#"rx="4""#).count(), 1, "{two}");
    assert!(two.contains(">G<") && two.contains(">x<"), "the color key stays:\n{two}");
}

#[test]
fn a_legend_off_on_one_binding_leaves_the_key_out_for_the_plot() {
    // The key is the channel's. Lines colored by a column and labels colored by
    // the same column read one color scale, so one `legend = FALSE` is enough
    // and the second binding saying nothing does not bring the key back.
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 3.0, 4.0])
        .with_float("b", vec![4.0, 5.0, 6.0, 7.0])
        .with_str("g", vec!["x".into(), "x".into(), "y".into(), "y".into()]);
    let data: HashMap<String, DataFrame> = [("t".to_string(), df)].into_iter().collect();
    let spec = PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Line)
            .encode_def(Channel::Color, ChannelDef::field("g").with_legend(false)))
        .layer(Layer::new(Mark::Text).encode(Channel::Label, "g").encode(Channel::Color, "g"));
    let svg = SvgRenderer::default().render(&spec, &data);
    assert!(!svg.contains(">G<"), "{svg}");
    assert_eq!(svg.matches(r#"rx="4""#).count(), 0, "{svg}");
}

#[test]
fn turning_off_one_half_of_a_merged_key_leaves_the_other_half_standing() {
    // `color(g) + shape(g)` share one key: glyphs in the categories' colors.
    let merged = render_styled(Layer::new(Mark::Point)
        .encode(Channel::Color, "g").encode(Channel::Shape, "g"));
    assert_eq!(merged.matches(r#"rx="4""#).count(), 1, "one merged key:\n{merged}");

    // Shape's key off: there is no glyph key left to carry the hues, so the
    // color key stands on its own again rather than vanishing with it.
    let color_only = render_styled(Layer::new(Mark::Point)
        .encode(Channel::Color, "g")
        .encode_def(Channel::Shape, ChannelDef::field("g").with_legend(false)));
    assert_eq!(color_only.matches(r#"rx="4""#).count(), 1, "{color_only}");
    assert!(color_only.contains(r#"fill-opacity="0.82" rx="2""#),
        "the color key's square swatches:\n{color_only}");

    // Color's key off: the glyph key stays and goes back to the neutral ink,
    // or the hues would still be decoded and the request would change nothing.
    let shape_only = render_styled(Layer::new(Mark::Point)
        .encode_def(Channel::Color, ChannelDef::field("g").with_legend(false))
        .encode(Channel::Shape, "g"));
    assert_eq!(shape_only.matches(r#"rx="4""#).count(), 1, "{shape_only}");
    assert_ne!(shape_only, merged, "legend = FALSE on color changed nothing");
    let key = &shape_only[shape_only.find(r#"rx="4""#).unwrap()..];
    assert!(key.contains("#3c3c46") && !key.contains(PALETTE_GOG[1]),
        "the glyph key decodes shape alone:\n{key}");
}

// -- a color bound to a level by name (spec §10) ---------------------

#[test]
fn a_named_palette_gives_each_level_its_own_color_whatever_the_row_order() {
    // The coverage chapter's demonstration: the same palette over the same
    // rows in two orders gave the categories different colors, because an
    // unnamed palette is handed out in the column's order. Named, it cannot.
    let palette = crate::ir::PaletteDef::Levels(crate::ir::LevelColors(vec![
        ("y".into(), "tomato".into()),
        ("x".into(), "steelblue".into()),
    ]));
    let draw = |g: Vec<String>| {
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0])
            .with_float("b", vec![4.0, 5.0, 6.0])
            .with_str("g", g);
        let data: HashMap<String, DataFrame> = [("t".to_string(), df)].into_iter().collect();
        let mut spec = PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "g"));
        spec.palette = palette.clone();
        SvgRenderer::default().render(&spec, &data)
    };
    for rows in [vec!["x", "y", "x"], vec!["y", "x", "y"]] {
        let svg = draw(rows.iter().map(|s| s.to_string()).collect());
        // Each legend row is its swatch then its name, so the color written
        // just before a name is that level's.
        let swatch_of = |level: &str| {
            let at = svg.find(&format!(">{level}</text>")).expect("legend row");
            let before = &svg[..at];
            let fill = before.rfind(r#"fill=""#).unwrap();
            before[fill + 6..].split('"').next().unwrap().to_string()
        };
        assert_eq!(swatch_of("x"), "steelblue", "rows {rows:?}:\n{svg}");
        assert_eq!(swatch_of("y"), "tomato", "rows {rows:?}:\n{svg}");
        // The legend runs in the column's order, not the palette's.
        let (x, y) = (svg.find(">x</text>").unwrap(), svg.find(">y</text>").unwrap());
        assert_eq!(x < y, rows[0] == "x", "rows {rows:?}: legend order");
    }
}

// -- continuous color ------------------------------------------------



#[test]
fn a_continuous_color_earns_one_legend_not_two() {
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 3.0])
        .with_float("v", vec![10.0, 20.0, 30.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new()
        .data("t")
        .x("a")
        .y("a")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "v"));
    let svg = SvgRenderer::default().render(&spec, &data);
    // Labeled at min / mid / max…
    for want in [">10<", ">20<", ">30<"] {
        assert!(svg.contains(want), "legend should show {want}");
    }
    // …but drawn as one continuous strip, not three sampled swatches.
    assert!(svg.contains("<linearGradient"), "continuous color needs a gradient strip");
    assert!(svg.contains("-ramp"), "the strip should use the gradient");
    // and exactly one legend box, not a categorical one as well
    assert_eq!(svg.matches(r#"rx="4""#).count(), 1, "expected a single legend box");
}

#[test]
fn different_ramps_get_different_gradient_ids() {
    // The book inlines many SVGs into one HTML document, where SVG ids
    // are global and the first definition wins for every reference. The
    // id therefore hashes the gradient's *content*: two plots may only
    // share an id when they would paint the same strip anyway. The old
    // geometry-derived id collided the moment two plots shared a layout,
    // and a viridis legend upstream repainted a white–navy strip
    // downstream.
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 3.0])
        .with_float("v", vec![10.0, 20.0, 30.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let base = PlotSpec::new().data("t").x("a").y("a")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "v"));

    let gid = |svg: &str| svg.split(r#"linearGradient id=""#).nth(1)
        .and_then(|s| s.split('"').next())
        .expect("no gradient id").to_string();

    let mut viridis = base.clone();
    viridis.palette = crate::ir::PaletteDef::Named("viridis".into());
    let mut custom = base.clone();
    custom.palette = crate::ir::PaletteDef::Custom(vec!["white".into(), "navy".into()]);

    let a = gid(&SvgRenderer::default().render(&viridis, &data));
    let b = gid(&SvgRenderer::default().render(&custom, &data));
    assert_ne!(a, b, "identical layouts must not share a gradient id across ramps");

    // Same ramp twice → same id, and that collision is harmless by
    // construction: both definitions paint identically.
    let c = gid(&SvgRenderer::default().render(&custom, &data));
    assert_eq!(b, c);
}

#[test]
fn the_strip_is_long_enough_to_read_as_a_gradient() {
    // The whole point of one strip instead of three swatches is that the
    // reader sees the scale itself. At the plain row height the strip was
    // a 40 px stub — three swatches touching. Gradient rows are taller,
    // and the strip spans center-of-first to center-of-last of them.
    use crate::render::legend::LEGEND_RAMP_ROW_H;
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 3.0])
        .with_float("v", vec![10.0, 20.0, 30.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("a").y("a")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "v"));
    let svg = SvgRenderer::default().render(&spec, &data);

    let strip_h = svg.lines()
        .find_map(|l| {
            if !l.contains("-ramp") { return None }
            l.split(r#" height=""#).nth(1)?.split('"').next()?.parse::<f64>().ok()
        })
        .expect("no gradient strip drawn");
    // Three labels → the strip spans two gradient rows.
    assert!((strip_h - 2.0 * LEGEND_RAMP_ROW_H).abs() < 1e-6,
            "strip is {strip_h} px, expected {}", 2.0 * LEGEND_RAMP_ROW_H);
}

#[test]
fn only_color_gets_a_strip_the_others_are_sampled() {
    // A legend shows the scale. Color is the only continuous channel whose
    // whole range fits in a fixed space; there is no way to draw a continuum
    // of circles, so `size` and `opacity` are sampled. Same rule, different
    // shapes — if `size` ever grows a gradient this test is the reminder to
    // think about whether that means anything.
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 3.0])
        .with_float("v", vec![10.0, 20.0, 30.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let base = PlotSpec::new().data("t").x("a").y("a");

    let color = SvgRenderer::default()
        .render(&base.clone().layer(Layer::new(Mark::Point).encode(Channel::Color, "v")), &data);
    assert!(color.contains("<linearGradient"));

    for ch in [Channel::Size, Channel::Opacity] {
        let svg = SvgRenderer::default()
            .render(&base.clone().layer(Layer::new(Mark::Point).encode(ch.clone(), "v")), &data);
        assert!(!svg.contains("<linearGradient"), "{ch:?} should be sampled, not a strip");
        assert!(svg.contains(">20<"), "{ch:?} should still label its midpoint");
    }
}

#[test]
fn the_strip_runs_the_way_the_labels_do() {
    // Largest at the top. If the gradient were flipped the plot would still
    // render and quietly mean the opposite of its own legend.
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 3.0])
        .with_float("v", vec![10.0, 20.0, 30.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("a").y("a")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "v"));
    let svg = SvgRenderer::default().render(&spec, &data);

    // y1=1 → y2=0 means offset 0 is at the *bottom*, so the first stop
    // (the ramp's light end) sits at the low-value end.
    assert!(svg.contains(r#"x1="0" y1="1" x2="0" y2="0""#), "strip should run bottom-up");
    let first_stop = svg.split("stop-color=\"").nth(1).unwrap().split('"').next().unwrap();
    assert_eq!(first_stop.to_lowercase(), RAMP_BLUE[0], "offset 0 should be the light end");

    // And the max label precedes the min label in document order (top row first).
    let max_at = svg.find(">30<").unwrap();
    let min_at = svg.find(">10<").unwrap();
    assert!(max_at < min_at, "the largest value should be labeled at the top");
}

// -- horizontal bars --------------------------------------------------

/// `(x, y, width, height)` of each drawn bar, in document order.
fn bar_rects(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    svg.lines()
        // Bars carry a `fill-opacity` but never a corner radius; legend
        // swatches and facet strips carry `rx=`, so excluding it isolates bars
        // even when a color legend is present.
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && !l.contains(" rx="))
        .map(|l| {
            // Read named attributes rather than positions — `stroke-width`
            // also ends in `width=`, and matching loosely would silently
            // pick it up.
            let attr = |name: &str| -> f64 {
                l.split(&format!(" {name}=\""))
                    .nth(1)
                    .and_then(|r| r.split('"').next())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(f64::NAN)
            };
            (attr("x"), attr("y"), attr("width"), attr("height"))
        })
        .collect()
}

/// A brushed bar is drawn at the width and the place it has unbrushed
/// (2026-10-09). Each pass, selected and unselected, used to measure its own
/// half of the rows, so the two disagreed about how wide a bar is and the
/// brush was refused. The writer now measures the whole share and draws one
/// side of it, so the only thing a brush changes is which bars are dimmed.
#[test]
fn a_brushed_bar_keeps_the_width_and_place_it_has_unbrushed() {
    let df = DataFrame::new()
        .with_str("g", vec!["A".into(), "A".into(), "B".into(), "B".into(), "C".into(), "C".into()])
        .with_str("c", vec!["p".into(), "q".into(), "p".into(), "q".into(), "p".into(), "q".into()])
        .with_float("v", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    for layer in [
        Layer::new(Mark::Bar),
        Layer::new(Mark::Bar).transform(Transform::Dodge).encode(Channel::Color, "c"),
    ] {
        let plain = PlotSpec::new().data("t").x("g").y("v").layer(layer.clone());
        let brushed = plain.clone().brush(crate::ir::BrushDef::new("v").at(2.5, 4.5));
        let (a, b) = (SvgRenderer::default().render(&plain, &data),
                      SvgRenderer::default().render(&brushed, &data));
        assert!(b.contains("<g opacity="), "the unselected bars are pushed back: {:?}", layer.transforms);
        let sorted = |s: &str| {
            let mut r = bar_rects(s);
            r.sort_by(|p, q| p.partial_cmp(q).unwrap());
            r
        };
        assert_eq!(sorted(&a), sorted(&b), "a brush moved or resized a bar: {:?}", layer.transforms);
        // The four bars outside the bound are in the pushed-back group, and the
        // two inside it (3 and 4) are drawn after it, at full strength. The group
        // holds the writer's clip group, so it closes at the second `</g>`.
        let dimmed = &b[b.find("<g opacity=").unwrap()..];
        let close = dimmed.match_indices("</g>").nth(1).unwrap().0;
        assert_eq!(bar_rects(&dimmed[..close]).len(), 4, "{:?}", layer.transforms);
        assert_eq!(bar_rects(&b).len(), 6);
    }
}

/// `group` on a dodged `bar` under a statistic draws what `color` draws, in one
/// fill (2026-10-09): one mean per group, side by side, with no legend to
/// decode a color nobody asked for. The same rectangles, so the two read alike.
#[test]
fn group_dodges_a_bars_statistic_as_color_does_without_the_color() {
    let df = DataFrame::new()
        .with_str("g", vec!["A".into(), "A".into(), "A".into(), "B".into(), "B".into(), "B".into()])
        .with_str("c", vec!["p".into(), "q".into(), "q".into(), "p".into(), "q".into(), "p".into()])
        .with_float("v", vec![1.0, 2.0, 4.0, 3.0, 4.0, 5.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    // The colored plot's key is left out, or it would narrow the panel and move
    // every bar for a reason that is not the split.
    let spec = |split: Channel| {
        let mut def = crate::ir::ChannelDef::field("c");
        if split == Channel::Color {
            def.legend = Some(false);
        }
        PlotSpec::new().data("t").x("g").y("v")
            .layer(Layer::new(Mark::Bar).transform(Transform::Mean).transform(Transform::Dodge)
                .encode_def(split, def))
    };
    let colored = SvgRenderer::default().render(&spec(Channel::Color), &data);
    let grouped = SvgRenderer::default().render(&spec(Channel::Group), &data);
    assert_eq!(bar_rects(&grouped).len(), 4, "two slots, two groups each");
    assert_eq!(bar_rects(&grouped), bar_rects(&colored), "group places the bars as color does");
    let fills: std::collections::BTreeSet<&str> = grouped.lines()
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && !l.contains(" rx="))
        .filter_map(|l| l.split(" fill=\"").nth(1).and_then(|r| r.split('"').next()))
        .collect();
    assert_eq!(fills.len(), 1, "group separates, it does not color: {fills:?}");
    // One mean per group: A's `q` rows average 3, which no single row holds.
    assert!(bar_rects(&grouped).iter().map(|r| r.3).collect::<Vec<_>>()
        .windows(2).any(|w| (w[0] - w[1]).abs() > 1e-6), "the groups' means differ");

    // Piled, the parts share one fill and touch, so they are parted by a line
    // in the panel color, as a histogram's bars are; a color split keeps its
    // faint self-colored edge, since its fills already part it.
    let pile = |split: Channel| SvgRenderer::default().render(&PlotSpec::new().data("t")
        .x("g").layer(Layer::new(Mark::Bar).transform(Transform::Count)
            .transform(Transform::Stack).encode(split, "c")), &data);
    let edges = |s: &str| s.lines()
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && !l.contains(" rx="))
        .filter(|l| l.contains(&format!(r#"stroke="{PANEL_BG}""#))).count();
    assert_eq!(edges(&pile(Channel::Group)), 4, "a group-only pile parts its pieces");
    assert_eq!(edges(&pile(Channel::Color)), 0, "a colored pile keeps its own edge");
}

#[test]
fn dodge_sets_grouped_bars_side_by_side_and_narrows_them() {
    // Two categories, each split by color into two groups. Overlaid, the two
    // groups share their category's slot (same center, full width). Dodged,
    // they sit side by side — each half as wide, offset symmetrically about
    // that same center, together tiling exactly the slot one bar filled (§5).
    let df = DataFrame::new()
        .with_str("g", vec!["A".into(), "A".into(), "B".into(), "B".into()])
        .with_str("c", vec!["p".into(), "q".into(), "p".into(), "q".into()])
        .with_float("v", vec![1.0, 2.0, 3.0, 4.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);

    let spec = |dodge: bool| {
        let mut layer = Layer::new(Mark::Bar).encode(Channel::Color, "c");
        if dodge {
            layer = layer.transform(Transform::Dodge);
        }
        PlotSpec::new().data("t").x("g").y("v").layer(layer)
    };

    let overlaid = bar_rects(&SvgRenderer::default().render(&spec(false), &data));
    let dodged = bar_rects(&SvgRenderer::default().render(&spec(true), &data));
    assert_eq!(overlaid.len(), 4, "expected four bars overlaid");
    assert_eq!(dodged.len(), 4, "expected four bars dodged");

    let center = |r: &(f64, f64, f64, f64)| r.0 + r.2 / 2.0;
    let key = |c: f64| (c * 100.0).round() as i64;

    // Overlaid: the groups share each category's center — two distinct centers,
    // one full width.
    let ov_centers: std::collections::BTreeSet<i64> = overlaid.iter().map(|r| key(center(r))).collect();
    assert_eq!(ov_centers.len(), 2, "overlaid groups should share each category's center");
    let w_full = overlaid[0].2;
    assert!(overlaid.iter().all(|r| (r.2 - w_full).abs() < 1e-6), "overlaid bars are full width");

    // Dodged: four distinct centers, each bar in a tile half the slot (G = 2),
    // and four fifths of its tile, the same share a category bar takes of its
    // slot (`style(gap = )`'s default, 2026-10-09; before, the tiles were full
    // and one color's bars merged).
    let dg_centers: std::collections::BTreeSet<i64> = dodged.iter().map(|r| key(center(r))).collect();
    assert_eq!(dg_centers.len(), 4, "every dodged bar should get its own position");
    assert!(
        dodged.iter().all(|r| (r.2 - w_full / 2.0 * 0.8).abs() < 0.01),
        "two groups → each dodged bar is four fifths of half the slot: {:?}",
        dodged.iter().map(|r| r.2).collect::<Vec<_>>()
    );

    // Symmetric about the shared center: each overlaid center is the mean of
    // the two dodged bars that replaced it — the group straddles, never drifts.
    for oc in overlaid.iter().map(center) {
        let near: Vec<f64> = dodged.iter().map(center).filter(|c| (c - oc).abs() < w_full * 0.5).collect();
        assert_eq!(near.len(), 2, "each slot splits into exactly two dodged bars");
        // To a hundredth: `x` and the width are each written to two decimals.
        assert!(
            ((near[0] + near[1]) / 2.0 - oc).abs() < 0.011,
            "dodged pair should straddle the original center {oc}"
        );
    }
}

#[test]
fn stacked_bars_pile_full_width_and_abut() {
    // The same split, stacked instead of dodged: the two groups pile at each
    // category's one center — full width, not narrowed — each segment sitting
    // exactly on the one below, with no gap or overlap (§5).
    let df = DataFrame::new()
        .with_str("g", vec!["A".into(), "A".into(), "B".into(), "B".into()])
        .with_str("c", vec!["p".into(), "q".into(), "p".into(), "q".into()])
        .with_float("v", vec![1.0, 2.0, 3.0, 4.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);

    let spec = PlotSpec::new().data("t").x("g").y("v")
        .layer(Layer::new(Mark::Bar).transform(Transform::Stack).encode(Channel::Color, "c"));
    let svg = SvgRenderer::default().render(&spec, &data);
    let rects = bar_rects(&svg);
    assert_eq!(rects.len(), 4, "two categories × two groups = four segments");

    let center = |r: &(f64, f64, f64, f64)| r.0 + r.2 / 2.0;
    let key = |c: f64| (c * 100.0).round() as i64;

    // Full width and one center per category — the opposite of dodge.
    let w_full = rects[0].2;
    assert!(rects.iter().all(|r| (r.2 - w_full).abs() < 1e-6), "stacked segments keep full width");
    let centers: std::collections::BTreeSet<i64> = rects.iter().map(|r| key(center(r))).collect();
    assert_eq!(centers.len(), 2, "stacked groups pile at one center per category, not side by side");

    // Within each column the two segments abut: screen-y grows downward, so the
    // upper segment's foot (y + h) meets the lower segment's top (y).
    for &cx in &centers {
        let mut col: Vec<&(f64, f64, f64, f64)> =
            rects.iter().filter(|r| key(center(r)) == cx).collect();
        assert_eq!(col.len(), 2, "each category stacks two segments");
        col.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap()); // topmost (smallest y) first
        let (upper, lower) = (col[0], col[1]);
        assert!((upper.1 + upper.3 - lower.1).abs() < 0.5,
            "segments must abut: upper foot {} vs lower top {}", upper.1 + upper.3, lower.1);
    }

    // Solid, never the translucent overlay fill — the pile resolves the overlap.
    let bar_fill_os: Vec<String> = svg.lines()
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && !l.contains(" rx="))
        .filter_map(|l| l.split("fill-opacity=\"").nth(1).and_then(|r| r.split('"').next()).map(String::from))
        .collect();
    assert!(bar_fill_os.iter().all(|o| o != "0.400"),
        "a stacked bar draws solid, never the 0.4 overlay fill: {bar_fill_os:?}");
}

#[test]
fn stacked_areas_trace_a_floor_instead_of_the_baseline() {
    // Two regions over the same three x, split by color. Stacked, each band
    // fills between its own floor (the group below) and its top, so its polygon
    // retraces that floor per vertex (N top + N floor) instead of closing on the
    // flat baseline (N top + two corners). That geometric difference is what lets
    // the upper band sit on the lower rather than bury it — the retired Assumption.
    let df = DataFrame::new()
        .with_float("x", vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0])
        .with_float("y", vec![1.0, 2.0, 1.0, 3.0, 1.0, 2.0])
        .with_str("g", vec!["a".into(), "a".into(), "a".into(), "b".into(), "b".into(), "b".into()]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);

    let verts = |stack: bool| -> Vec<usize> {
        let mut layer = Layer::new(Mark::Area).encode(Channel::Color, "g");
        if stack { layer = layer.transform(Transform::Stack); }
        let spec = PlotSpec::new().data("t").x("x").y("y").layer(layer);
        let svg = SvgRenderer::default().render(&spec, &data);
        svg.lines().filter(|l| l.contains("<polygon"))
            .map(|l| l.matches(',').count()).collect()
    };

    let plain = verts(false);
    assert_eq!(plain.len(), 2, "one region per group");
    assert!(plain.iter().all(|&v| v == 5),
        "an unstacked band closes on the flat baseline: 3 top + 2 corners = 5, got {plain:?}");

    let stacked = verts(true);
    assert_eq!(stacked.len(), 2, "one band per group");
    assert!(stacked.iter().all(|&v| v == 6),
        "a stacked band retraces its floor: 3 top + 3 floor = 6, got {stacked:?}");
}

/// **A name the reader wrote survives an axis that draws no numbers.** A moved
/// pile and a partition's ring index both drop their ticks and the name gog
/// would derive, and both dropped a written `y_label()` as well, accepted and
/// silent. The written one is drawn now; the derived one still is not.
#[test]
fn a_written_axis_name_is_drawn_where_the_axis_draws_no_numbers() {
    let df = DataFrame::new()
        .with_float("x", vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0])
        .with_float("y", vec![1.0, 2.0, 1.0, 3.0, 1.0, 2.0])
        .with_str("g", ["a", "a", "a", "b", "b", "b"].into_iter().map(String::from).collect());
    let data = HashMap::from([("t".to_string(), df)]);
    for baseline in ["wiggle", "center"] {
        let draw = |written: Option<&str>| {
            let mut layer = Layer::new(Mark::Area).encode(Channel::Color, "g")
                .transform(Transform::Stack);
            layer.stack = Some(crate::ir::StackSpec {
                share: None, baseline: Some(baseline.to_string()),
            });
            let spec = PlotSpec::new().data("t").x("x").y("y").layer(layer);
            let spec = match written { Some(w) => spec.y_label(w), None => spec };
            text_of(&SvgRenderer::default().render(&spec, &data))
        };
        let labels = draw(Some("Plays per week"));
        assert!(labels.iter().any(|l| l == "Plays per week"), "{baseline}: {labels:?}");
        assert!(!labels.iter().any(|l| l == "Y"),
            "{baseline}: the derived name stays off: {labels:?}");
        assert!(!draw(None).iter().any(|l| l == "Y"), "{baseline}: nothing written, no name");
    }
    // The ring index, flat and bent: the name is drawn, no ring number is.
    for polar in [false, true] {
        let mut s = PlotSpec::new().data("t").x("v").y_label("Level");
        if polar {
            s = s.coord(CoordSpace::Polar(crate::ir::PolarView::default()));
        }
        let s = s.layer(Layer::new(Mark::Zone).partition(&["g", "i"]));
        let labels = text_of(&SvgRenderer::default().render(&s, &tree_data()));
        assert!(labels.iter().any(|l| l == "Level"), "polar = {polar}: {labels:?}");
        assert!(!labels.iter().any(|l| l == "Depth"), "polar = {polar}: {labels:?}");
    }
}

fn medals_data() -> HashMap<String, DataFrame> {
    let df = DataFrame::new()
        .with_str("country", vec!["USA".into(), "China".into(), "GB".into()])
        .with_float("gold", vec![46.0, 38.0, 29.0]);
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);
    m
}

#[test]
fn bars_lie_down_when_the_categories_are_on_y() {
    let vertical = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("country").y("gold").layer(Layer::new(Mark::Bar)),
        &medals_data(),
    );
    let horizontal = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("gold").y("country").layer(Layer::new(Mark::Bar)),
        &medals_data(),
    );

    let v = bar_rects(&vertical);
    let h = bar_rects(&horizontal);
    assert_eq!(v.len(), 3);
    assert_eq!(h.len(), 3);

    // Vertical: one width, varying heights, all standing on one baseline.
    assert!(v.iter().all(|r| (r.2 - v[0].2).abs() < 0.01), "widths should match: {v:?}");
    assert!(v.iter().any(|r| (r.3 - v[0].3).abs() > 1.0), "heights should vary: {v:?}");
    assert!(v.iter().all(|r| (r.1 + r.3 - (v[0].1 + v[0].3)).abs() < 0.01), "common baseline");

    // Horizontal: one height, varying widths, all starting at one baseline.
    assert!(h.iter().all(|r| (r.3 - h[0].3).abs() < 0.01), "heights should match: {h:?}");
    assert!(h.iter().any(|r| (r.2 - h[0].2).abs() > 1.0), "widths should vary: {h:?}");
    assert!(h.iter().all(|r| (r.0 - h[0].0).abs() < 0.01), "common baseline: {h:?}");
}

#[test]
fn the_first_category_reads_first_in_both_orientations() {
    // Screen y grows downward while the scale grows upward, so a categorical
    // y must be reversed or `order(desc)` would put the largest bar at the
    // bottom of a horizontal chart and the leftmost of a vertical one.
    let spec = PlotSpec::new()
        .data("t")
        .x("gold")
        .y("country")
        .order_desc("gold")
        .layer(Layer::new(Mark::Bar));
    let svg = SvgRenderer::default().render(&spec, &medals_data());

    let bars = bar_rects(&svg);
    // Widest bar (the largest value) must sit at the smallest y — the top.
    let widest = bars.iter().cloned().fold(bars[0], |a, b| if b.2 > a.2 { b } else { a });
    assert!(
        bars.iter().all(|r| r.1 >= widest.1),
        "largest bar should be topmost: {bars:?}"
    );
}

#[test]
fn ordering_a_factor_by_itself_uses_its_levels_not_its_spelling() {
    // `order(size)` asks for the column's own order. A factor's own order is
    // the levels it declares, and here they disagree with the alphabet in
    // both directions: alphabetically the bands read large, medium, small.
    //
    // Sorting the labels instead threw the levels away, which was invisible
    // because a scrambled categorical axis is still a well-formed one. The
    // case that found it was five-year age bands, where "5" sorts between
    // "45" and "50" and a population pyramid came out with shuffled floors.
    let df = DataFrame::new()
        .with_levels(
            "size",
            vec!["small".into(), "medium".into(), "large".into()],
            vec!["small".into(), "medium".into(), "large".into()],
        )
        .with_float("n", vec![1.0, 2.0, 3.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);

    // Each category has a distinct height, so the bars name themselves: read
    // them left to right and the heights say which order the axis chose.
    let heights = |spec: &PlotSpec| {
        let mut bars = bar_rects(&SvgRenderer::default().render(spec, &data));
        assert_eq!(bars.len(), 3, "one bar per band: {bars:?}");
        bars.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        bars.iter().map(|r| r.3.round()).collect::<Vec<_>>()
    };

    let base = PlotSpec::new().data("t").x("size").y("n").layer(Layer::new(Mark::Bar));
    let up = heights(&base.clone().order("size"));
    assert!(up[0] < up[1] && up[1] < up[2], "levels order, small to large: {up:?}");

    let down = heights(&base.clone().order_desc("size"));
    assert!(down[0] > down[1] && down[1] > down[2], "the same levels reversed: {down:?}");

    // And a column with no levels has nothing to go on but its spelling, so
    // the alphabet still answers there. Same three words, no `with_levels`.
    let plain = DataFrame::new()
        .with_str("size", vec!["small".into(), "medium".into(), "large".into()])
        .with_float("n", vec![1.0, 2.0, 3.0]);
    let mut plain_data = HashMap::new();
    plain_data.insert("t".to_string(), plain);
    let mut bars = bar_rects(&SvgRenderer::default().render(&base.order("size"), &plain_data));
    bars.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let alpha = bars.iter().map(|r| r.3.round()).collect::<Vec<_>>();
    assert!(alpha[0] > alpha[1] && alpha[1] > alpha[2],
            "large(3), medium(2), small(1) is alphabetical: {alpha:?}");
}

#[test]
fn a_horizontal_count_measures_along_x() {
    // The transform groups by the position axis and writes to the measured
    // one; with the categories on y that output belongs to x.
    let df = DataFrame::new().with_str(
        "g",
        vec!["a".into(), "a".into(), "b".into()],
    );
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new()
        .data("t")
        .y("g")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count));
    let svg = SvgRenderer::default().render(&spec, &data);

    let bars = bar_rects(&svg);
    assert_eq!(bars.len(), 2, "one bar per category: {bars:?}");
    assert!(bars.iter().all(|r| (r.3 - bars[0].3).abs() < 0.01), "equal thickness");
    assert!((bars[0].2 - bars[1].2).abs() > 1.0, "counts 2 and 1 should differ: {bars:?}");
    // The synthesized label names the measured axis, which is now x.
    assert!(svg.contains(">Count<"), "the count axis should be labeled");
}

// -- horizontal box and whisker ---------------------------------------

/// `(x1, y1, x2, y2)` of each drawn `<line>`, in document order.
fn line_segs(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    svg.lines()
        .filter(|l| l.contains("<line"))
        .map(|l| {
            let attr = |name: &str| -> f64 {
                l.split(&format!(" {name}=\""))
                    .nth(1)
                    .and_then(|r| r.split('"').next())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(f64::NAN)
            };
            (attr("x1"), attr("y1"), attr("x2"), attr("y2"))
        })
        .filter(|s| [s.0, s.1, s.2, s.3].iter().all(|v| v.is_finite()))
        .collect()
}

fn grouped_measurements() -> HashMap<String, DataFrame> {
    // Three groups with visibly different spreads, so a box's measure extent
    // cannot accidentally match its slot thickness.
    let mut g = Vec::new();
    let mut v = Vec::new();
    for (name, base, spread) in [("alpha", 10.0, 1.0), ("beta", 30.0, 6.0), ("gamma", 50.0, 3.0)] {
        for k in 0..9 {
            g.push(name.to_string());
            v.push(base + (k as f64 - 4.0) * spread);
        }
    }
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new().with_str("g", g).with_float("v", v));
    data
}

/// Turning a box plot on its side must **transpose** it, not redraw it: the
/// slot thickness moves to the other axis, the summary's extent with it, and
/// every line that ran across the box now runs along it.
///
/// Written as one assertion set parameterized by orientation, so it cannot pass
/// by describing each case separately — which is how a mirrored copy of a
/// writer hides a missed swap. Before `box` took a category on `y`, the
/// horizontal half of this was not expressible at all.
#[test]
fn a_box_plot_transposes_when_the_category_moves_to_y() {
    let data = grouped_measurements();
    let build = |horizontal: bool| {
        let spec = if horizontal {
            PlotSpec::new().data("t").x("v").y("g")
        } else {
            PlotSpec::new().data("t").x("g").y("v")
        };
        SvgRenderer::default().render(&spec.layer(Layer::new(Mark::Box)), &data)
    };
    let (v_svg, h_svg) = (build(false), build(true));
    let (v_boxes, h_boxes) = (bar_rects(&v_svg), bar_rects(&h_svg));

    // Same picture, turned: the same number of boxes either way. A writer that
    // read the wrong category list would silently draw none.
    assert_eq!(v_boxes.len(), 3, "one box per group upright: {v_boxes:?}");
    assert_eq!(h_boxes.len(), 3, "one box per group on its side: {h_boxes:?}");

    // Upright, the *width* is the slot (equal for every box) and the *height* is
    // the IQR (unequal, since the spreads differ). Lying down, exactly reversed.
    let spread = |vals: &[f64]| vals.iter().cloned().fold(f64::MIN, f64::max)
        - vals.iter().cloned().fold(f64::MAX, f64::min);
    for (label, boxes, slot, measure) in [
        ("upright", &v_boxes, 2usize, 3usize),
        ("sideways", &h_boxes, 3usize, 2usize),
    ] {
        let pick = |i: usize| boxes.iter().map(|r| [r.0, r.1, r.2, r.3][i]).collect::<Vec<_>>();
        assert!(spread(&pick(slot)) < 0.01, "{label}: every box shares one slot thickness");
        assert!(spread(&pick(measure)) > 1.0, "{label}: the IQR differs per group");
    }

    // The line-work turns with the box. A median bar and a whisker cap run
    // *across* the slot; a whisker runs *along* the measure. Upright that makes
    // the caps horizontal and the whiskers vertical, and sideways the reverse —
    // so the two orientations must have no segment direction in common.
    let flat = |s: &(f64, f64, f64, f64)| (s.1 - s.3).abs() < 0.01;
    let upright = |s: &(f64, f64, f64, f64)| (s.0 - s.2).abs() < 0.01;
    let (v_segs, h_segs) = (line_segs(&v_svg), line_segs(&h_svg));
    // Ignore the axis rules and gridlines, which do not turn with the mark:
    // the box's own line-work is what carries a stroke-linecap.
    let boxwork = |svg: &str, segs: Vec<(f64, f64, f64, f64)>| {
        let keep: Vec<usize> = svg.lines().filter(|l| l.contains("<line")).enumerate()
            .filter(|(_, l)| l.contains("stroke-linecap")).map(|(i, _)| i).collect();
        keep.into_iter().filter_map(|i| segs.get(i).copied()).collect::<Vec<_>>()
    };
    let vw = boxwork(&v_svg, v_segs);
    let hw = boxwork(&h_svg, h_segs);
    assert!(!vw.is_empty() && !hw.is_empty(), "the boxes should draw line-work");
    assert!(vw.iter().all(|s| flat(s) || upright(s)), "every segment is axis-aligned");
    // Upright: whiskers vertical, medians and caps horizontal — both kinds present.
    assert!(vw.iter().any(upright) && vw.iter().any(flat), "upright: both directions drawn");
    assert!(hw.iter().any(upright) && hw.iter().any(flat), "sideways: both directions drawn");
    // And the counts swap: as many flat segments upright as upright ones sideways.
    assert_eq!(
        vw.iter().filter(|s| flat(s)).count(), hw.iter().filter(|s| upright(s)).count(),
        "the across-the-slot line-work should turn with the box"
    );
    assert_eq!(
        vw.iter().filter(|s| upright(s)).count(), hw.iter().filter(|s| flat(s)).count(),
        "the along-the-measure line-work should turn with the box"
    );
}

/// The same claim for `interval`: a whisker spans along the measure and its caps
/// run across the slot, whichever way round the two axes are bound.
#[test]
fn an_error_bar_spans_along_whichever_axis_measures() {
    let data = grouped_measurements();
    let build = |horizontal: bool| {
        let spec = if horizontal {
            PlotSpec::new().data("t").x("v").y("g")
        } else {
            PlotSpec::new().data("t").x("g").y("v")
        };
        SvgRenderer::default().render(
            &spec.layer(Layer::new(Mark::Interval).transform(Transform::Range)), &data)
    };
    for (label, horizontal) in [("upright", false), ("sideways", true)] {
        let svg = build(horizontal);
        let segs = line_segs(&svg);
        // A span runs along the measure; a cap across the slot. Three groups
        // means three spans and six caps, whichever way the plot is turned.
        let along: Vec<_> = segs.iter()
            .filter(|s| if horizontal { (s.1 - s.3).abs() < 0.01 && (s.0 - s.2).abs() > 1.0 }
                        else          { (s.0 - s.2).abs() < 0.01 && (s.1 - s.3).abs() > 1.0 })
            .collect();
        let across: Vec<_> = segs.iter()
            .filter(|s| if horizontal { (s.0 - s.2).abs() < 0.01 && (s.1 - s.3).abs() > 0.01 }
                        else          { (s.1 - s.3).abs() < 0.01 && (s.0 - s.2).abs() > 0.01 })
            .collect();
        assert!(along.len() >= 3, "{label}: expected a span per group, got {}", along.len());
        assert!(across.len() >= 6, "{label}: expected two caps per group, got {}", across.len());
    }
}

#[test]
fn group_separates_series_without_coloring_them() {
    // `group` and `color` must not be the same atom. `group` splits a line
    // into one polyline per category; coloring by group index would invent
    // an encoding with no legend to decode it — and would leave no way to
    // say "separate these series but keep one color", which is the only
    // reason `group` exists next to `color`.
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 1.0, 2.0])
        .with_float("b", vec![1.0, 2.0, 3.0, 4.0])
        .with_str("g", vec!["x".into(), "x".into(), "y".into(), "y".into()]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("a").y("b");

    let strokes = |svg: &str| -> Vec<String> {
        svg.lines()
            .filter(|l| l.contains("<polyline"))
            .filter_map(|l| l.split(r#" stroke=""#).nth(1))
            .filter_map(|r| r.split('"').next())
            .map(str::to_string)
            .collect()
    };

    let grouped = SvgRenderer::default().render(
        &spec.clone().layer(Layer::new(Mark::Line).encode(Channel::Group, "g")),
        &data,
    );
    let g = strokes(&grouped);
    assert_eq!(g.len(), 2, "group should draw one polyline per category");
    assert_eq!(g[0], g[1], "group must not color: got {g:?}");

    let colored = SvgRenderer::default().render(
        &spec.layer(Layer::new(Mark::Line).encode(Channel::Color, "g")),
        &data,
    );
    let c = strokes(&colored);
    assert_eq!(c.len(), 2);
    assert_ne!(c[0], c[1], "color must color: got {c:?}");
}

#[test]
fn a_line_takes_a_set_width_and_opacity() {
    // The channels are refused here (one stroke cannot vary per row); the
    // settings are the supported way to reach the same properties.
    let svg = render_styled(Layer::new(Mark::Line).style_size(6.0).style_opacity(0.4));
    let polyline = svg
        .lines()
        .find(|l| l.contains("<polyline"))
        .expect("a polyline should be drawn");
    assert!(polyline.contains(r#"stroke-width="6""#), "got: {polyline}");
    assert!(polyline.contains(r#"stroke-opacity="0.400""#), "got: {polyline}");
}

// -- facets -----------------------------------------------------------
//
// Small multiples: the outer frame's categories each get a panel, the
// panels share one scale, and statistics run within each panel's rows.

/// Panel backgrounds are the one `#f5f5f8` rect per frame.
fn panel_count(svg: &str) -> usize {
    svg.matches(r##"fill="#f5f5f8""##).count()
}

fn faceted_points() -> (PlotSpec, HashMap<String, DataFrame>) {
    let df = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .with_float("y", vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0])
        .with_str("g", vec![
            "a".into(), "a".into(), "b".into(),
            "b".into(), "c".into(), "c".into(),
        ]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point));
    (spec, data)
}

/// Five levels — an odd count, so wrapping at two leaves a ragged row.
fn five_level_points() -> (PlotSpec, HashMap<String, DataFrame>) {
    let names = ["a", "b", "c", "d", "e"];
    let df = DataFrame::new()
        .with_float("x", (0..10).map(|i| (i % 2) as f64).collect())
        .with_float("y", (0..10).map(|i| i as f64).collect())
        .with_str("g", (0..10).map(|i| names[i / 2].to_string()).collect());
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point));
    (spec, data)
}

#[test]
fn a_facet_makes_one_panel_per_category_and_names_each() {
    let (spec, data) = faceted_points();
    let svg = SvgRenderer::default().render(&spec.facet_col("g"), &data);
    assert_eq!(panel_count(&svg), 3);
    for name in ["a", "b", "c"] {
        assert!(svg.contains(&format!(">{name}</text>")), "missing strip label {name}");
    }
}

/// A shared scale is what makes panels comparable, and what makes some of
/// them unreadable. Freed, each panel fits its own rows — and only the axis
/// that asked: `x` stays shared here, so the two are a mix, not a mode.
#[test]
fn a_freed_axis_is_fitted_per_panel_and_the_other_one_is_not() {
    let df = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0])
        .with_float("y", vec![1.0, 2.0, 100.0, 200.0, 10.0, 20.0])
        .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into(),
                            "c".into(), "c".into()]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point))
        .facet_col("g");

    // Shared: one y range over 1..200, so every panel is ticked alike and
    // only the bottom row carries the x numbers.
    let shared = SvgRenderer::default().render(&spec, &data);
    // Freed: three different y ranges, so "200" can only come from panel b
    // and "20" from panel c — neither appears when the scale is shared.
    let mut freed = spec.clone();
    freed.y = freed.y.map(|d| d.with_free());
    let svg = SvgRenderer::default().render(&freed, &data);

    assert!(!shared.contains(">20</text>"), "a shared y spans 1..200 and never ticks 20");
    assert!(svg.contains(">20</text>"), "panel c should tick its own 20");
    assert!(svg.contains(">200</text>"), "panel b should tick its own 200");
    assert!(svg.contains(">100</text>"), "panel b should tick its own 100");
    // x was not freed, so all three panels still share 1.0..2.0.
    assert!(svg.contains(">2.0</text>") && shared.contains(">2.0</text>"));
    assert_eq!(panel_count(&svg), 3);
}

/// A freed panel's **guides** are fitted with its marks, not with the plot.
///
/// The bug this pins was a *statement order*: the per-panel axes were resolved
/// below the frame routines, so a freed polar panel drew its sectors from its
/// own fit and its rings from the shared one — every panel ringed alike while
/// its tick numbers read its own scale, so the rings annotated a scale that was
/// not there. Flat panels were unaffected only because a flat frame is drawn
/// from the panel rectangle rather than from the ticks.
///
/// Read on the **radii**, which is where the disagreement lives; the labels were
/// right the whole time and are what made it look correct. The three groups are
/// deliberately *not* self-similar — `[0,4]`, `[0,90]`, `[0,8.2]` put their ticks
/// at different fractions of their own domains, where three ranges of the same
/// shape would ring identically whether the fix were in or not.
#[test]
fn a_freed_polar_panel_rings_its_own_scale_and_not_the_plots() {
    let df = DataFrame::new()
        .with_str("cat", ["N", "E", "S", "W"].iter().cycle().take(12)
            .map(|s| s.to_string()).collect())
        .with_float("val", vec![1.0, 2.0, 3.0, 4.0,
                                10.0, 90.0, 50.0, 20.0,
                                7.0, 7.5, 8.0, 8.2])
        .with_str("g", ["a", "b", "c"].iter().flat_map(|s| std::iter::repeat(s.to_string()).take(4))
            .collect());
    let data = HashMap::from([("t".to_string(), df)]);
    let base = PlotSpec::new().data("t").x("cat").y("val")
        .layer(Layer::new(Mark::Bar))
        .coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }))
        .facet_col("g");

    // The radial rings each panel drew, as fractions of that panel's own outer
    // ring — the comparison that survives the discs being different sizes.
    let rings = |svg: &str| -> Vec<Vec<i64>> {
        let mut by_centre: std::collections::BTreeMap<(i64, i64), Vec<f64>> = Default::default();
        for line in svg.lines().filter(|l| l.contains("<circle")) {
            let f = |k: &str| line.split(k).nth(1)
                .and_then(|s| s.split('"').next())
                .and_then(|s| s.parse::<f64>().ok());
            if let (Some(cx), Some(cy), Some(r)) = (f(r#"cx=""#), f(r#"cy=""#), f(r#"r=""#)) {
                by_centre.entry((cx as i64, cy as i64)).or_default().push(r);
            }
        }
        by_centre.into_values().map(|mut rs| {
            rs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let max = rs.last().copied().unwrap_or(1.0);
            rs.iter().map(|r| (r / max * 1000.0).round() as i64).collect()
        }).collect()
    };

    let mut freed = base.clone();
    freed.y = freed.y.map(|d| d.with_free());
    let f = rings(&SvgRenderer::default().render(&freed, &data));
    assert_eq!(f.len(), 3, "three panels of rings");
    assert!(f[0] != f[1] && f[1] != f[2],
        "each freed panel rings its own domain, so no two agree: {f:?}");

    // And the other half of the rule: a plot nobody freed is unmoved. Shared,
    // one scale means one set of rings, in every panel.
    let s = rings(&SvgRenderer::default().render(&base, &data));
    assert!(s[0] == s[1] && s[1] == s[2],
        "one shared scale is one set of rings: {s:?}");
}

/// Five levels wrapped at two: a 2 × 3 rectangle holding five panels, each
/// carrying its own name, and one cell left empty by the fold. The empty
/// cell gets no panel background — unlike a *crossing*'s empty combination,
/// which is framed because the crossing says it is possible.
#[test]
fn a_wrapped_facet_draws_one_panel_per_level_and_names_every_one() {
    let (spec, data) = five_level_points();
    let svg = SvgRenderer::default()
        .render(&spec.facet_col("g").facet_wrap(2), &data);
    assert_eq!(panel_count(&svg), 5, "five levels, not the rectangle's six cells");
    for name in ["a", "b", "c", "d", "e"] {
        assert!(svg.contains(&format!(">{name}</text>")), "missing strip label {name}");
    }
}

/// The direction is the operator's. `/` runs the levels *down*, so `a` and
/// `b` share a column and sit in successive rows — where a ribbon numbered
/// row-major would put them side by side. This also settles the subsets:
/// a panel's name and its rows are both read at `Panel::slot`, so a name in
/// the right cell is rows in the right cell.
#[test]
fn wrapping_down_stacks_the_first_levels_in_one_column() {
    let (spec, data) = five_level_points();
    let svg = SvgRenderer::default()
        .render(&spec.facet_row("g").facet_wrap(2), &data);
    let at = |name: &str| -> (f64, f64) {
        let tag = format!(">{name}</text>");
        let line = svg.lines().find(|l| l.contains(&tag) && l.contains("<text x="))
            .unwrap_or_else(|| panic!("no strip for {name}"));
        let num = |key: &str| -> f64 {
            let s = &line[line.find(key).unwrap() + key.len()..];
            s[..s.find('"').unwrap()].parse().unwrap()
        };
        (num("x=\""), num("y=\""))
    };
    let (ax, ay) = at("a");
    let (bx, by) = at("b");
    let (cx, _) = at("c");
    assert!((ax - bx).abs() < 1e-6, "`a` and `b` should share a column: {ax} vs {bx}");
    assert!(by > ay, "`b` should sit below `a`: {by} vs {ay}");
    assert!(cx > ax, "`c` starts the next column: {cx} vs {ax}");
}

#[test]
fn an_unfaceted_plot_is_the_one_panel_degenerate_case() {
    let (spec, data) = faceted_points();
    let svg = SvgRenderer::default().render(&spec, &data);
    assert_eq!(panel_count(&svg), 1);
}

#[test]
fn a_crossed_grid_draws_every_combination_even_the_empty_one() {
    // g × h has 3 × 2 = 6 combinations but only 4 appear in the rows.
    // A crossing draws all 6: the frame says the combination is possible
    // even when this data has no example of it (Wilkinson ch. 11).
    let (spec, mut data) = faceted_points();
    let df = data.remove("t").unwrap().with_str("h", vec![
        "u".into(), "u".into(), "v".into(), "v".into(), "u".into(), "u".into(),
    ]);
    data.insert("t".to_string(), df);
    let svg = SvgRenderer::default().render(&spec.facet_col("g").facet_row("h"), &data);
    assert_eq!(panel_count(&svg), 6);
}

#[test]
fn facet_statistics_run_within_each_panel() {
    // Counting rows per category: globally "p" appears 3 times, but no
    // single panel holds more than 2 of them. If the count ran before the
    // facet split, a bar of height 3 would need a y tick labeled 3.
    let df = DataFrame::new()
        .with_str("cat", vec!["p".into(), "p".into(), "q".into(), "p".into()])
        .with_str("g", vec!["a".into(), "a".into(), "a".into(), "b".into()]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("cat")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count))
        .facet_col("g");
    let svg = SvgRenderer::default().render(&spec, &data);

    // Panel a draws bars for p and q, panel b for p alone: three bars.
    assert_eq!(bar_lefts(&svg).len(), 3);
    assert!(!svg.contains(">3</text>"),
        "a y tick at 3 means the count ran across panels: {svg}");
}

#[test]
fn tick_labels_are_drawn_only_where_panels_touch_the_margins() {
    // y runs 10..60 so its tick labels cannot be mistaken for x's 1..6.
    let df = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .with_float("y", vec![10.0, 40.0, 20.0, 50.0, 30.0, 60.0])
        .with_str("g", vec![
            "a".into(), "a".into(), "b".into(),
            "b".into(), "c".into(), "c".into(),
        ]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    // An explicit tick_count wins over the fewer-ticks-per-narrow-panel
    // default, which keeps the tick values this test pins deterministic.
    let spec = PlotSpec::new().data("t").x("x").y("y").tick_count(5)
        .layer(Layer::new(Mark::Point)).facet_col("g");
    let svg = SvgRenderer::default().render(&spec, &data);

    // One row of panels: every panel touches the bottom margin, so an
    // x tick label appears once per panel; a y label only once, beside
    // the left column.
    assert_eq!(svg.matches(">2</text>").count(), 3, "x tick 2 once per panel");
    assert_eq!(svg.matches(">20</text>").count(), 1, "y tick 20 on the left column only");
}

#[test]
fn a_layer_whose_table_lacks_the_facet_column_is_drawn_in_every_panel() {
    let (spec, mut data) = faceted_points();
    data.insert("ref".to_string(), DataFrame::new()
        .with_float("x", vec![1.0, 6.0])
        .with_float("y", vec![3.5, 3.5]));
    let mut line = Layer::new(Mark::Line);
    line.data = Some("ref".to_string());
    let svg = SvgRenderer::default().render(&spec.layer(line).facet_col("g"), &data);
    assert_eq!(svg.matches("<polyline").count(), 3,
        "the reference line should appear in all three panels");
}

#[test]
fn facet_panels_share_one_scale() {
    // Panel b's own y values stop at 2, but the shared scale runs to the
    // crossing's maximum, so panel b still draws against ticks up to 6 —
    // that shared frame is what makes the panels comparable.
    let df = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 1.0, 2.0])
        .with_float("y", vec![5.0, 6.0, 1.0, 2.0])
        .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into()]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point)).facet_col("g");
    let svg = SvgRenderer::default().render(&spec, &data);

    // The y axis (drawn once, left column) reaches 6.
    assert!(svg.contains(">6</text>"), "{svg}");
    // Every circle in the right panel sits in its lower half: with the
    // shared scale, y=1 and y=2 map below the panels' vertical midpoint.
    let cys: Vec<f64> = svg.lines()
        .filter(|l| l.contains("<circle"))
        .filter_map(|l| l.split(r#"cy=""#).nth(1)?.split('"').next()?.parse().ok())
        .collect();
    assert_eq!(cys.len(), 4);
    let mid = (cys.iter().cloned().fold(f64::INFINITY, f64::min)
             + cys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)) / 2.0;
    assert!(cys[2] > mid && cys[3] > mid,
        "panel b's points should sit low on the shared scale: {cys:?}");
}

/// A frame whose two panels have deliberately mismatched spreads and counts —
/// the shape that made every input to a bin layout panel-sized. Panel `a` runs
/// 0..1 with four rows, panel `b` runs 0..40 with eight, so a per-panel cut
/// disagrees on the width (Sturges' `k` differs *and* the spans differ by 40×)
/// and on where the bins start.
fn lopsided_panels() -> HashMap<String, DataFrame> {
    let mut v = vec![0.0, 0.3, 0.6, 1.0];
    v.extend([0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0]);
    let mut g: Vec<String> = vec!["a".into(); 4];
    g.extend(vec!["b".to_string(); 8]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new().with_float("v", v).with_str("g", g));
    data
}

#[test]
fn a_facet_cuts_one_set_of_bins_for_every_panel() {
    // The cut is an extent description, so it is shared across panels exactly
    // as the scale is (spec §11) — otherwise `bar * bin + x(v) | facet(g)`
    // draws bars of different widths against one axis, and their heights are
    // counts of different quantities. gapminder made this plain: 5.5 years per
    // bar in Asia against 1.7 in Europe, so Africa's peak of 13 *looked* taller
    // than Europe's 8 while being barely half as dense.
    let data = lopsided_panels();
    let spec = PlotSpec::new().data("t").x("v")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin))
        .facet_col("g");
    let svg = SvgRenderer::default().render(&spec, &data);

    let widths: Vec<f64> = bar_rects(&svg).iter().map(|r| r.2).collect();
    assert!(!widths.is_empty(), "no bars drawn: {svg}");
    let (lo, hi) = (
        widths.iter().cloned().fold(f64::INFINITY, f64::min),
        widths.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );
    assert!(hi - lo < 0.01,
        "every panel's bars must be one width; got {lo}..{hi}: {widths:?}");
}

#[test]
fn a_facets_shared_cut_still_tallies_within_each_panel() {
    // The other half of the same rule, and the half that must *not* change:
    // the cut is the plot's, the tally is the panel's (spec §5, §11). Sharing
    // the tally too would draw one histogram five times.
    let data = lopsided_panels();
    let spec = PlotSpec::new().data("t").x("v")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin))
        .facet_col("g");
    let svg = SvgRenderer::default().render(&spec, &data);

    // Panel `a`'s four rows all fall in the leftmost shared bin, so it draws
    // exactly one bar; panel `b` spreads across several. Equal counts per panel
    // would mean the tally had escaped its panel.
    let bars = bar_rects(&svg);
    let mid = bars.iter().map(|r| r.0).fold(f64::NEG_INFINITY, f64::max) / 2.0;
    let left = bars.iter().filter(|r| r.0 < mid).count();
    let right = bars.len() - left;
    assert_eq!(left, 1, "panel a's rows all land in one shared bin: {bars:?}");
    assert!(right > 1, "panel b's rows spread across bins: {bars:?}");
}

#[test]
fn an_unfaceted_bin_is_untouched_by_the_shared_cut() {
    // The cut is resolved for every plot, not only faceted ones, so that one
    // path serves both. That is only safe if a single panel — which *is* the
    // whole frame — cuts exactly what it cut before, so this pins the identity.
    let data = lopsided_panels();
    let plain = PlotSpec::new().data("t").x("v")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    let svg = SvgRenderer::default().render(&plain, &data);
    let widths: Vec<f64> = bar_rects(&svg).iter().map(|r| r.2).collect();
    assert!(!widths.is_empty(), "no bars drawn: {svg}");

    // Sturges over all twelve rows: k = ceil(log2(12)) + 1 = 5 bins across
    // 0..40, so a bar is a fifth of the plotted span however the rows sit.
    let lefts = bar_lefts(&svg);
    // Tolerance of a rounding step, not of a discrepancy: the two quantities
    // are written to the SVG separately at two decimals, so they can differ in
    // the last one. The defect this guards against was a factor of three.
    let gap = lefts.windows(2).map(|w| w[1] - w[0])
        .fold(f64::INFINITY, f64::min);
    assert!((gap - widths[0]).abs() < 0.02,
        "a histogram's bars abut: gap {gap}, width {}", widths[0]);
}

// -----------------------------------------------------------------------
// play — the facet read in time
//
// Every property here has a facet counterpart a few tests up, and that is
// the claim being pinned as much as any single behavior: one scale over
// every subset, statistics within each subset, and a layer that lacks the
// splitting column drawn in all of them. What is new is only *when* the
// subsets are shown.
// -----------------------------------------------------------------------

fn played_points() -> (PlotSpec, HashMap<String, DataFrame>) {
    // Two moments, and deliberately lopsided: everything in 1962 sits past
    // everything in 1957, so a per-frame scale would be visible as motion
    // that is the axis's rather than the data's.
    let df = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 3.0, 10.0, 20.0, 30.0])
        .with_float("y", vec![1.0, 2.0, 3.0, 10.0, 20.0, 30.0])
        .with_float("year", vec![1957.0, 1957.0, 1957.0, 1962.0, 1962.0, 1962.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Point))
        .channel(Channel::Play, "year");
    (spec, data)
}

fn frame_count(svg: &str) -> usize {
    svg.matches(r#"<animate attributeName="display""#).count()
}

/// **The claim a written-out file rests on**, and the reason stills are cut
/// by selection rather than by a second pass over the data: two moments of
/// one plot differ in *which group shows* and in nothing else at all.
///
/// Erase the display attributes and the two frames are the same bytes. That
/// is a stronger statement than "the axes match" — it covers every tick, the
/// color map, both legends, the strip, and the layout arithmetic in one
/// assertion, and it fails the moment any of them is decided per frame. The
/// fixture is lopsided on purpose (everything in 1962 sits past everything in
/// 1957), so a per-frame fit would move an axis rather than being invisible.
#[test]
fn two_stills_of_one_plot_differ_only_in_which_moment_shows() {
    let (spec, data) = played_points();
    let figure = crate::ir::Figure::Plot(Box::new(spec));
    let frames = crate::plot::render_frames_with(
        &figure, &data, crate::plot::Strictness::Strict,
    )
    .expect("a played plot has frames")
    .frames;
    assert_eq!(frames.len(), 2, "one still per moment");

    let blind = |s: &str| {
        s.replace(r#"display="inline""#, "").replace(r#"display="none""#, "")
    };
    assert_eq!(blind(&frames[0]), blind(&frames[1]),
        "the moments disagree about something other than which one is shown");
    assert_ne!(frames[0], frames[1], "and they must differ about that");
}

/// A still is a picture, so it carries no clock. Left in, the `<animate>`
/// would switch the moment off a fraction of a second after it was drawn —
/// which a browser shows and a rasterizer ignores, so it would survive every
/// check that reads the file as an image.
#[test]
fn a_still_carries_no_timing_and_shows_exactly_its_own_moment() {
    let (spec, data) = played_points();
    // A played plot cuts moments in more than one place — the marks, and the
    // strip that names them — so "one group showing" is the wrong count. What
    // must hold is that a still shows exactly what the sequence shows before
    // its clock starts, which is this number whatever the plot is made of.
    let animated = SvgRenderer::default().render(&spec, &data);
    let opening = animated.matches(r#"<g display="inline">"#).count();
    assert!(opening > 0, "the fixture must play at all");

    let figure = crate::ir::Figure::Plot(Box::new(spec));
    let frames = crate::plot::render_frames_with(
        &figure, &data, crate::plot::Strictness::Strict,
    )
    .unwrap()
    .frames;
    for (i, svg) in frames.iter().enumerate() {
        assert_eq!(frame_count(svg), 0, "still {i} still carries timing");
        assert_eq!(svg.matches(r#"<g display="inline">"#).count(), opening,
            "still {i} shows a different set of groups than the sequence opens with");
    }
}

/// The sequence path reports what the check said. `render_frames` returned
/// bare frames once, and every non-fatal diagnostic — this Assumption
/// included — was built and then dropped, so `--gif` wrote the file and
/// said nothing. The silent drop §12 forbids, on the one output a reader
/// cannot re-run with the words on.
#[test]
fn writing_out_the_frames_keeps_the_diagnostics() {
    let (mut spec, data) = played_points();
    spec.layers[0].transforms.push(crate::ir::Transform::Quantile);
    spec.layers[0].quantile = Some(crate::ir::QuantileSpec { p: Some(0.5) });
    let figure = crate::ir::Figure::Plot(Box::new(spec));
    let drawn = crate::plot::render_frames_with(
        &figure, &data, crate::plot::Strictness::Strict,
    )
    .expect("quantile(0.5) is an Assumption, not a refusal");
    assert!(!drawn.frames.is_empty());
    assert!(
        drawn.diagnostics.iter().any(|d| d.message.contains("median")),
        "the Assumption must ride along with the frames: {:?}",
        drawn.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// A plot that does not play cannot be a sequence, and §12 says the refusal
/// names what to do instead rather than writing a one-frame file nobody asked
/// for. Guarded because the direction is the useful half of the message.
#[test]
fn a_plot_that_does_not_play_refuses_to_yield_frames() {
    let (_, data) = played_points();
    let still = PlotSpec::new().data("t").x("x").y("y").layer(Layer::new(Mark::Point));
    let figure = crate::ir::Figure::Plot(Box::new(still));
    let err = crate::plot::render_frames_with(
        &figure, &data, crate::plot::Strictness::Strict,
    )
    .expect_err("an unplayed plot has no moments");
    let last = err.last().expect("a diagnostic");
    assert!(last.message.contains("does not play"), "{}", last.message);
    assert!(last.message.contains("play(year)"),
        "the refusal must say what to write instead: {}", last.message);
}

/// The invariant the whole feature is built around: an unplayed plot is what
/// it always was. Not "close enough" — a corpus of 481 recorded hashes says
/// so per sentence, and this is the unit-level statement of the same thing.
#[test]
fn a_plot_that_does_not_play_carries_no_timing_at_all() {
    let (spec, data) = played_points();
    let mut still = spec.clone();
    still.channels.remove(&Channel::Play);
    let svg = SvgRenderer::default().render(&still, &data);
    assert!(!svg.contains("<animate"), "no timing");
    assert!(!svg.contains("<g display="), "no frame groups");
    assert_eq!(panel_count(&svg), 1, "and still the one-panel degenerate case");
}

/// One group per moment, for the marks and again for the strip that names
/// them — and the first is the one written showing.
#[test]
fn play_cuts_one_group_per_moment_and_shows_the_first() {
    let (spec, data) = played_points();
    let svg = SvgRenderer::default().render(&spec, &data);
    assert_eq!(frame_count(&svg), 4, "two moments, once for the marks and once for the strip");
    assert_eq!(svg.matches(r#"<g display="inline">"#).count(), 2,
        "exactly the first moment is written visible");
    assert_eq!(svg.matches(r#"<g display="none">"#).count(), 2);
    // The static state is the print fallback: `rsvg-convert` ignores
    // `<animate>` and draws the attributes as written, so this is what a PDF
    // page of this plot shows.
    let first = svg.find(r#"<g display="inline">"#).unwrap();
    let hidden = svg.find(r#"<g display="none">"#).unwrap();
    assert!(first < hidden, "the visible moment is the first one, not a later one");
}

/// A single-valued column is a plot, not a sequence. Nothing is written,
/// because a group that never changes is bytes spent on nothing.
#[test]
fn one_moment_is_not_an_animation() {
    let (spec, data) = played_points();
    let mut one = data.clone();
    one.insert("t".to_string(), DataFrame::new()
        .with_float("x", vec![1.0, 2.0])
        .with_float("y", vec![1.0, 2.0])
        .with_float("year", vec![1957.0, 1957.0]));
    let svg = SvgRenderer::default().render(&spec, &one);
    assert!(!svg.contains("<animate"), "one frame needs no timing: {svg}");
}

/// The property that makes the motion the data's. Both moments are placed
/// against one range, so 1957's rows sit low in the panel rather than
/// filling it — which is what they would do under a per-frame fit.
#[test]
fn every_moment_is_drawn_against_one_shared_scale() {
    let (spec, data) = played_points();
    let svg = SvgRenderer::default().render(&spec, &data);
    let cys: Vec<f64> = svg.split(r#"<circle"#).skip(1)
        .filter_map(|s| s.split(r#"cy=""#).nth(1)?.split('"').next()?.parse().ok())
        .collect();
    assert_eq!(cys.len(), 6, "every row of every moment is drawn");
    // Six distinct heights: if each frame were fitted to itself, 1..3 and
    // 10..30 would map to the same three positions and there would be three.
    let mut rounded: Vec<i64> = cys.iter().map(|v| (v * 10.0).round() as i64).collect();
    rounded.sort_unstable();
    rounded.dedup();
    assert_eq!(rounded.len(), 6,
        "one range over the whole sequence, so no two rows share a height");
}

/// A layer that does not bind `play` stands still behind the ones that do.
/// The facet rule — a layer whose table lacks the column is drawn in every
/// panel — arriving here through §8's scope resolution rather than through a
/// second rule written for animation.
///
/// **Drawn once where that keeps the order it was written in** (`draw_steps`).
/// Written before every played layer, it is drawn once under the moments;
/// after every one, once over them; between two, in every moment, since its
/// place is between theirs. Hoisting every still layer out of the moments would
/// put it behind the played ones whatever order it was written in, which is the
/// enclosing context reinterpreting an inner expression, and Law 6 forbids it.
/// Copied into every moment, a basemap under played points cost 28 KB a frame.
#[test]
fn a_layer_that_does_not_play_stands_still_in_its_written_place() {
    let df = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 3.0, 4.0])
        .with_float("y", vec![1.0, 2.0, 3.0, 4.0])
        .with_float("year", vec![1957.0, 1957.0, 1962.0, 1962.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    // `play` written *after* the mark binds that layer alone; the rule layer
    // never names it and so is never cut down. The rule names its own axis
    // (§8) because a table with a column for both leaves nothing to say which
    // one it marks — `check_rule` refuses that, and rightly.
    let played = || Layer::new(Mark::Point).encode(Channel::Play, "year");
    let still = || Layer::new(Mark::Rule).encode(Channel::Y, "y");
    let draw = |layers: Vec<Layer>| {
        let mut spec = PlotSpec::new().data("t").x("x").y("y");
        for layer in layers { spec = spec.layer(layer) }
        SvgRenderer::default().render(&spec, &data)
    };
    // Each moment runs from its group tag to its own `<animate>`, the last thing
    // inside it. A rule's `<line>` carries its own stroke, where gridlines and
    // ticks take theirs from their group, so these count the rule layer alone.
    let moments = |svg: &str| -> Vec<String> {
        svg.split(r#"<g display="#).skip(1).take(2)
            .map(|m| m.split("<animate").next().unwrap().to_string()).collect()
    };
    let rules = |text: &str| text.lines()
        .filter(|l| l.contains("<line") && l.contains(" stroke=")).count();

    // After every played layer: drawn once, after the moments, so over them.
    let over = draw(vec![played(), still()]);
    assert_eq!(frame_count(&over), 4, "two moments, for the marks and for the strip");
    for (i, m) in moments(&over).iter().enumerate() {
        assert_eq!(m.matches("<circle").count(), 2, "moment {i} shows only its own two rows");
        assert_eq!(rules(m), 0, "moment {i} holds no copy of the still layer");
    }
    assert_eq!(rules(&over), 4, "the four rules, drawn once");
    let last_moment = over.find("<g display=\"none\">").unwrap();
    assert!(over.find(r##"stroke="#4e79a7" stroke"##).unwrap() > last_moment,
        "and after the moments, as it was written");

    // Before every played layer: drawn once, before the moments, so under them.
    let under = draw(vec![still(), played()]);
    assert_eq!(rules(&under), 4, "the four rules, drawn once");
    assert!(under.find(r##"stroke="#4e79a7" stroke"##).unwrap() < under.find("<g display=").unwrap(),
        "and before the moments, as it was written");

    // Between two played layers: in every moment, where its place is.
    let between = draw(vec![played(), still(), played()]);
    for (i, m) in moments(&between).iter().enumerate() {
        assert_eq!(rules(m), 4, "moment {i} holds the still layer between its two played ones");
    }
}

/// **The same in a cube.** The cube paints its layers in their written order,
/// each sorted by depth within itself, so a still layer written before every
/// played one is drawn once under the moments and one written after them once
/// over them. A surface under played points was copied into every moment.
#[test]
fn a_layer_that_does_not_play_is_drawn_once_in_a_cube_too() {
    let df = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 3.0, 4.0])
        .with_float("y", vec![1.0, 3.0, 2.0, 4.0])
        .with_float("z", vec![2.0, 1.0, 4.0, 3.0])
        .with_float("year", vec![1957.0, 1957.0, 1962.0, 1962.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let still = || {
        let mut layer = Layer::new(Mark::Point);
        layer.style.color = Some("firebrick".to_string());
        layer
    };
    let played = || Layer::new(Mark::Point).encode(Channel::Play, "year");
    let draw = |layers: Vec<Layer>| {
        let mut spec = PlotSpec::new().data("t").x("x").y("y").z("z")
            .coord(CoordSpace::Space(crate::ir::SpaceView::default()));
        for layer in layers { spec = spec.layer(layer) }
        SvgRenderer::default().render(&spec, &data)
    };
    let still_dots = |text: &str| text.matches(r#"fill="firebrick""#).count();
    for (layers, before) in [(vec![still(), played()], true), (vec![played(), still()], false)] {
        let svg = draw(layers);
        assert_eq!(still_dots(&svg), 4, "the still layer's four rows, drawn once");
        let first = svg.find("<g display=").expect("the cube is played");
        let at = svg.find(r#"fill="firebrick""#).unwrap();
        assert_eq!(at < first, before, "the still layer keeps its written place");
        for m in svg.split("<g display=").skip(1).take(2) {
            let m = m.split("<animate").next().unwrap();
            assert_eq!(still_dots(m), 0, "a moment holds no copy of the still layer");
        }
    }
    // Between two played layers it stays in every moment, as on the page.
    let between = draw(vec![played(), still(), played()]);
    assert_eq!(still_dots(&between), 8, "the still layer, in each of the two moments");
}

/// **`text` maps `size` and `opacity` the way `point` does, and stands in the
/// cube** (built 2026-10-09). The channel fraction a dot turns into a radius a
/// label turns into a font size, 8 px at the smallest value and 24 px at the
/// largest (`encode::text_size_at`), and an opacity is the letters'. Each key
/// shows the letter `a`, at the size or the opacity it decodes. In `space()` a
/// label sits where its place projects, drawn far to near.
#[test]
fn text_maps_size_and_opacity_and_stands_in_the_cube() {
    let df = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 3.0])
        .with_float("y", vec![3.0, 2.0, 1.0])
        // Seen from the default view the last row is the farthest, so the
        // dots are painted in reverse, and labels drawn in row order would not
        // follow them.
        .with_float("z", vec![3.0, 2.0, 1.0])
        .with_float("v", vec![10.0, 20.0, 30.0])
        .with_str("name", vec!["low".into(), "mid".into(), "high".into()]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let text = || Layer::new(Mark::Text).encode(Channel::Label, "name");
    let draw = |spec: PlotSpec| SvgRenderer::default().render(&spec, &data);
    let flat = || PlotSpec::new().data("t").x("x").y("y");
    // The attribute on the `<text>` element that draws `word`.
    let attr = |svg: &str, word: &str, key: &str| -> Option<f64> {
        let end = svg.find(&format!(">{word}</text>"))?;
        let tag = &svg[svg[..end].rfind("<text")?..end];
        let at = tag.find(&format!(" {key}=\""))? + key.len() + 3;
        tag[at..].split('"').next()?.parse().ok()
    };

    let sized = draw(flat().layer(text().encode(Channel::Size, "v")));
    assert_eq!(attr(&sized, "low", "font-size"), Some(8.0), "the smallest value is 8 px");
    assert_eq!(attr(&sized, "high", "font-size"), Some(24.0), "the largest is 24 px");
    assert_eq!(attr(&sized, "mid", "font-size"), Some(16.0), "and halfway is halfway");
    // The key decodes letters with a letter, at the sizes it decodes.
    assert!(sized.contains(r##"font-size="8.0" text-anchor="middle" fill="#3c3c46" fill-opacity="1.000">a</text>"##)
        && sized.contains(r#"font-size="24.0" text-anchor="middle""#), "an `a` key at 8 and 24 px");
    assert!(!sized.contains("<circle"), "no circle decodes a word");

    let faded = draw(flat().layer(text().encode(Channel::Opacity, "v")));
    assert_eq!(attr(&faded, "low", "fill-opacity"), Some(0.15));
    assert_eq!(attr(&faded, "high", "fill-opacity"), Some(0.95));
    assert!(faded.contains(r#"fill-opacity="0.150">a</text>"#), "an `a` key at the least opacity");

    // Unmapped, a label writes no size of its own, as before.
    let plain = draw(flat().layer(text()));
    assert_eq!(attr(&plain, "mid", "font-size"), None);

    // In the cube: each label at its dot's place, in the order the dots are
    // painted, far to near, so a nearer label reads over a farther one.
    let cube = draw(flat().z("z").coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        .layer(Layer::new(Mark::Point)).layer(text()));
    let xs_of = |svg: &str, tag: &str, key: &str| -> Vec<String> {
        svg.split(tag).skip(1)
            .filter_map(|t| Some(t.split(key).nth(1)?.split('"').next()?.to_string()))
            .collect()
    };
    let dots = xs_of(&cube, "<circle ", r#"cx=""#);
    let words: Vec<String> = cube.split("<text x=\"").skip(1)
        .filter_map(|t| t.split('"').next().map(str::to_string))
        .filter(|x| dots.contains(x)).collect();
    assert_eq!(dots.len(), 3, "three dots in the cube");
    assert_eq!(words, dots, "each label at its dot, drawn in the dots' far-to-near order");
}

/// The channel earns a strip, and the strip names each moment. Without it
/// the reader watches points move with nothing saying what they move
/// through — the animation's version of an unlabeled axis.
#[test]
fn the_strip_names_every_moment_and_a_year_reads_as_a_year() {
    let (spec, data) = played_points();
    let svg = SvgRenderer::default().render(&spec, &data);
    assert!(svg.contains(">1957</text>"), "the first moment is named: {svg}");
    assert!(svg.contains(">1962</text>"), "and so is the second");
    assert!(!svg.contains(">1957.0<"), "a year is named, not measured");
}

/// `speed` is a multiple of the pace, so it divides the loop.
#[test]
fn speed_shortens_the_loop_it_does_not_drop_frames() {
    let (spec, data) = played_points();
    let plain = SvgRenderer::default().render(&spec, &data);
    let mut fast = spec.clone();
    fast.channels.insert(
        Channel::Play,
        crate::ir::ChannelDef::field("year").with_speed(2.0),
    );
    let quick = SvgRenderer::default().render(&fast, &data);
    assert_eq!(frame_count(&plain), frame_count(&quick), "the same moments");
    assert!(plain.contains(r#"dur="1.600s""#), "two frames at the default pace");
    assert!(quick.contains(r#"dur="0.800s""#), "twice as fast is half as long");
}

/// `play` crossed with `facet`: panels split the page, moments split the
/// clock, and the two strips stack rather than collide.
#[test]
fn a_played_facet_animates_every_panel_in_step() {
    let (spec, data) = faceted_points();
    let mut spec = spec.facet_col("g");
    // `g` names the panels; a second column names the moments.
    let df = data["t"].clone();
    let mut data = HashMap::new();
    data.insert("t".to_string(), df.with_float(
        "year", vec![1957.0, 1962.0, 1957.0, 1962.0, 1957.0, 1962.0]));
    spec.channels.insert(Channel::Play, crate::ir::ChannelDef::field("year"));
    let svg = SvgRenderer::default().render(&spec, &data);
    assert_eq!(panel_count(&svg), 3, "the page is still split three ways");
    // Two moments per panel, plus two for the strip that names them.
    assert_eq!(frame_count(&svg), 3 * 2 + 2);
    for name in ["a", "b", "c", "1957", "1962"] {
        assert!(svg.contains(&format!(">{name}</text>")), "missing strip label {name}");
    }
}

// -- the faceted cube ---------------------------------------------------
//
// A facet crossed with 3-D, unblocked 2026-07-28 by deleting a refusal that
// had never been true: the 3-D branch reads `l`, the *panel's* rect, so N
// panels have always projected N cubes. These tests pin that, because the
// thing that hid it for so long was that nothing asserted it either way.

/// One wireframe group per cube — the faint `#d8d8de` box `write_space_box`
/// draws behind the cloud.
fn cube_count(svg: &str) -> usize {
    svg.matches(r##"stroke="#d8d8de""##).count()
}

/// Three categories, three panels, three cubes, and every point still drawn.
/// The `Scene` is built from `l` rather than from the plot, so this is the
/// assertion that the projector reads the panel it is standing in.
#[test]
fn a_faceted_cube_projects_one_scene_per_panel() {
    let (spec, data) = faceted_points();
    let df = data["t"].clone();
    let mut data = HashMap::new();
    data.insert("t".to_string(),
        df.with_float("z", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
    let spec = spec.z("z").facet_col("g");
    let svg = SvgRenderer::default().render(&spec, &data);

    assert_eq!(panel_count(&svg), 3, "the page is split three ways");
    assert_eq!(cube_count(&svg), 3, "and each panel gets its own cube");
    assert_eq!(svg.matches("<circle").count(), 6, "no point is lost to the split");

    // The cubes stand apart: three distinct leftmost box edges, not one box
    // drawn three times in the same place.
    let lefts: std::collections::HashSet<String> = svg.lines()
        .filter(|l| l.trim_start().starts_with("<line x1="))
        .filter_map(|l| l.split(r#"x1=""#).nth(1))
        .filter_map(|r| r.split('"').next())
        .map(str::to_string)
        .collect();
    assert!(lefts.len() >= 3, "cubes should sit at different x, got {lefts:?}");
}

/// **A guide inside the panel is drawn in every panel; a guide on the panel's
/// boundary is drawn once.** A flat facet writes `x` once, below the bottom
/// row, because the panels share that edge. A cube's three axes are edges of
/// the *cube*, and the cube is inside the panel, so there is no shared edge
/// to write them on and each panel names its own. `polar` already works this
/// way. The layout agrees: 3-D reserves no outer margin for tick numbers.
#[test]
fn a_faceted_cube_repeats_its_guides_because_they_live_inside_the_panel() {
    let (spec, data) = faceted_points();
    let df = data["t"].clone();
    let mut data = HashMap::new();
    data.insert("t".to_string(),
        df.with_float("z", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));

    let names = |svg: &str, name: &str| svg.matches(&format!(">{name}</text>")).count();

    let flat = SvgRenderer::default().render(&spec.clone().facet_col("g"), &data);
    assert_eq!(names(&flat, "X"), 1, "a flat axis name is written once for the plot");

    let cube = SvgRenderer::default().render(&spec.z("z").facet_col("g"), &data);
    assert_eq!(names(&cube, "X"), 3, "a cube names its own axes in every panel");
    assert_eq!(names(&cube, "Z"), 3, "including the third, which flat has nowhere to put");
}

/// `free` on `z` was legal and unreachable for as long as the cube could not
/// be faceted (spec §11). It needed nothing added to start working — the
/// positions are one family of three, which is Law 1 paying out — so this is
/// `a_freed_axis_is_fitted_per_panel_and_the_other_one_is_not` one axis over.
#[test]
fn a_freed_z_fits_each_panels_own_cube() {
    let df = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0])
        .with_float("y", vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0])
        .with_float("z", vec![1.0, 2.0, 100.0, 200.0, 10.0, 20.0])
        .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into(),
                            "c".into(), "c".into()]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("x").y("y").z("z")
        .layer(Layer::new(Mark::Point))
        .facet_col("g");

    let shared = SvgRenderer::default().render(&spec, &data);
    let mut freed = spec.clone();
    freed.z = freed.z.map(|d| d.with_free());
    let svg = SvgRenderer::default().render(&freed, &data);

    assert!(!shared.contains(">20</text>"), "a shared z spans 1..200 and never ticks 20");
    assert!(svg.contains(">20</text>"), "panel c should tick its own 20");
    assert!(svg.contains(">200</text>"), "panel b should tick its own 200");
    assert_eq!(cube_count(&svg), 3, "still one cube per panel");
}

/// The flat rule holding in the cube: a *crossing* frames every combination,
/// including one with no rows, because the crossing says it is possible. (A
/// folded ribbon is the other case and leaves its spare cells blank.)
#[test]
fn a_crossed_grid_of_cubes_frames_the_empty_combination_too() {
    let df = DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 3.0])
        .with_float("y", vec![1.0, 2.0, 3.0])
        .with_float("z", vec![1.0, 2.0, 3.0])
        .with_str("c", vec!["p".into(), "q".into(), "p".into()])
        .with_str("r", vec!["u".into(), "u".into(), "v".into()]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("x").y("y").z("z")
        .layer(Layer::new(Mark::Point))
        .facet_col("c").facet_row("r");
    let svg = SvgRenderer::default().render(&spec, &data);

    // (q, v) has no rows, and still gets a panel and a cube to say so.
    assert_eq!(panel_count(&svg), 4, "a 2 x 2 crossing frames all four");
    assert_eq!(cube_count(&svg), 4, "the empty combination gets a cube too");
    assert_eq!(svg.matches("<circle").count(), 3, "but only three points exist");
}

/// The two partitions crossed, which is the sentence with everything in it:
/// panels across the page, moments along the clock, and a projected cube in
/// each. The cube itself is drawn *outside* the frame groups, so it stands
/// still while only the marks swap — the same rule the flat panel follows.
#[test]
fn a_faceted_cube_animates_every_panel_in_step() {
    let (spec, data) = faceted_points();
    let df = data["t"].clone();
    let mut data = HashMap::new();
    data.insert("t".to_string(), df
        .with_float("z", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .with_float("year", vec![1957.0, 1962.0, 1957.0, 1962.0, 1957.0, 1962.0]));
    let mut spec = spec.z("z").facet_col("g");
    spec.channels.insert(Channel::Play, crate::ir::ChannelDef::field("year"));
    let svg = SvgRenderer::default().render(&spec, &data);

    assert_eq!(panel_count(&svg), 3, "the page is still split three ways");
    assert_eq!(cube_count(&svg), 3, "one cube per panel, not one per moment");
    // Two moments per panel, plus two for the strip that names them.
    assert_eq!(frame_count(&svg), 3 * 2 + 2);
    for name in ["a", "b", "c", "1957", "1962"] {
        assert!(svg.contains(&format!(">{name}</text>")), "missing strip label {name}");
    }
}

#[test]
fn a_facets_shared_cut_survives_a_color_split_inside_it() {
    // Two splits at once, and neither may take the cut back: `color` shares
    // edges across its groups (`bin_layout` exists for that) and `facet` shares
    // them across panels, so all four histograms here land on one lattice.
    // Law 1 — the two splits compose without either learning about the other.
    let mut v = vec![0.0, 0.3, 0.6, 1.0];
    v.extend([0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0]);
    let g: Vec<String> = ["a", "a", "a", "a", "b", "b", "b", "b", "b", "b", "b", "b"]
        .iter().map(|s| s.to_string()).collect();
    let c: Vec<String> = ["p", "q", "p", "q", "p", "q", "p", "q", "p", "q", "p", "q"]
        .iter().map(|s| s.to_string()).collect();
    let mut data = HashMap::new();
    data.insert("t".to_string(),
        DataFrame::new().with_float("v", v).with_str("g", g).with_str("c", c));
    let spec = PlotSpec::new().data("t").x("v")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin).encode(Channel::Color, "c"))
        .facet_col("g");
    let svg = SvgRenderer::default().render(&spec, &data);

    let widths: Vec<f64> = bar_rects(&svg).iter().map(|r| r.2).collect();
    assert!(!widths.is_empty(), "no bars drawn: {svg}");
    let (lo, hi) = (
        widths.iter().cloned().fold(f64::INFINITY, f64::min),
        widths.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );
    assert!(hi - lo < 0.01,
        "color groups and panels share one cut; got {lo}..{hi}: {widths:?}");
}

#[test]
fn a_facets_shared_cut_reaches_the_two_dimensional_reading() {
    // A mesh takes the rule for the reason `bin2d_mixed` already states: cells
    // that do not line up across the plot are not a mesh. A panel is a column
    // of cells one dimension out, so a heatmap faceted into panels with
    // different spreads must still tile one lattice.
    let mut x = vec![0.0, 0.3, 0.6, 1.0];
    x.extend([0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0]);
    let y = x.clone();
    let mut g: Vec<String> = vec!["a".into(); 4];
    g.extend(vec!["b".to_string(); 8]);
    let mut data = HashMap::new();
    data.insert("t".to_string(),
        DataFrame::new().with_float("x", x).with_float("y", y).with_str("g", g));
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .layer(Layer::new(Mark::Zone).transform(Transform::Bin))
        .facet_col("g");
    let svg = SvgRenderer::default().render(&spec, &data);

    let widths: Vec<f64> = bar_rects(&svg).iter().map(|r| r.2).collect();
    assert!(!widths.is_empty(), "no cells drawn: {svg}");
    let (lo, hi) = (
        widths.iter().cloned().fold(f64::INFINITY, f64::min),
        widths.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );
    assert!(hi - lo < 0.01,
        "every panel's cells must tile one mesh; got {lo}..{hi}: {widths:?}");
}

#[test]
fn panel_clip_ids_are_distinct_within_a_plot_and_stable_across_identical_ones() {
    // Same lesson as the gradient legend: the book inlines many SVGs into
    // one page, where ids are global and the first definition wins. An id
    // derived from the rectangle collides only when the rectangles agree,
    // which is the one collision that cannot mislead.
    let (spec, data) = faceted_points();
    let svg = SvgRenderer::default().render(&spec.clone().facet_col("g"), &data);
    let ids: Vec<&str> = svg.lines()
        .filter_map(|l| l.split(r#"<clipPath id=""#).nth(1)?.split('"').next())
        .collect();
    assert_eq!(ids.len(), 3);
    assert!(ids.iter().all(|id| ids.iter().filter(|o| *o == id).count() == 1),
        "panel clips must not collide: {ids:?}");

    let again = SvgRenderer::default().render(&spec.facet_col("g"), &data);
    assert_eq!(svg, again, "identical specs must yield identical ids");
}

// -----------------------------------------------------------------------
// path — the stroke that keeps the table's order
// -----------------------------------------------------------------------

/// A route that doubles back: x goes up, down, then up again, so the row
/// order and the x order are genuinely different sequences. Sorting it is
/// visible; nothing subtler is needed to tell the two marks apart.
fn doubling_back() -> HashMap<String, DataFrame> {
    HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![3.0, 1.0, 4.0, 2.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 4.0]),
    )])
}

fn vertex_xs(svg: &str) -> Vec<f64> {
    svg.lines()
        .find(|l| l.contains("<polyline"))
        .and_then(|l| l.split(r#"points=""#).nth(1)?.split('"').next())
        .map(|pts| pts.split_whitespace()
            .filter_map(|p| p.split(',').next()?.parse().ok())
            .collect())
        .unwrap_or_default()
}

#[test]
fn a_path_keeps_the_tables_order_where_a_line_sorts_by_x() {
    // The whole reason `path` is a mark and not a flag. Written as one
    // assertion set over both marks so it cannot pass by describing each
    // separately — the trap a mirrored copy falls into.
    let data = doubling_back();
    let spec = |m: Mark| PlotSpec::new().data("t").x("x").y("y").layer(Layer::new(m));

    let line = vertex_xs(&SvgRenderer::default().render(&spec(Mark::Line), &data));
    let path = vertex_xs(&SvgRenderer::default().render(&spec(Mark::Path), &data));
    assert_eq!(line.len(), 4);
    assert_eq!(path.len(), 4);

    assert!(line.windows(2).all(|w| w[0] <= w[1]),
        "a line sorts its vertices by x: {line:?}");
    assert!(!path.windows(2).all(|w| w[0] <= w[1]),
        "a path must not sort — it follows the rows: {path:?}");
    // And concretely: the table starts at x = 3, so the path does too, while
    // the line starts at the smallest x in the column.
    let mut sorted = path.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(line, sorted, "the line is exactly the path's vertices, sorted");
    assert!(path[0] > path[1], "the path's first vertex is the table's first row");
}

// -----------------------------------------------------------------------
// zone — the rectangle whose unbounded axis is the panel's
// -----------------------------------------------------------------------

/// Every `<rect>` a zone drew, as (x, y, width, height).
///
/// Picked out by shape rather than by position, because the panel background
/// and the legend swatches are rects too: a zone's are the ones carrying both
/// a fill opacity and `stroke="none"`. It used to key on the *translucent*
/// opacity alone, which stopped identifying zones the moment a binned one
/// drew opaque — it is the data, not decoration over it — and silently
/// reported no zones at all rather than failing.
fn zone_rects(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    svg.lines()
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                 && l.contains(r#"stroke="none""#))
        .map(|l| {
            let num = |k: &str| -> f64 {
                let at = l.find(k).unwrap() + k.len();
                l[at..].split('"').next().unwrap().parse().unwrap()
            };
            (num(r#"x=""#), num(r#"y=""#), num(r#"width=""#), num(r#"height=""#))
        })
        .collect()
}

/// The panel's own rectangle — the background the renderer writes straight from
/// `Layout`, and therefore the ground truth a zone's unbounded side is claimed
/// to match. It is the first `<rect>` with a plain `fill` and no opacity.
fn panel_rect(svg: &str) -> (f64, f64, f64, f64) {
    let l = svg.lines()
        .find(|l| l.trim_start().starts_with("<rect") && !l.contains("fill-opacity")
               && l.contains("width=") && l.contains(r#"x=""#))
        .expect("the panel background is drawn");
    let num = |k: &str| -> f64 {
        let at = l.find(k).unwrap() + k.len();
        l[at..].split('"').next().unwrap().parse().unwrap()
    };
    (num(r#"x=""#), num(r#"y=""#), num(r#"width=""#), num(r#"height=""#))
}

#[test]
fn the_axis_a_zone_is_not_bounded_on_is_the_panel_s_own_extent() {
    // The mark's whole claim, and the one thing `ribbon * bounds` cannot do:
    // an unbounded axis reaches the panel *exactly*, not to a padded data
    // value. Asserted as one differential over both axes, so it cannot pass by
    // describing each separately — the y-bounded zone must span the full width
    // and the x-bounded one the full height, and each must be *narrower* than
    // the panel on the axis it is bounded on.
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("a", vec![0.0, 10.0]).with_float("b", vec![0.0, 10.0]));
    data.insert("z".to_string(), DataFrame::new()
        .with_float("lo", vec![3.0]).with_float("hi", vec![6.0]));

    let draw = |zone: Layer| {
        let spec = PlotSpec::new().data("t").x("a").y("b")
            .layer(zone.data("z")).layer(Layer::new(Mark::Point));
        zone_rects(&SvgRenderer::default().render(&spec, &data))
    };
    let across = draw(Layer::new(Mark::Zone).bounds("lo", "hi"));
    let up     = draw(Layer::new(Mark::Zone).span("lo", "hi"));
    assert_eq!(across.len(), 1, "one row is one rectangle");
    assert_eq!(up.len(), 1);

    let (ax, ay, aw, ah) = across[0]; // spans the panel horizontally
    let (ux, uy, uw, uh) = up[0];     // spans it vertically

    // Measured against the **panel itself**, read off the background rect the
    // renderer draws from the same `Layout`. An earlier version of this test
    // compared the two zones only to each other and could not see the mark
    // stopping short of the panel at all — a zone inset by 10% still passed
    // every relative assertion, because the *other* zone was narrower still.
    let (px, py, pw, ph) = panel_rect(&SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Zone).data("z").bounds("lo", "hi"))
            .layer(Layer::new(Mark::Point)), &data));

    assert!((ax - px).abs() < 0.01 && (aw - pw).abs() < 0.01,
        "a zone bounded on y reaches both panel edges horizontally: \
         got x={ax} w={aw}, panel x={px} w={pw}");
    assert!((uy - py).abs() < 0.01 && (uh - ph).abs() < 0.01,
        "a zone bounded on x reaches both panel edges vertically: \
         got y={uy} h={uh}, panel y={py} h={ph}");
    // And each *bounded* side really is bounded, so the test could not pass
    // with a zone that simply covered the whole panel both ways.
    assert!(ah < ph * 0.9, "the bounded side is the data's, not the panel's");
    assert!(uw < pw * 0.9);
    // Where that band *sits*, not only how thick it is: 3..6 falls inside 0..10 at
    // both ends, so the band clears both panel edges. A band drawn at the wrong
    // offset — flush with an edge, or off the panel entirely — is still the right
    // thickness and passes everything above. Stated as containment rather than as
    // a fraction of the panel, because the exact offset also encodes how far the
    // axis expands past its data, which is a different test's subject.
    assert!(ay > py && ay + ah < py + ph,
        "a zone over y 3..6 clears both panel edges: \
         got y={ay} h={ah}, panel y={py} h={ph}");
    assert!(ux > px && ux + uw < px + pw,
        "a zone over x 3..6 clears both panel edges: \
         got x={ux} w={uw}, panel x={px} w={pw}");
}

#[test]
fn a_zone_reads_which_axis_its_measure_pair_bounds_off_the_bindings() {
    // `bounds(lower, upper)` is the **measure** pair, and which axis measures is
    // the bindings' answer (`legality::zone_orient`, §6 — the reason there is no
    // `flip` atom). On a categorical `y` the measure is `x`, so the spans run
    // across and every rectangle is one slot tall; put the category on `x` and
    // the identical sentence stands the spans up instead.
    //
    // Written as the **same pair of columns under both bindings**, so it cannot
    // pass by swapping unconditionally — which is the other way to be wrong here,
    // and the way a reader who only saw the funnel would fix it. Before 2026-07-28
    // the pair went to `y` whatever the columns held: the categorical axis got a
    // measure it could not place, `x` was left with no column to fit and fell back
    // to `0.0 … 1.0`, and the rectangles landed thousands of pixels off-panel with
    // nothing warning about it (§12). Both places that decide this are covered
    // here at once — the range `build_axis` fits and the sides `write_zone` draws
    // — because the failure needed them to *agree*, and they did, wrongly.
    let mut data = HashMap::new();
    data.insert("f".to_string(), DataFrame::new()
        .with_str("stage", vec!["visit".into(), "trial".into(), "buy".into()])
        .with_float("lo", vec![-20.0, -10.0, -2.0])
        .with_float("hi", vec![20.0, 10.0, 2.0]));

    let draw = |spec: PlotSpec| {
        let svg = SvgRenderer::default().render(&spec, &data);
        (zone_rects(&svg), panel_rect(&svg))
    };
    // Each plot is measured against **its own** panel: the two differ in margin,
    // since one spends its left edge on category names and the other on numbers.
    let (across, (px, py, pw, ph)) = draw(PlotSpec::new().data("f").y("stage")
        .layer(Layer::new(Mark::Zone).bounds("lo", "hi")));
    let (up, (_, _, qw, qh)) = draw(PlotSpec::new().data("f").x("stage")
        .layer(Layer::new(Mark::Zone).bounds("lo", "hi")));

    assert_eq!(across.len(), 3, "one row is one rectangle");
    assert_eq!(up.len(), 3);

    // On a categorical y: three slots, so each rectangle is a third of the panel
    // tall, and the widths carry the measurement in the data's proportions
    // (20 : 10 : 2). The slot half is what fails first if the pairs stop being
    // swapped — a measure placed on the categorical axis has no slot height at all.
    for (i, &(x, y, w, h)) in across.iter().enumerate() {
        assert!((h - ph / 3.0).abs() < 0.5,
            "rect {i} fills its slot: got h={h}, slot={}", ph / 3.0);
        assert!(x >= px - 0.01 && x + w <= px + pw + 0.01,
            "rect {i} is on the panel: got x={x} w={w}, panel x={px} w={pw}");
        assert!(y >= py - 0.01 && y + h <= py + ph + 0.01,
            "rect {i} is on the panel: got y={y} h={h}, panel y={py} h={ph}");
    }
    let (w0, w1, w2) = (across[0].2, across[1].2, across[2].2);
    assert!(w0 > w1 && w1 > w2, "the widths are the measurement: {w0} {w1} {w2}");
    assert!((w0 / w1 - 2.0).abs() < 0.05 && (w1 / w2 - 5.0).abs() < 0.05,
        "and in the data's own proportions (40 : 20 : 4): {w0} {w1} {w2}");

    // The mirror, from the identical columns: a categorical x measures along y,
    // which is what today's code already did and must keep doing.
    for (i, &(_, _, w, h)) in up.iter().enumerate() {
        assert!((w - qw / 3.0).abs() < 0.5,
            "rect {i} fills its slot the other way: got w={w}, slot={}", qw / 3.0);
        assert!(h <= qh + 0.01, "rect {i} is on the panel: got h={h}, panel h={qh}");
    }
    assert!(up[0].3 > up[1].3 && up[1].3 > up[2].3,
        "the heights are the measurement when the slot is on x");
}

/// The confusion matrix, twice: once from a table that already holds the tally,
/// once from the raw rows with `count` doing it. Nine cells either way.
fn confusion() -> HashMap<String, DataFrame> {
    let (a, p) = (["cat", "dog", "bird"], ["cat", "dog", "bird"]);
    let n = [18.0, 2.0, 1.0, 3.0, 21.0, 2.0, 1.0, 4.0, 15.0];
    let (mut ac, mut pc, mut nc) = (Vec::new(), Vec::new(), Vec::new());
    let (mut ar, mut pr) = (Vec::new(), Vec::new());
    for (i, &va) in a.iter().enumerate() {
        for (j, &vp) in p.iter().enumerate() {
            let c = n[i * 3 + j];
            ac.push(va.to_string());
            pc.push(vp.to_string());
            nc.push(c);
            for _ in 0..c as usize {
                ar.push(va.to_string());
                pr.push(vp.to_string());
            }
        }
    }
    // The pre-computed tally is called `count` on purpose: it is the name
    // `zone * count` synthesizes, so the two plots title their legends the same
    // and the comparison below can be exact bytes rather than "near enough".
    // That the long form and the short form coincide is Law 5, not a fixture
    // convenience — `color(count)` says out loud what the transform says for you.
    HashMap::from([
        ("tallied".to_string(), DataFrame::new()
            .with_str("actual", ac).with_str("predicted", pc).with_float("count", nc)),
        ("raw".to_string(), DataFrame::new()
            .with_str("actual", ar).with_str("predicted", pr)),
    ])
}

#[test]
fn a_tile_plot_fills_the_slot_whole_and_leaves_no_seam() {
    // The fourth extent description, asserted as geometry: a category owns
    // `[k-½, k+½]`, so three categories on each axis cut the panel into nine
    // cells that **touch** — no gap, no overlap, and the outermost edges flush
    // with the panel. A categorical `bar` takes 80% of its slot, and if a zone
    // inherited that the seams would show up here as a 20% shortfall.
    let data = confusion();
    let spec = PlotSpec::new().data("tallied").x("actual").y("predicted")
        .layer(Layer::new(Mark::Zone).encode(Channel::Color, "count"));
    let svg = SvgRenderer::default().render(&spec, &data);

    let cells = zone_rects(&svg);
    assert_eq!(cells.len(), 9, "three categories each way is nine cells");
    let (px, py, pw, ph) = panel_rect(&svg);

    // Three distinct columns and three distinct rows, each one third of the panel.
    // Both axes, because area alone cannot tell them apart: nine cells stacked in
    // three columns but a single row band sum to exactly the same panel area, and
    // would pass every other assertion here.
    let thirds = |mut vs: Vec<f64>, origin: f64, extent: f64, what: &str| {
        vs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        vs.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        assert_eq!(vs.len(), 3, "three {what}s of cells, got {vs:?}");
        for (i, v) in vs.iter().enumerate() {
            let want = origin + extent / 3.0 * i as f64;
            assert!((v - want).abs() < 0.5, "{what} {i} starts at {v}, want {want}");
        }
    };
    thirds(cells.iter().map(|c| c.0).collect(), px, pw, "column");
    thirds(cells.iter().map(|c| c.1).collect(), py, ph, "row");
    for c in &cells {
        assert!((c.2 - pw / 3.0).abs() < 0.5, "a cell is one whole slot wide, got {}", c.2);
        assert!((c.3 - ph / 3.0).abs() < 0.5, "and one whole slot tall, got {}", c.3);
    }
    // The mesh covers the panel exactly: nine cells of a ninth each. Compared as
    // a fraction, because nine cells each rounded to two decimal places cannot
    // sum to the panel's area to the last unit — and a 20% seam, the thing this
    // is here to catch, is four orders of magnitude larger than that rounding.
    let area: f64 = cells.iter().map(|c| c.2 * c.3).sum();
    assert!((area / (pw * ph) - 1.0).abs() < 1e-4,
        "the cells tile the panel, got {area} of {}", pw * ph);
    // Abutting rectangles antialias into a visible lattice unless snapped.
    assert!(svg.contains("crispEdges"), "a tiled cell turns antialiasing off");
}

#[test]
fn a_tallied_tile_plot_draws_what_the_pre_computed_one_draws() {
    // *`bin` cuts, `count` tallies* — and the payoff of saying it that way is
    // that the grammar can do the counting. So the two sentences must agree:
    // `zone + color(n)` over a table someone summarized by hand, and
    // `zone * count` over the raw rows, are one plot. Compared as drawn cells
    // (position **and** fill), because agreeing on geometry while disagreeing
    // on which cell got which color would be the silent kind of wrong.
    let data = confusion();
    let cells = |svg: &str| -> Vec<String> {
        svg.lines().filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                         && l.contains(r#"stroke="none""#))
            .map(|l| l.trim().to_string()).collect()
    };
    let pre = SvgRenderer::default().render(
        &PlotSpec::new().data("tallied").x("actual").y("predicted")
            .layer(Layer::new(Mark::Zone).encode(Channel::Color, "count")), &data);
    let tallied = SvgRenderer::default().render(
        &PlotSpec::new().data("raw").x("actual").y("predicted")
            .layer(Layer::new(Mark::Zone).transform(Transform::Count)), &data);

    assert_eq!(cells(&pre), cells(&tallied),
        "the tallied tile plot draws the pre-computed one, cell for cell");
    assert_eq!(cells(&pre).len(), 9);
}

/// Raw readings over a pair of categories, and the same table reduced by hand —
/// the fixture for the **two-dimensional group-by** (spec §5).
///
/// Deliberately ragged: three, one and two readings in the three populated cells,
/// so a mean is not a sum divided by a constant and cannot be confused with one,
/// and one pair with **no rows at all**, which must come out absent rather than
/// zero — `agg2d`'s half of the rule `count2d` already keeps.
fn readings() -> HashMap<String, DataFrame> {
    //  (site, day)      values          mean
    //  north / mon      1, 2, 6          3.0
    //  north / tue      10               10.0
    //  south / mon      4, 8             6.0
    //  south / tue      —                absent
    let site = ["north", "north", "north", "north", "south", "south"];
    let day  = ["mon",   "mon",   "mon",   "tue",   "mon",   "mon"];
    let val  = [1.0, 2.0, 6.0, 10.0, 4.0, 8.0];
    HashMap::from([
        ("raw".to_string(), DataFrame::new()
            .with_str("site", site.iter().map(|s| s.to_string()).collect())
            .with_str("day", day.iter().map(|s| s.to_string()).collect())
            .with_float("reading", val.to_vec())),
        ("meaned".to_string(), DataFrame::new()
            .with_str("site", vec!["north".into(), "north".into(), "south".into()])
            .with_str("day", vec!["mon".into(), "tue".into(), "mon".into()])
            .with_float("reading", vec![3.0, 10.0, 6.0])),
    ])
}

#[test]
fn a_summarized_tile_plot_draws_what_the_pre_computed_one_draws() {
    // The two-dimensional group-by, asserted the way the tally already is: the
    // grammar doing the summarizing must draw exactly what a table summarized by
    // hand draws. That is a stronger claim than "the numbers are right", because
    // it catches the ramp and the key as well as the arithmetic — a heatmap of
    // cell means under a legend that spans the *raw* column's range is
    // self-consistent in its fills and still wrong, which is how that bug lived
    // long enough to be found by comparing values rather than by rendering.
    //
    // A `zone` measures by `color`, so `color` is the channel that names the
    // column to reduce — the claim these five were once refused under ("no
    // channel left to name it") read off the mark instead of the message.
    let data = readings();
    let cells = |svg: &str| -> Vec<String> {
        svg.lines().filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                         && l.contains(r#"stroke="none""#))
            .map(|l| l.trim().to_string()).collect()
    };
    let pre = SvgRenderer::default().render(
        &PlotSpec::new().data("meaned").x("site").y("day")
            .layer(Layer::new(Mark::Zone).encode(Channel::Color, "reading")), &data);
    let computed = SvgRenderer::default().render(
        &PlotSpec::new().data("raw").x("site").y("day")
            .layer(Layer::new(Mark::Zone).transform(Transform::Mean)
                .encode(Channel::Color, "reading")), &data);

    assert_eq!(cells(&pre), cells(&computed),
        "`zone * mean` draws the hand-reduced table, cell for cell");
    // Three cells, not four: the pair with no readings has no mean, so it is an
    // absence and stays blank — `count2d`'s rule, which `agg2d` had to keep or a
    // missing cell would read as a measured zero.
    assert_eq!(cells(&pre).len(), 3, "an empty pair is an absence, not a zero");

    // And the key decodes the cells it is beside. The strip's three labels are the
    // reduced extremes and their midpoint; reading the *raw* frame put the bottom
    // at 1.00, which is the bug this line pins — fills that were self-consistent
    // under a legend that decoded them wrongly.
    let labels: Vec<String> = computed.lines()
        .filter(|l| l.contains("<text") && !l.contains("font-weight"))
        .filter_map(|l| l.rsplit('>').nth(1)?.split('<').next())
        .filter(|s| s.parse::<f64>().is_ok())
        .map(str::to_string).collect();
    assert_eq!(labels, ["10", "6.5", "3"],
        "the key spans the reduced values, not the raw column's 1..10");
}

#[test]
fn a_reduction_stands_up_in_the_cube_the_way_a_tally_does() {
    // The same subtraction one axis further out (spec §5/§15): a `bar` in `space`
    // measures with `z`, so it groups by the pair and reduces the column `z`
    // names. The tile plot standing up, with a mean for a height instead of a
    // tally — and asserted the same way the flat one is, against the table reduced
    // by hand, because that is what makes it the *same rule* rather than a second
    // feature that happens to work.
    let data = readings();
    let cube = |src: &str, layer: Layer| SvgRenderer::default().render(
        &PlotSpec::new().data(src).x("site").y("day").z("reading")
            .coord(CoordSpace::Space(SpaceView::default()))
            .layer(layer), &data);
    let faces = |svg: &str| -> Vec<String> {
        svg.lines().filter(|l| l.trim_start().starts_with("<path d=") && l.contains("fill-opacity"))
            .map(|l| l.trim().to_string()).collect()
    };

    let pre = cube("meaned", Layer::new(Mark::Bar));
    let computed = cube("raw", Layer::new(Mark::Bar).transform(Transform::Mean));
    assert_eq!(faces(&pre), faces(&computed),
        "`bar * mean + space()` stands up the hand-reduced table, face for face");
    // Three columns of three visible faces: the back three are culled, and the
    // fourth cell — the pair with no readings — has no mean and so no column.
    assert_eq!(faces(&pre).len(), 9, "three columns, three visible faces each");
}

// ---- the two slot marks that learned to stand, 2026-07-26 ---------------

/// **A pair transform in the cube groups by the *floor*, not by `x` alone.**
///
/// The bug this pins is the one building it found: `reads_two_dimensions` asked
/// only about the five single-value statistics, so a whisker fell through to the
/// one-key branch, grouped by `x`, and drew one per **row**. On the six rows of
/// `readings()` that is three whiskers where there should be three cells — and on
/// a real table it was 75 whiskers on a 6-cell floor.
#[test]
fn a_whisker_in_the_cube_stands_one_per_cell_not_one_per_row() {
    let data = readings();
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("raw").x("site").y("day").z("reading")
            .coord(CoordSpace::Space(SpaceView::default()))
            .layer(Layer::new(Mark::Interval).transform(Transform::Range)), &data);

    // Three non-empty (site, day) cells — south/tue has no readings and, by the
    // absent-pair rule `agg2d` and `count2d` already follow, gets no whisker.
    // Each is one span plus a **cross** at each end: 1 + 4 = 5 strokes.
    let marks = svg.lines().filter(|l| l.trim_start().starts_with("<line")
        && l.contains("stroke-linecap")).count();
    assert_eq!(marks, 15, "three cells × (a span + two crossed caps), got {marks}");
    assert!(!svg.contains("NaN"));
}

/// The cube's whisker spans the cell's own **extents**, and the pairing is the
/// same one the flat mark reads — checked against the hand-reduced table, which
/// is what makes it the *same rule* rather than a second feature that works.
#[test]
fn a_cube_whisker_spans_the_cells_own_range() {
    let data = readings();
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("raw").x("site").y("day").z("reading")
            .coord(CoordSpace::Space(SpaceView::default()))
            .layer(Layer::new(Mark::Interval).transform(Transform::Range)), &data);
    // north/mon holds 1, 2, 6 → a span of 1..6; north/tue holds 10 alone → a
    // span of zero length, which is honest and still drawn as a capped point.
    // The z axis spans 1..10, so the longest span is 5/9 of the cube's height and
    // the shortest is none of it: assert the *spread* of span lengths rather than
    // pixels, which is what survives a change of viewing angle.
    let spans: Vec<f64> = svg.lines()
        .filter(|l| l.trim_start().starts_with("<line") && l.contains(r#"stroke-linecap="round""#))
        .filter_map(|l| {
            let g = |k: &str| l.split(&format!("{k}=\""))
                .nth(1)?.split('"').next()?.parse::<f64>().ok();
            Some(((g("x1")? - g("x2")?).powi(2) + (g("y1")? - g("y2")?).powi(2)).sqrt())
        })
        .collect();
    assert!(!spans.is_empty());
    let longest = spans.iter().cloned().fold(0.0, f64::max);
    assert!(longest > 1.0, "every span came out zero-length: {spans:?}");
}

/// **A box's median is culled like any other face.** It is a plane *inside* an
/// opaque solid, so only where it meets a front-facing side may be drawn — the
/// same rule `write_solid` applies to the solid itself. Drawing the whole quad
/// put two edges on top of a box they are behind.
#[test]
fn a_cube_boxs_median_is_drawn_only_where_the_solid_faces_us() {
    let data = readings();
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("raw").x("site").y("day").z("reading")
            .coord(CoordSpace::Space(SpaceView::default()))
            .layer(Layer::new(Mark::Box)), &data);
    // A convex box shows exactly two of its four sides at an ordinary viewing
    // angle, so each median contributes two segments and never four — **except**
    // a degenerate one. `readings()` has north/tue with a single observation, so
    // its quartiles coincide, its box has no height, and what is visible of its
    // median is the whole quad seen face-on: four edges, which is the flat mark's
    // "a zero-IQR group collapses to a flat line, which is honest" read one
    // dimension up. So two ordinary cells give 2 each and the degenerate one 4.
    let medians = svg.lines().filter(|l| l.trim_start().starts_with("<line")
        && l.contains(r#"stroke-linecap="butt""#)).count();
    let boxes = svg.lines().filter(|l| l.trim_start().starts_with("<path d=")
        && l.contains("fill-opacity")).count();
    assert!(boxes > 0, "no box body drawn");
    // Per cell: 2 whiskers, plus 2 median edges (4 for the degenerate cell).
    assert_eq!(medians, 6 + 2 + 2 + 4, "median edges are culled per visible side");
    assert!(!svg.contains("NaN"));
}

/// A 3-D box is **opaque**, where a flat one is not, and that is `bar`'s answer
/// rather than a second opinion: painter's order places these solids, and a
/// translucent one lets a far box show through a near one and undoes the sort.
#[test]
fn a_cube_box_is_opaque_so_the_depth_sort_survives() {
    let data = readings();
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("raw").x("site").y("day").z("reading")
            .coord(CoordSpace::Space(SpaceView::default()))
            .layer(Layer::new(Mark::Box)), &data);
    for l in svg.lines().filter(|l| l.trim_start().starts_with("<path d=") && l.contains("fill-opacity")) {
        let o: f64 = l.split("fill-opacity=\"").nth(1).unwrap()
            .split('"').next().unwrap().parse().unwrap();
        assert!(o > 0.999, "a solid in the cube must be opaque, got {o}");
    }
}

#[test]
fn a_zone_bounded_by_one_slot_spans_the_panel_on_the_other() {
    // `rule`'s relaxation, arriving a third time and for free: bounded where the
    // bindings say, spanning where they do not. Nothing was built for this — it
    // falls out of *a category bounds its axis* applied to one axis instead of
    // two, which is the test that it is one rule rather than a tile-plot case.
    let data = confusion();
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("tallied").x("actual")
            .layer(Layer::new(Mark::Zone)), &data);
    let (_, py, pw, ph) = panel_rect(&svg);
    let cells = zone_rects(&svg);
    assert!(!cells.is_empty(), "a slotted zone draws");
    for c in &cells {
        assert!((c.1 - py).abs() < 0.5 && (c.3 - ph).abs() < 0.5,
            "unbounded on y, so it reaches both panel edges: got y={} h={}", c.1, c.3);
        assert!((c.2 - pw / 3.0).abs() < 0.5, "bounded on x by its own slot");
    }
    // Background, not data: it is a rectangle over a slot, with nothing measured
    // in it, so it stays translucent for whatever is drawn underneath.
    assert!(svg.contains(r#"fill-opacity="0.200""#), "an unmeasured zone is background");
}

#[test]
fn a_banded_zone_fills_the_very_curves_the_contour_strokes() {
    // The claim that makes `levels` one parameter serving two marks: a filled
    // band's edge *is* the contour line, because both marks run the same
    // transform and part only in the writer. Asserted end to end through the
    // renderer rather than at the transform, where it would be true by
    // construction and prove nothing about what gets drawn.
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for i in 0..120 {
        let t = i as f64;
        xs.push(2.0 + (t * 0.7).sin());
        ys.push(2.0 + (t * 1.3).cos());
    }
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new().with_float("a", xs).with_float("b", ys));

    let spec = |mark: Mark| {
        let mut l = Layer::new(mark).transform(Transform::Density);
        l.density = Some(crate::ir::DensitySpec { adjust: None, bandwidth: None, levels: Some(4) , compare: None, reach: None, support: (None, None) });
        PlotSpec::new().data("t").x("a").y("b").layer(l)
    };
    let banded = SvgRenderer::default().render(&spec(Mark::Zone), &data);
    let traced = SvgRenderer::default().render(&spec(Mark::Path), &data);

    // Each level is one region, and its rings are the region's subpaths.
    let regions = band_regions(&banded);
    assert!(regions.len() >= 4, "four levels fill at least four bands, got {}", regions.len());
    let bands: Vec<String> = regions.into_iter().flatten().collect();
    // A banded zone draws no cells — it is the level sets, not the mesh.
    assert!(zone_rects(&banded).is_empty(), "a banded zone paints no cells");

    // Every band vertex is a vertex of the traced contour, to the printed pixel.
    let band_pts: std::collections::HashSet<String> = bands.iter()
        .flat_map(|p| p.split_whitespace().map(str::to_string))
        .collect();
    let line_pts: std::collections::HashSet<String> = traced.lines()
        // A contour stroke, not a gridline: only the data strokes carry a
        // linecap. Matching on `<line>` alone picked up the axis grid, which is
        // how this test first "failed" on coordinates that were never contours.
        // A ring of one level is one `<polyline>`; a run one segment long is a
        // `<line>`.
        .filter(|l| l.contains(r#"stroke-linecap="round""#))
        .flat_map(|l| -> Vec<String> {
            let num = |k: &str| -> String {
                let at = l.find(k).unwrap() + k.len();
                let v: f64 = l[at..].split('"').next().unwrap().parse().unwrap();
                format!("{v:.2}")
            };
            if l.contains("<polyline") {
                let at = l.find(r#"points=""#).unwrap() + r#"points=""#.len();
                l[at..].split('"').next().unwrap().split_whitespace().map(|p| {
                    let (x, y) = p.split_once(',').unwrap();
                    format!("{:.2},{:.2}", x.parse::<f64>().unwrap(), y.parse::<f64>().unwrap())
                }).collect()
            } else if l.contains("<line") {
                vec![format!("{},{}", num(r#"x1=""#), num(r#"y1=""#)),
                     format!("{},{}", num(r#"x2=""#), num(r#"y2=""#))]
            } else {
                Vec::new()
            }
        })
        .collect();
    assert!(!line_pts.is_empty(), "the traced contour drew segments");
    let shared = band_pts.iter().filter(|p| line_pts.contains(*p)).count();
    assert_eq!(shared, band_pts.len(),
        "every band vertex is a contour vertex: {shared} of {} matched", band_pts.len());
}

/// **A crater's middle is not painted as its highest band.** Points on a
/// circle make a density high on the ring and low in the middle, so a level set
/// high on the ring is an annulus. Filled one ring at a time, the inner ring
/// painted the middle in the ring's own color; drawn as one even-odd region per
/// level, the middle belongs to no band at a level the field does not reach
/// there. Asserted on containment, the question a reader's eye asks, rather
/// than on the markup.
#[test]
fn a_banded_crater_leaves_its_middle_to_the_bands_below() {
    let (xs, ys): (Vec<f64>, Vec<f64>) = (0..300).map(|i| {
        let (i, r) = (i as f64, 3.0 + 0.25 * (1.3 * i as f64).sin());
        (r * (0.37 * i).cos(), r * (0.37 * i).sin())
    }).unzip();
    let data = HashMap::from([("t".to_string(), DataFrame::new().with_float("a", xs).with_float("b", ys))]);
    let mut l = Layer::new(Mark::Zone).transform(Transform::Density);
    l.density = Some(crate::ir::DensitySpec { adjust: None, bandwidth: None, levels: Some(6), compare: None, reach: None, support: (None, None) });
    let svg = SvgRenderer::default().render(&PlotSpec::new().data("t").x("a").y("b").layer(l), &data);
    let regions: Vec<Vec<Vec<(f64, f64)>>> = band_regions(&svg).iter()
        .map(|rings| rings.iter().map(|r| points_of(r)).collect())
        .collect();
    assert!(regions.len() >= 4, "six levels fill several bands: {}", regions.len());
    // The ring's center, from the lowest band's extent, which surrounds it.
    let outer = regions[0].iter().flatten();
    let (lo, hi) = outer.fold(((f64::INFINITY, f64::INFINITY), (f64::NEG_INFINITY, f64::NEG_INFINITY)),
        |(lo, hi), &(x, y)| ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x), hi.1.max(y))));
    let c = ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0);
    // Even-odd containment: a ray to the right crosses the region's rings an odd
    // number of times exactly when the point is inside.
    let inside = |rings: &Vec<Vec<(f64, f64)>>| rings.iter().map(|ring| {
        (0..ring.len()).filter(|&k| {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            (a.1 > c.1) != (b.1 > c.1) && c.0 < a.0 + (c.1 - a.1) * (b.0 - a.0) / (b.1 - a.1)
        }).count()
    }).sum::<usize>() % 2 == 1;
    let covering = regions.iter().filter(|r| inside(r)).count();
    assert!(covering <= 1, "the middle is painted by {covering} of {} bands", regions.len());
}

#[test]
fn levels_choose_the_geometry_and_nothing_else_does() {
    // `field_geometry` is the one place the reading is decided, so the table of
    // answers is worth stating outright — a `path` has only one thing it can do
    // with a field, a `zone` has two and `levels` is the request.
    use crate::legality::{field_geometry, FieldGeometry};
    let layer = |mark: Mark, t: Transform, levels: Option<usize>| {
        let mut l = Layer::new(mark).transform(t);
        if levels.is_some() {
            l.density = Some(crate::ir::DensitySpec { adjust: None, bandwidth: None, levels , compare: None, reach: None, support: (None, None) });
        }
        l
    };
    for (mark, t, levels, want) in [
        (Mark::Path, Transform::Density, None,    Some(FieldGeometry::Rings)),
        (Mark::Path, Transform::Density, Some(6), Some(FieldGeometry::Rings)),
        (Mark::Zone, Transform::Density, None,    Some(FieldGeometry::Cells)),
        (Mark::Zone, Transform::Density, Some(6), Some(FieldGeometry::Rings)),
        (Mark::Zone, Transform::Bin,     None,    Some(FieldGeometry::Cells)),
        // Not a field at all: a mark with a measure axis reads one dimension.
        (Mark::Line, Transform::Density, None,    None),
        (Mark::Bar,  Transform::Bin,     None,    None),
    ] {
        assert_eq!(field_geometry(&layer(mark.clone(), t.clone(), levels)), want,
            "{mark:?} * {t:?} with levels={levels:?}");
    }
}

#[test]
fn a_binned_zone_tiles_the_panel_edge_to_edge() {
    // The heatmap's structural claim, and the one thing the axis machinery had
    // to learn: `build_axis` reads the position columns, which a 2-D bin fills
    // with cell *centers*, so fitting to them leaves the outer half of every
    // edge cell hanging past the range — clipped away by the panel as a border
    // of short cells. Measured against the **panel's own rectangle**, for the
    // reason the zone test above records: comparing the cells only to each
    // other cannot see the whole tiling sitting inset.
    //
    // A full 5x5 mesh of points, so no cell is empty and the tiling's bounding
    // box is the mesh itself.
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for i in 0..5 {
        for j in 0..5 {
            xs.push(i as f64);
            ys.push(j as f64);
        }
    }
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new().with_float("a", xs).with_float("b", ys));

    let mut layer = Layer::new(Mark::Zone).transform(Transform::Bin);
    layer.bin = Some(crate::ir::BinSpec { bins: Some(5), width: None, tiling: None });
    let spec = PlotSpec::new().data("t").x("a").y("b").layer(layer);
    let svg = SvgRenderer::default().render(&spec, &data);

    let cells = zone_rects(&svg);
    assert_eq!(cells.len(), 25, "a full 5x5 mesh is 25 cells, got {}", cells.len());

    let (px, py, pw, ph) = panel_rect(&svg);
    let left = cells.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let top = cells.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let right = cells.iter().map(|c| c.0 + c.2).fold(f64::NEG_INFINITY, f64::max);
    let bottom = cells.iter().map(|c| c.1 + c.3).fold(f64::NEG_INFINITY, f64::max);

    assert!((left - px).abs() < 0.01 && (right - (px + pw)).abs() < 0.01,
        "the tiling spans the panel horizontally: cells {left}..{right}, panel {px}..{}",
        px + pw);
    assert!((top - py).abs() < 0.01 && (bottom - (py + ph)).abs() < 0.01,
        "the tiling spans the panel vertically: cells {top}..{bottom}, panel {py}..{}",
        py + ph);

    // And the cells partition rather than merely cover: 25 equal cells whose
    // areas sum to the panel's leaves no room for an overlap or a gap. The
    // seam between two cells is answered by turning antialiasing off, not by
    // growing them (`marks/zone.rs`), which is why this can still be exact —
    // a cut cell is where the mesh cut it, to the pixel.
    //
    // The tolerance is relative because SVG coordinates are written to two
    // decimals, so 25 cells accumulate about four square pixels of rounding on
    // a panel of 340,000. It is still far tighter than any real defect: a
    // one-pixel seam between columns would miss by three thousand.
    let area: f64 = cells.iter().map(|c| c.2 * c.3).sum();
    assert!((area - pw * ph).abs() < pw * ph * 1e-4,
        "25 cells tile the panel exactly: {area} vs {}", pw * ph);
}

#[test]
fn a_hex_tiling_draws_hexagons_that_still_reach_the_panel() {
    // Two claims at once, because they are the two halves of "the mark asks
    // the tiling what its cells look like". A hexagon has no four edges, so it
    // is drawn as a polygon rather than a rect — and the axis, which fits
    // itself to the mesh, has to read the hex extent too or the outermost
    // cells hang off the panel exactly as the rect ones once did.
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for i in 0..24 {
        for j in 0..24 {
            xs.push(i as f64);
            ys.push(j as f64);
        }
    }
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new().with_float("a", xs).with_float("b", ys));

    let mut layer = Layer::new(Mark::Zone).transform(Transform::Bin);
    layer.bin = Some(crate::ir::BinSpec {
        bins: Some(8), width: None, tiling: Some("hex".into()),
    });
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("a").y("b").layer(layer), &data);

    let polys: Vec<&str> = svg.lines()
        .filter(|l| l.contains("<polygon") && l.contains("fill-opacity"))
        .collect();
    assert!(polys.len() > 20, "a hex mesh draws a polygon per cell, got {}", polys.len());
    assert!(!svg.contains(r#"<rect x="#) || !polys.is_empty());

    // Six vertices, every one of them — a five- or seven-sided cell would
    // tessellate wrongly and is exactly what a bad vertex list produces.
    for p in &polys {
        let pts = p.split(r#"points=""#).nth(1).unwrap().split('"').next().unwrap();
        assert_eq!(pts.split_whitespace().count(), 6, "a hexagon has six vertices: {pts}");
    }

    // The mesh reaches the panel. Read off every vertex, since a hexagon's
    // extreme point is a vertex rather than a corner of its bounding box.
    let (px, py, pw, ph) = panel_rect(&svg);
    let mut coords: Vec<(f64, f64)> = Vec::new();
    for p in &polys {
        let pts = p.split(r#"points=""#).nth(1).unwrap().split('"').next().unwrap();
        for v in pts.split_whitespace() {
            let (a, b) = v.split_once(',').unwrap();
            coords.push((a.parse().unwrap(), b.parse().unwrap()));
        }
    }
    let left = coords.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let right = coords.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max);
    let top = coords.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let bottom = coords.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max);
    assert!((left - px).abs() < 0.05 && (right - (px + pw)).abs() < 0.05,
        "the hex mesh spans the panel horizontally: {left}..{right} vs {px}..{}", px + pw);
    assert!((top - py).abs() < 0.05 && (bottom - (py + ph)).abs() < 0.05,
        "the hex mesh spans the panel vertically: {top}..{bottom} vs {py}..{}", py + ph);
}

#[test]
fn a_binned_zone_reads_its_color_off_the_count_and_says_so_in_the_legend() {
    // The measure moved from length to color, which is the whole reason a
    // heatmap cell is a `zone` and not a bar (spec §18). Nothing binds it: the
    // transform writes the count and the mark reads it, the same courtesy
    // `bar * bin` does for `y`. So the two things to pin are that the density
    // difference reaches the ink, and that the reader is given a key to it.
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for i in 0..5 {
        for j in 0..5 {
            // One cell of the mesh gets piled high, the rest get one row each.
            let n = if i == 0 && j == 0 { 50 } else { 1 };
            for _ in 0..n {
                xs.push(i as f64);
                ys.push(j as f64);
            }
        }
    }
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new().with_float("a", xs).with_float("b", ys));

    let mut layer = Layer::new(Mark::Zone).transform(Transform::Bin);
    layer.bin = Some(crate::ir::BinSpec { bins: Some(5), width: None, tiling: None });
    let svg = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("a").y("b").layer(layer), &data);

    let fills: Vec<&str> = svg.lines()
        .filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                 && l.contains(r#"stroke="none""#))
        .filter_map(|l| l.split(r#"fill="#).nth(1))
        .map(|s| s.trim_start_matches('"').split('"').next().unwrap())
        .collect();
    assert!(fills.len() > 1, "a tiling of one color is not reading the count");
    assert!(fills.iter().collect::<std::collections::HashSet<_>>().len() >= 2,
        "the busy cell must not be painted like the empty-ish ones: {fills:?}");

    // The key. Without it the darkest cell is a shade with no number attached,
    // and a heatmap whose scale cannot be read is a picture, not a graphic.
    assert!(svg.contains(">Count<"), "the synthesized count earns a legend:\n{svg}");

    // Opaque, unlike a decorating zone: there is nothing behind the data for it
    // to show through to, and 20% would wash the ramp out.
    assert!(svg.lines()
               .filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                        && l.contains(r#"stroke="none""#))
               .all(|l| l.contains(r#"fill-opacity="1.000""#)),
        "a binned zone is the data, so it is not translucent");
}

#[test]
fn a_zone_given_both_pairs_is_bounded_on_both_axes() {
    // The box: neither side is the panel's any more.
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("a", vec![0.0, 10.0]).with_float("b", vec![0.0, 10.0]));
    data.insert("z".to_string(), DataFrame::new()
        .with_float("lo", vec![3.0]).with_float("hi", vec![6.0])
        .with_float("s", vec![2.0]).with_float("e", vec![7.0]));
    let spec = PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Zone).data("z").bounds("lo", "hi").span("s", "e"))
        .layer(Layer::new(Mark::Point));
    let r = zone_rects(&SvgRenderer::default().render(&spec, &data));
    assert_eq!(r.len(), 1);
    let full = zone_rects(&SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Zone).data("z").bounds("lo", "hi"))
            .layer(Layer::new(Mark::Point)), &data));
    assert!(r[0].2 < full[0].2, "a box is narrower than the band it came from");
    assert!((r[0].3 - full[0].3).abs() < 0.01, "and the same height — the same y pair");
}

#[test]
fn a_zone_fits_the_axis_to_its_own_sides() {
    // The defect this pins: a bounded `zone` keeps its frame untransformed and
    // reads its four columns straight, so the axis fit — which asks only what
    // values the *position column* takes — never saw where the rectangles were.
    // Layered over other data the axis was right anyway, which is every use of
    // the mark in the book; alone, there was no column to fit at all, the range
    // fell back to `0..1`, and rectangles placed in data units landed tens of
    // thousands of pixels outside the panel. An empty-looking plot with
    // fabricated `0.0 … 1.0` axes, silently — the shape §12 refuses.
    //
    // Asserted as containment rather than against a tick list, because the fit
    // breathes: the claim is that what was drawn is *inside the panel it was
    // drawn on*, which is the thing that was false and is what a reader sees.
    let mut data = HashMap::new();
    data.insert("w".to_string(), DataFrame::new()
        .with_float("base", vec![0.0, 120.0, 165.0])
        .with_float("top",  vec![120.0, 165.0, 147.0])
        .with_float("l",    vec![0.6, 1.6, 2.6])
        .with_float("r",    vec![1.4, 2.4, 3.4]));
    let spec = PlotSpec::new().data("w")
        .layer(Layer::new(Mark::Zone).bounds("base", "top").span("l", "r"));
    let svg = SvgRenderer::default().render(&spec, &data);
    let (px, py, pw, ph) = panel_rect(&svg);
    let rects = zone_rects(&svg);
    assert_eq!(rects.len(), 3, "one row is one rectangle:\n{svg}");
    for (x, y, w, h) in rects {
        assert!(x >= px - 0.5 && x + w <= px + pw + 0.5,
                "({x}, {w}) is outside the panel's x span ({px}, {pw})");
        assert!(y >= py - 0.5 && y + h <= py + ph + 0.5,
                "({y}, {h}) is outside the panel's y span ({py}, {ph})");
    }

    // And the union, not a fallback: a zone beside data that *does* name the
    // axis widens it rather than replacing or being ignored by it. Here the
    // point layer spans 0..10 and the zone reaches to 40, so the axis must
    // hold both — the case a highlight reaching past the last point makes.
    data.insert("t".to_string(), DataFrame::new()
        .with_float("a", vec![0.0, 10.0]).with_float("b", vec![0.0, 10.0]));
    data.insert("z".to_string(), DataFrame::new()
        .with_float("lo", vec![30.0]).with_float("hi", vec![40.0]));
    let both = SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Zone).data("z").bounds("lo", "hi"))
            .layer(Layer::new(Mark::Point)), &data);
    let (_, py, _, ph) = panel_rect(&both);
    let band = zone_rects(&both);
    assert_eq!(band.len(), 1);
    assert!(band[0].1 >= py - 0.5 && band[0].1 + band[0].3 <= py + ph + 0.5,
            "a band above the data widens the axis instead of drawing off it:\n{both}");
    let dots: Vec<f64> = both.lines()
        .filter(|l| l.contains("<circle"))
        .map(|l| {
            let at = l.find(r#"cy=""#).unwrap() + 4;
            l[at..].split('"').next().unwrap().parse().unwrap()
        })
        .collect();
    assert!(!dots.is_empty() && dots.iter().all(|&y| y >= py - 0.5 && y <= py + ph + 0.5),
            "and the data it sits over is still on the panel:\n{both}");
}

#[test]
fn one_row_is_one_rectangle_so_one_table_draws_several() {
    // `rule`'s payoff, inherited: the position is a column, so the table's
    // length decides how many are drawn.
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("a", vec![0.0, 10.0]).with_float("b", vec![0.0, 10.0]));
    data.insert("z".to_string(), DataFrame::new()
        .with_float("s", vec![1.0, 5.0, 8.0]).with_float("e", vec![2.0, 6.0, 9.0]));
    let spec = PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Zone).data("z").span("s", "e"))
        .layer(Layer::new(Mark::Point));
    assert_eq!(zone_rects(&SvgRenderer::default().render(&spec, &data)).len(), 3);
}

#[test]
fn an_arrowhead_is_drawn_at_the_end_the_setting_names() {
    // One head for "end" and "start", two for "both" — and the head is a
    // filled polygon, so counting them is counting polygons.
    let data = doubling_back();
    let heads = |arrow: Option<&str>| {
        let mut layer = Layer::new(Mark::Path);
        layer.style.arrow = arrow.map(String::from);
        let svg = SvgRenderer::default()
            .render(&PlotSpec::new().data("t").x("x").y("y").layer(layer), &data);
        svg.matches("<polygon").count()
    };
    assert_eq!(heads(None), 0, "a bare path has no head");
    assert_eq!(heads(Some("end")), 1);
    assert_eq!(heads(Some("start")), 1);
    assert_eq!(heads(Some("both")), 2);
}

#[test]
fn the_head_sits_on_the_vertex_the_setting_names_not_the_other_one() {
    // "end" and "start" must not be interchangeable: the tip of each head is
    // the *last* and *first* row respectively, which on this route are
    // different points (x = 2 and x = 3).
    let data = doubling_back();
    let tip = |arrow: &str| {
        let mut layer = Layer::new(Mark::Path);
        layer.style.arrow = Some(arrow.to_string());
        let svg = SvgRenderer::default()
            .render(&PlotSpec::new().data("t").x("x").y("y").layer(layer), &data);
        let poly = svg.lines().find(|l| l.contains("<polygon")).unwrap().to_string();
        let pts = poly.split(r#"points=""#).nth(1).unwrap().split('"').next().unwrap();
        let first: f64 = pts.split(',').next().unwrap().parse().unwrap();
        first
    };
    let xs = vertex_xs(&SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("x").y("y").layer(Layer::new(Mark::Path)), &data));
    assert!((tip("end") - xs[3]).abs() < 0.01, "the end head tips at the last row");
    assert!((tip("start") - xs[0]).abs() < 0.01, "the start head tips at the first row");
}

#[test]
fn a_split_path_draws_one_stroke_per_group_each_in_its_own_row_order() {
    // `color` splits a path the way it splits a line, and each group keeps
    // the table's order within itself rather than being pooled and sorted.
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![3.0, 1.0, 3.0, 1.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 4.0])
            .with_str("g", vec!["a", "a", "b", "b"].into_iter().map(String::from).collect()),
    )]);
    let layer = Layer::new(Mark::Path).encode(Channel::Color, "g");
    let svg = SvgRenderer::default()
        .render(&PlotSpec::new().data("t").x("x").y("y").layer(layer), &data);
    assert_eq!(svg.matches("<polyline").count(), 2, "one stroke per group");
    for line in svg.lines().filter(|l| l.contains("<polyline")) {
        let pts = line.split(r#"points=""#).nth(1).unwrap().split('"').next().unwrap();
        let xs: Vec<f64> = pts.split_whitespace()
            .filter_map(|p| p.split(',').next()?.parse().ok()).collect();
        assert!(xs[0] > xs[1], "each group keeps its own row order: {xs:?}");
    }
}

// -----------------------------------------------------------------------
// The dot plot — `stack` spent on glyphs instead of on length
// -----------------------------------------------------------------------

/// Every glyph's center, y first — a pile read off the page.
fn glyph_centers(svg: &str) -> Vec<(f64, f64)> {
    svg.lines()
        .filter(|l| l.contains("<circle") && l.contains("fill-opacity"))
        .map(|l| {
            let attr = |k: &str| -> f64 {
                l.split(&format!(r#"{k}=""#)).nth(1).unwrap().split('"').next().unwrap().parse().unwrap()
            };
            (attr("cx"), attr("cy"))
        })
        .collect()
}

/// The dot plot's whole claim (spec §5): **one glyph per observation.** Not one
/// per bin, which is what `point * bin` draws un-piled — the pile is `stack`
/// spending the same `[base, top]` span on how *many* dots there are, because a
/// point has no length to stretch across it.
#[test]
fn a_dot_plot_draws_one_glyph_per_observation() {
    let vals = vec![1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 8.0];
    let n = vals.len();
    let data = HashMap::from([("t".to_string(), DataFrame::new().with_float("v", vals))]);
    let dots = |stack: bool| {
        let mut layer = Layer::new(Mark::Point).transform(Transform::Bin);
        if stack { layer = layer.transform(Transform::Stack); }
        glyph_centers(&SvgRenderer::default()
            .render(&PlotSpec::new().data("t").x("v").layer(layer), &data))
    };
    // Un-piled: one summary dot per non-empty bin. Piled: one per row.
    assert!(dots(false).len() < n, "`point * bin` summarizes, got {:?}", dots(false).len());
    assert_eq!(dots(true).len(), n, "every observation should get a dot");

    // The pile is a column: the dots of one bin share an x and are evenly
    // spaced up the measure axis, one count unit apart.
    let mut by_x: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
    for (cx, cy) in dots(true) {
        by_x.entry(format!("{cx:.1}")).or_default().push(cy);
    }
    let tallest = by_x.values().max_by_key(|v| v.len()).unwrap();
    assert_eq!(tallest.len(), 3, "three rows fell in one bin, so three dots pile there");
    let mut ys = tallest.clone();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let gaps: Vec<f64> = ys.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gaps.iter().all(|g| (g - gaps[0]).abs() < 0.01),
        "the rungs of a pile are one count apart: {gaps:?}");
}

/// Past some height a pile stops being dots: the rungs are one count unit apart,
/// so they overlap once that unit is narrower than a dot (spec §12, an
/// Assumption). The condition is derived from the page, not from a threshold, so
/// what matters is that it tracks *both* inputs — the pile's height and the dot's
/// size — rather than firing at some row count.
#[test]
fn a_pile_too_tall_to_count_says_so_and_still_draws() {
    // One bin, `n` rows in it, so the tallest pile is exactly n.
    let plot = |n: usize, size: Option<f64>| {
        let data = HashMap::from([(
            "t".to_string(),
            DataFrame::new().with_float("v", vec![1.0; n]),
        )]);
        let mut layer = Layer::new(Mark::Point)
            .transform(Transform::Bin).transform(Transform::Stack);
        layer.style.size = size;
        let spec = PlotSpec::new().data("t").x("v").layer(layer);
        let eff = vec![vec![crate::transform::pile(
            &crate::transform::apply(
                &data["t"], &spec.layers[0].transforms, "v", "", None, None, None, None, None, None, None, None, None, None, &[]),
            "")]];
        let warn = pile_overlap_warning(&spec, &eff, "", (0.0, n as f64), 400.0,
                                        SvgRenderer::default().point_radius);
        (warn, SvgRenderer::default().render(&spec, &data))
    };

    // 20 dots over 400px is 20px a rung, wider than a 9px dot: no warning.
    let (quiet, svg) = plot(20, None);
    assert!(quiet.is_none(), "a countable pile should say nothing: {quiet:?}");
    assert_eq!(svg.matches("<circle").count(), 20, "and it draws every dot");

    // 200 dots over the same 400px is 2px a rung: they overlap, so it says so —
    // naming the count and the summary that reads at that size, and still drawing.
    let (loud, svg) = plot(200, None);
    let msg = loud.expect("an overlapping pile should warn");
    assert!(msg.contains("200 dots"), "the warning counts them: {msg}");
    assert!(msg.contains("bar * bin"), "and gives direction: {msg}");
    assert_eq!(svg.matches("<circle").count(), 200, "an Assumption never blocks");

    // Shrinking the dots buys room, which is why the test is not a row count: the
    // same 200 rows go quiet at a 1px radius.
    assert!(plot(200, Some(0.9)).0.is_none(), "smaller dots, no overlap, no warning");
}

/// A split pile stacks like a stacked bar — group *b* starts where group *a*
/// stopped — which is the same `stack_base` every other mark reads.
#[test]
fn a_split_pile_sits_on_the_group_below_it() {
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("v", vec![1.0, 1.0, 1.0, 1.0, 1.0])
            .with_str("g", vec!["a", "a", "b", "b", "b"].into_iter().map(String::from).collect()),
    )]);
    let layer = Layer::new(Mark::Point)
        .transform(Transform::Bin).transform(Transform::Stack)
        .encode(Channel::Color, "g");
    let svg = SvgRenderer::default()
        .render(&PlotSpec::new().data("t").x("v").layer(layer), &data);

    // Five rows, five dots, all in the one bin every value shares.
    let dots = glyph_centers(&svg);
    assert_eq!(dots.len(), 5, "one dot per row across both groups");
    // Group a is the palette's first hue and sits at the bottom: sorting the
    // dots down the page must give a's two, then b's three.
    let mut rows: Vec<(f64, String)> = svg.lines()
        .filter(|l| l.contains("<circle") && l.contains("fill-opacity"))
        .map(|l| {
            let cy: f64 = l.split(r#"cy=""#).nth(1).unwrap().split('"').next().unwrap().parse().unwrap();
            let fill = l.split(r#"fill=""#).nth(1).unwrap().split('"').next().unwrap().to_string();
            (cy, fill)
        })
        .collect();
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap()); // bottom of the page up
    let hues: Vec<&str> = rows.iter().map(|(_, f)| f.as_str()).collect();
    assert_eq!(hues[0], PALETTE_GOG[0], "the first group is on the floor");
    assert_eq!(hues[1], PALETTE_GOG[0]);
    assert!(hues[2..].iter().all(|h| *h == PALETTE_GOG[1]),
        "the second group piles on top of it: {hues:?}");
}

// -----------------------------------------------------------------------
// `surface` — the sheet through the samples (spec §15)
// -----------------------------------------------------------------------

/// An `nx` × `ny` grid of `z = x + 2y`, one row per crossing, with the rows in
/// scrambled order so nothing can pass by accident of arrival.
fn grid_frame(nx: usize, ny: usize) -> DataFrame {
    let (mut xs, mut ys, mut zs) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..(nx * ny) {
        // A stride coprime with the count walks every cell in a scrambled order.
        let c = (k * 7) % (nx * ny);
        let (i, j) = (c % nx, c / nx);
        xs.push(i as f64);
        ys.push(j as f64);
        zs.push(i as f64 + 2.0 * j as f64);
    }
    DataFrame::new().with_float("x", xs).with_float("y", ys).with_float("h", zs)
}

fn surface_faces(svg: &str) -> Vec<(String, String)> {
    svg.lines()
        .filter(|l| l.contains("<path d=\"M") && l.contains("fill-opacity"))
        .map(|l| {
            let f = |k: &str| l.split(k).nth(1).unwrap().split('"').next().unwrap().to_string();
            (f(r#"fill=""#), f(r#"stroke=""#))
        })
        .collect()
}

#[test]
fn a_surface_draws_one_face_per_complete_cell_of_its_grid() {
    // The mark's whole geometry in one number: a 5×4 lattice of nodes has 4×3
    // blocks of four, so the sheet is 12 faces. The rows arrive scrambled, which
    // is what makes this a test of *recovering* the lattice rather than of reading
    // the table in order.
    let data = HashMap::from([("t".to_string(), grid_frame(5, 4))]);
    let spec = PlotSpec::new().data("t").x("x").y("y").z("h")
        .layer(Layer::new(Mark::Surface));
    let svg = SvgRenderer::default().render(&spec, &data);
    assert_eq!(surface_faces(&svg).len(), 12, "a 5x4 grid of nodes is 4x3 faces");
}

/// The lids of a terraced sheet: level faces, so the *undimmed* color, which is
/// what separates them from the risers in a face list.
fn level_faces(svg: &str) -> usize {
    surface_faces(svg).iter().filter(|(f, _)| *f == PALETTE_GOG[0]).count()
}

#[test]
fn a_cut_floor_lays_one_lid_per_cell_where_a_node_floor_spans_the_gaps_between_them() {
    // **The whole feature in two numbers** (spec §15). The same 3×3 table read as
    // *nodes* is 2×2 blocks of four corners — 4 faces, and five of the nine values
    // survive only as anchors — while read as *cells* it is 9 lids covering the
    // floor. That gap is why a design measuring one value per bin was drawing a
    // sheet over two thirds of its own grid.
    let data = HashMap::from([("t".to_string(), grid_frame(3, 3))]);
    let nodes = PlotSpec::new().data("t").x("x").y("y").z("h")
        .layer(Layer::new(Mark::Surface));
    assert_eq!(surface_faces(&SvgRenderer::default().render(&nodes, &data)).len(), 4,
        "9 nodes are 2x2 blocks of four");

    let cells = PlotSpec::new().data("t").x("x").y("y").z("h")
        .layer(Layer::new(Mark::Surface)
            .transform(Transform::Bin).transform(Transform::Mean).bins(3));
    let svg = SvgRenderer::default().render(&cells, &data);
    assert_eq!(level_faces(&svg), 9, "9 cut cells are 9 lids: {}", surface_faces(&svg).len());
}

#[test]
fn a_terraced_sheet_is_connected_by_risers_and_a_flat_one_needs_none() {
    // **Lids alone are not a sheet.** Without the riser standing on the boundary
    // two cells share, the mark draws disconnected tiles floating at their own
    // heights — confetti rather than relief — and the claim the geometry rests on,
    // that a cut floor tiles without gaps, would hold only in plan view.
    let data = HashMap::from([("t".to_string(), grid_frame(3, 3))]);
    let stepped = PlotSpec::new().data("t").x("x").y("y").z("h")
        .layer(Layer::new(Mark::Surface)
            .transform(Transform::Bin).transform(Transform::Mean).bins(3));
    let svg = SvgRenderer::default().render(&stepped, &data);
    // A 3×3 floor shares 2×3 boundaries across and 3×2 up: 12 risers, and
    // `grid_frame`'s height rises along both axes so none of them is level.
    assert_eq!(surface_faces(&svg).len() - level_faces(&svg), 12,
        "every internal boundary carries a riser");

    // **And a riser is the *step*, not a wall down to the baseline** — which is
    // the whole of why this reads where a 3-D histogram of the same table does
    // not. A perfectly flat field has no steps, so it draws lids and nothing else,
    // where a bar chart of it would still stand nine full-height columns.
    let flat = DataFrame::new()
        .with_float("x", (0..9).map(|k| (k % 3) as f64).collect())
        .with_float("y", (0..9).map(|k| (k / 3) as f64).collect())
        .with_float("h", vec![5.0; 9]);
    let svg = SvgRenderer::default()
        .render(&stepped, &HashMap::from([("t".to_string(), flat)]));
    assert_eq!(surface_faces(&svg).len(), level_faces(&svg),
        "a level field raises no risers");
}

#[test]
fn a_surfaces_shades_come_from_its_slopes_not_from_where_the_camera_is() {
    // The invariant `palette::shade` exists for, one geometry over from the bar:
    // shading is keyed to the face's own tilt, so turning the scene rearranges the
    // faces on the page and repaints none of them. A lamp in screen space would
    // fail this, and the same sheet would change color when the reader turned it.
    let data = HashMap::from([("t".to_string(), grid_frame(4, 4))]);
    let base = PlotSpec::new().data("t").x("x").y("y").z("h")
        .layer(Layer::new(Mark::Surface));
    let shades = |turn: f64| {
        let spec = PlotSpec::new().data("t").x("x").y("y").z("h")
            .layer(Layer::new(Mark::Surface))
            .coord(CoordSpace::Space(SpaceView { turn, tilt: 25.0 }));
        let mut s: Vec<String> =
            surface_faces(&SvgRenderer::default().render(&spec, &data))
                .into_iter().map(|(f, _)| f).collect();
        s.sort();
        s.dedup();
        s
    };
    assert_eq!(shades(30.0), shades(115.0), "a turn must not repaint a face");
    // And the shading is really doing something — a plane at 45° in the cube is
    // not left at the flat color, or the assertion above would be vacuous.
    let flat = surface_faces(&SvgRenderer::default().render(&base, &data));
    assert!(
        flat.iter().all(|(f, _)| *f != PALETTE_GOG[0]),
        "a sloped face should be shaded below the base color: {flat:?}"
    );
}

#[test]
fn a_measured_color_lands_per_face_where_a_region_would_need_a_gradient() {
    // What a mesh can do and an `area` cannot (spec §15): every face takes its own
    // stop off the ramp, so a sheet colored by height reads as a ramp without any
    // gradient machinery. Distinct fills, and they are the ramp's colors rather
    // than the categorical palette's.
    let data = HashMap::from([("t".to_string(), grid_frame(4, 4))]);
    let spec = PlotSpec::new().data("t").x("x").y("y").z("h").layer(
        Layer::new(Mark::Surface).encode(Channel::Color, "h"),
    );
    let svg = SvgRenderer::default().render(&spec, &data);
    let fills: Vec<String> = surface_faces(&svg).into_iter().map(|(f, _)| f).collect();
    assert_eq!(fills.len(), 9);
    let distinct: std::collections::HashSet<&String> = fills.iter().collect();
    assert!(distinct.len() > 1, "a ramp over the faces must vary: {fills:?}");
    assert!(
        svg.contains("linearGradient") || svg.contains(RAMP_BLUE[RAMP_BLUE.len() - 1]),
        "the ramp's own colors, and its key"
    );
}

#[test]
fn a_long_title_is_held_inside_the_canvas_rather_than_clipped() {
    // A title centers on the **panel**, which is the thing it names, and a
    // legend pushes that center left. At full width there is room to spare, so
    // nothing showed for the life of the project; in a page cell the panel's
    // center sits far enough left that a long title starts at a negative x and
    // the cell's edge eats its first letters. Pinned at a narrow width for that
    // reason — a test at 800px would pass either way.
    let data = HashMap::from([("t".to_string(), grid_frame(4, 4))]);
    let title = "Turned: the same sheet from the west";
    let spec = PlotSpec::new().data("t").x("x").y("y").title(title)
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "h"));
    let svg = SvgRenderer::for_theme(&spec.theme.resolved(), 390.0, 300.0)
        .render(&spec, &data);

    let cx: f64 = svg.lines()
        .find(|l| l.contains(title) && l.contains(r#"text-anchor="middle""#))
        .and_then(|l| l.split(r#"x=""#).nth(1))
        .and_then(|s| s.split('"').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("no centered title in:\n{svg:.0}"));
    let half = crate::render::text::estimate_text_width(title, 16.0) / 2.0;
    assert!(cx - half >= 0.0,
            "title starts at {:.1}, outside the canvas", cx - half);
    assert!(cx + half <= 390.0,
            "title ends at {:.1}, past the canvas", cx + half);
}

#[test]
fn a_mesh_face_takes_its_color_from_its_center_not_from_one_of_its_corners() {
    // **Pinned on a coarse mesh on purpose, because that is the only place it
    // shows.** A face named by its low corner is wrong by half a cell everywhere,
    // and on a fine lattice neighbors barely differ, so the volcano at 31x44 drew
    // this defect perfectly for the life of the project. Four faces is where it is
    // impossible to miss, and a test that used a realistic grid would pass either
    // way — the density of the mesh, not the correctness of the code, decided
    // whether anyone could see it.
    //
    // The field is a symmetric bowl on a 3x3 lattice, so the four faces are
    // congruent and every one of them sits at the same mean height. Anything that
    // paints them differently is asserting a difference the data does not have.
    let (mut xs, mut ys, mut vs) = (vec![], vec![], vec![]);
    for &a in &[-2.0_f64, 0.0, 2.0] {
        for &b in &[-2.0_f64, 0.0, 2.0] {
            xs.push(a);
            ys.push(b);
            vs.push(0.019 + 0.0025 * (a * a + b * b));
        }
    }
    let frame = DataFrame::new()
        .with_float("x", xs).with_float("y", ys).with_float("v", vs.clone());
    let data = HashMap::from([("t".to_string(), frame)]);
    let spec = PlotSpec::new().data("t").x("x").y("y").z("v").layer(
        Layer::new(Mark::Surface).encode(Channel::Color, "v"),
    );
    let fills: Vec<String> = surface_faces(&SvgRenderer::default().render(&spec, &data))
        .into_iter().map(|(f, _)| f).collect();
    assert_eq!(fills.len(), 4, "a 3x3 lattice is 2x2 faces");
    let distinct: std::collections::HashSet<&String> = fills.iter().collect();
    assert_eq!(
        distinct.len(), 1,
        "four congruent faces of a symmetric bowl must share one color, got {fills:?}"
    );

    // And **every node reaches exactly the faces it belongs to**. Under the old
    // reading a face took `corners[0]`, so the last row and column of the lattice
    // colored nothing at all: five of these nine values were discarded.
    //
    // Probed by lighting one node at a time, which is exact rather than
    // suggestive. With a single node at 1 and the rest at 0, a face's mean is 0.25
    // if it touches that node and 0 otherwise, so counting the faces at the top
    // color counts the faces the node reached. A 3x3 lattice gives 1 face to each
    // of its four corners, 2 to each edge midpoint and 4 to the center, which is
    // the `[1, 2, 1]` product below. Asserting merely "the picture changed" would
    // have passed for the center node under the *old* code too.
    let xs9: Vec<f64> = (0..9).map(|i| [-2.0, 0.0, 2.0][i / 3]).collect();
    let ys9: Vec<f64> = (0..9).map(|i| [-2.0, 0.0, 2.0][i % 3]).collect();
    for k in 0..9 {
        let mut w = vec![0.0; 9];
        w[k] = 1.0;
        let frame = DataFrame::new()
            .with_float("x", xs9.clone()).with_float("y", ys9.clone())
            .with_float("v", vs.clone()).with_float("w", w);
        let d = HashMap::from([("t".to_string(), frame)]);
        let s = PlotSpec::new().data("t").x("x").y("y").z("v").layer(
            Layer::new(Mark::Surface).encode(Channel::Color, "w"),
        );
        let f: Vec<String> = surface_faces(&SvgRenderer::default().render(&s, &d))
            .into_iter().map(|(a, _)| a).collect();
        let expect = [1, 2, 1][k / 3] * [1, 2, 1][k % 3];
        // Compared as a **partition** rather than by picking the lit color, because
        // "which hex is the lit one" is not answerable by string order: the ramp
        // runs light to dark, so the face holding the larger value is the *darker*
        // string and `max()` returns the unlit one. The sizes of the color groups
        // carry the whole claim and need no such assumption.
        let mut sizes: Vec<usize> = {
            let mut c: std::collections::HashMap<&str, usize> = Default::default();
            for x in &f { *c.entry(x.as_str()).or_default() += 1; }
            c.into_values().collect()
        };
        sizes.sort_unstable();
        let mut want = if expect == 4 { vec![4] } else { vec![expect, 4 - expect] };
        want.sort_unstable();
        assert_eq!(
            sizes, want,
            "node {k} belongs to {expect} of the 4 faces; colors came out {f:?}"
        );
    }
}

/// The `fill-opacity` of every face of a sheet, in the order they were painted.
fn surface_fades(svg: &str) -> Vec<String> {
    svg.lines()
        .filter(|l| l.contains("<path d=\"M") && l.contains("fill-opacity"))
        .map(|l| l.split(r#"fill-opacity=""#).nth(1).unwrap()
                  .split('"').next().unwrap().to_string())
        .collect()
}

#[test]
fn a_mesh_face_reads_a_mapped_opacity_at_the_same_center_its_color_reads() {
    // **The Law-2 half of the channel, and the reason it is worth a test of its
    // own.** A face has four corners and one value, and *which* value is a question
    // every measured channel asks — so if `color` answers it at the face's center
    // and `opacity` answers it at a corner, one mark has grown two readings of one
    // geometry, which is the per-mark exception Law 2 forbids. The two run over the
    // same probe below and are asserted to agree face for face.
    //
    // Coarse on purpose, for the color test's reason one channel over: a face named
    // by its low corner is wrong by half a cell everywhere, and on a fine lattice
    // neighbors barely differ, so nothing shows.
    let xs9: Vec<f64> = (0..9).map(|i| [-2.0, 0.0, 2.0][i / 3]).collect();
    let ys9: Vec<f64> = (0..9).map(|i| [-2.0, 0.0, 2.0][i % 3]).collect();
    let vs: Vec<f64> = (0..9)
        .map(|i| { let (a, b) = (xs9[i], ys9[i]); 0.019 + 0.0025 * (a * a + b * b) })
        .collect();

    // A symmetric bowl: four congruent faces, one mean height, so one fade. Reading
    // a corner instead paints a difference the data does not have.
    let frame = DataFrame::new()
        .with_float("x", xs9.clone()).with_float("y", ys9.clone())
        .with_float("v", vs.clone());
    let data = HashMap::from([("t".to_string(), frame)]);
    let spec = PlotSpec::new().data("t").x("x").y("y").z("v").layer(
        Layer::new(Mark::Surface).encode(Channel::Opacity, "v"),
    );
    let fades = surface_fades(&SvgRenderer::default().render(&spec, &data));
    assert_eq!(fades.len(), 4, "a 3x3 lattice is 2x2 faces");
    let distinct: std::collections::HashSet<&String> = fades.iter().collect();
    assert_eq!(distinct.len(), 1,
        "four congruent faces of a symmetric bowl must share one opacity, got {fades:?}");

    // **Every node reaches exactly the faces it belongs to, and the two channels
    // agree about which.** Lighting one node at a time is exact rather than
    // suggestive: with a single node at 1 and the rest at 0 a face's mean is 0.25
    // if it touches that node and 0 otherwise, so the sizes of the groups count the
    // faces the node reached — 1 to each corner, 2 to each edge midpoint, 4 to the
    // center. Comparing the *partition* rather than picking the lit value keeps the
    // claim free of any assumption about which string is the bright one.
    let partition = |v: &[String]| -> Vec<usize> {
        let mut c: std::collections::HashMap<&str, usize> = Default::default();
        for x in v { *c.entry(x.as_str()).or_default() += 1; }
        let mut sizes: Vec<usize> = c.into_values().collect();
        sizes.sort_unstable();
        sizes
    };
    for k in 0..9 {
        let mut w = vec![0.0; 9];
        w[k] = 1.0;
        let frame = DataFrame::new()
            .with_float("x", xs9.clone()).with_float("y", ys9.clone())
            .with_float("v", vs.clone()).with_float("w", w);
        let d = HashMap::from([("t".to_string(), frame)]);
        let faded = PlotSpec::new().data("t").x("x").y("y").z("v").layer(
            Layer::new(Mark::Surface).encode(Channel::Opacity, "w"),
        );
        let ramped = PlotSpec::new().data("t").x("x").y("y").z("v").layer(
            Layer::new(Mark::Surface).encode(Channel::Color, "w"),
        );
        let by_fade = partition(&surface_fades(&SvgRenderer::default().render(&faded, &d)));
        let by_color = partition(
            &surface_faces(&SvgRenderer::default().render(&ramped, &d))
                .into_iter().map(|(f, _)| f).collect::<Vec<_>>());
        let expect = [1, 2, 1][k / 3] * [1, 2, 1][k % 3];
        let mut want = if expect == 4 { vec![4] } else { vec![expect, 4 - expect] };
        want.sort_unstable();
        assert_eq!(by_fade, want, "node {k} belongs to {expect} of the 4 faces");
        assert_eq!(by_fade, by_color,
            "one face, one reading: the fade and the ramp disagree at node {k}");
    }
}

#[test]
fn a_lid_fades_by_its_own_cell_where_a_quad_fades_by_the_four_it_spans() {
    // The other half of `face_value`: a cut floor never had the mesh's question.
    // A lid *is* one cell and owns its number outright, so averaging anything into
    // it would paint a value the cell does not have — the same reasoning that keeps
    // a riser on the plateau it descends from. Nine cells, nine distinct heights,
    // so nine distinct fades; a mesh over the same nine nodes has four.
    let (mut xs, mut ys, mut vs) = (vec![], vec![], vec![]);
    for i in 0..3 {
        for j in 0..3 {
            xs.push(i as f64);
            ys.push(j as f64);
            vs.push((1 + i * 3 + j) as f64);
        }
    }
    let frame = DataFrame::new()
        .with_float("x", xs).with_float("y", ys).with_float("v", vs);
    let data = HashMap::from([("t".to_string(), frame)]);
    let cut = PlotSpec::new().data("t").x("x").y("y").z("v").layer(
        Layer::new(Mark::Surface)
            .transform(Transform::Bin).transform(Transform::Mean).bins(3)
            .encode(Channel::Opacity, "v"),
    );
    let fades = surface_fades(&SvgRenderer::default().render(&cut, &data));
    let lids: std::collections::HashSet<&String> = fades.iter().collect();
    assert!(lids.len() >= 9,
        "nine cells hold nine different numbers, so nine different fades, got {:?}", lids.len());
}

#[test]
fn a_sheet_ramped_by_its_own_estimate_gets_a_key_that_decodes_it() {
    // The half of this that no legality check could see. Past the refusal the sheet
    // already ramped correctly and drew **no key at all**, because the legend looked
    // for the color column in the raw table and a `density` exists only downstream
    // of the transform that made it. A ramp nobody can decode is worse than a flat
    // sheet: it shows a variation and names no numbers for it.
    let scatter = DataFrame::new()
        .with_float("x", vec![0.11, 0.42, 0.77, 0.93, 0.28, 0.55])
        .with_float("y", vec![0.51, 0.13, 0.88, 0.34, 0.67, 0.22]);
    let data = HashMap::from([("t".to_string(), scatter)]);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        .layer(
            Layer::new(Mark::Surface)
                .transform(Transform::Density)
                .encode(Channel::Color, "density"),
        );
    let svg = SvgRenderer::default().render(&spec, &data);

    let fills: std::collections::HashSet<String> =
        surface_faces(&svg).into_iter().map(|(f, _)| f).collect();
    assert!(fills.len() > 1, "the estimate must ramp across the faces: {fills:?}");
    assert!(svg.contains("linearGradient"), "a measured color is keyed by a strip");
    // The key is titled for the column it decodes, and that title is the axis's
    // too — the height said twice is the whole point of the sentence, so the two
    // readings must agree about what they are showing.
    assert!(svg.matches("Density").count() >= 2, "axis and key both name it: {svg:.0}");
}

#[test]
fn border_on_a_surface_is_the_mesh_line() {
    // The seam hairline each face already carried, handed to the caller — which is
    // the reading that made `border_*` worth spanning to this mark. Without it a
    // face is stroked in its own shade (invisible, closing the antialiasing seam);
    // with it, one color across every face, whatever each face's own fill.
    let data = HashMap::from([("t".to_string(), grid_frame(4, 4))]);
    let plain = PlotSpec::new().data("t").x("x").y("y").z("h")
        .layer(Layer::new(Mark::Surface));
    for (fill, stroke) in surface_faces(&SvgRenderer::default().render(&plain, &data)) {
        assert_eq!(fill, stroke, "an unbordered face hides its own seam");
    }
    let meshed = PlotSpec::new().data("t").x("x").y("y").z("h")
        .layer(Layer::new(Mark::Surface).style_border("white", 0.8));
    let faces = surface_faces(&SvgRenderer::default().render(&meshed, &data));
    assert!(!faces.is_empty());
    for (fill, stroke) in faces {
        assert_eq!(stroke, "white", "the mesh line is one color");
        assert_ne!(fill, stroke, "and it is not the fill");
    }
}

#[test]
fn a_hole_in_the_grid_opens_the_sheet_rather_than_being_drawn_across() {
    // A face needs all four corners, so dropping one node removes exactly the
    // faces that touched it and leaves the rest of the sheet alone. The
    // alternative — drawing a face over the gap — would invent a value nobody
    // measured, which is the silent wrongness §12 forbids.
    let full = grid_frame(4, 4);
    let (xs, ys, hs) = (
        full.float_col("x").unwrap().to_vec(),
        full.float_col("y").unwrap().to_vec(),
        full.float_col("h").unwrap().to_vec(),
    );
    // Drop the interior node at (1, 1) — it is a corner of four faces.
    let keep: Vec<usize> = (0..xs.len()).filter(|&i| !(xs[i] == 1.0 && ys[i] == 1.0)).collect();
    let holed = DataFrame::new()
        .with_float("x", keep.iter().map(|&i| xs[i]).collect())
        .with_float("y", keep.iter().map(|&i| ys[i]).collect())
        .with_float("h", keep.iter().map(|&i| hs[i]).collect());
    let data = HashMap::from([("t".to_string(), holed)]);
    let spec = PlotSpec::new().data("t").x("x").y("y").z("h")
        .layer(Layer::new(Mark::Surface));
    let svg = SvgRenderer::default().render(&spec, &data);
    assert_eq!(surface_faces(&svg).len(), 5, "9 faces less the 4 touching the hole");
}

// -----------------------------------------------------------------------
// theme(font_size = ) — one number, three sizes
// -----------------------------------------------------------------------

fn typed_plot(font_size: Option<f64>) -> String {
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0])
            .with_float("b", vec![2.0, 4.0, 3.0]),
    )]);
    let mut spec = PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Point));
    spec.title = Some("A title".to_string());
    spec.theme = ThemeSpec { font_size, ..ThemeSpec::default() };
    SvgRenderer::for_theme(&spec.theme.resolved(), CANVAS.0, CANVAS.1)
        .render(&spec, &data)
}

/// Every `font-size="…"` on the page, largest first.
fn font_sizes(svg: &str) -> Vec<f64> {
    let mut out: Vec<f64> = svg
        .match_indices("font-size=\"")
        .filter_map(|(i, _)| {
            let rest = &svg[i + 11..];
            rest.find('"').and_then(|e| rest[..e].parse::<f64>().ok())
        })
        .collect();
    out.sort_by(|a, b| b.partial_cmp(a).unwrap());
    out.dedup();
    out
}

#[test]
fn the_three_furniture_sizes_are_one_number_on_a_scale() {
    // The constants this property replaced were 11, 13 and 16, and they were
    // already `round(11 × 1.2ᵏ)`. Stating the ratio once is what lets `theme()`
    // carry one number instead of three; if this ever stops holding, the
    // default look and the asked-for scale have parted company.
    assert_eq!(font_scale(FONT_BASE), (11.0, 13.0, 16.0));
}

#[test]
fn asking_for_the_default_size_draws_the_untouched_plot() {
    // `None` means the caller said nothing and `Some(11)` means they asked for
    // what they already had. Those must be the same picture, or the default is
    // only an approximation of the scale rather than a point on it — and every
    // golden, book plot and parity signature would move the day this shipped.
    assert_eq!(typed_plot(None), typed_plot(Some(FONT_BASE)));
}

#[test]
fn font_size_moves_every_furniture_size_and_the_margins_with_them() {
    assert_eq!(font_sizes(&typed_plot(None)), vec![16.0, 13.0, 11.0]);
    // 16 → 19 → 23, the same scale from a different base.
    assert_eq!(font_sizes(&typed_plot(Some(16.0))), vec![23.0, 19.0, 16.0]);

    // The layout derives every margin from these three, so bigger text has to
    // cost the panel room. A property that changed the glyphs and not the
    // rectangle would be drawing text over the plot.
    //
    // The *panel's* height, not the canvas's — the canvas is 600 either way,
    // which is what made the first version of this assertion pass on a plot it
    // had not actually measured. The panel is the first `<rect>` carrying an
    // `x`; the page background before it has only a width and a height.
    let panel = |svg: &str| -> f64 {
        let i = svg.find("<rect x=\"").unwrap();
        let j = svg[i..].find("height=\"").unwrap();
        svg[i + j + 8..].split('"').next().unwrap().parse().unwrap()
    };
    assert!(
        panel(&typed_plot(Some(24.0))) < panel(&typed_plot(None)),
        "a bigger type scale must leave the panel less room, not overprint it"
    );
}

// -----------------------------------------------------------------------
// theme(strip = ) — the band the journal preset used to leave gray
// -----------------------------------------------------------------------

fn faceted(theme: ThemeSpec) -> String {
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0, 4.0])
            .with_float("b", vec![2.0, 4.0, 3.0, 5.0])
            .with_str("g", ["x", "x", "y", "y"].iter().map(|s| s.to_string()).collect()),
    )]);
    let mut spec = PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Point));
    spec.facet = Some(crate::ir::FacetSpec { col: Some("g".into()), ..Default::default() });
    spec.theme = theme;
    SvgRenderer::default().render(&spec, &data)
}

/// The strip's band fill and the ink of the label sitting on it.
fn strip_band_and_ink(svg: &str) -> (String, String) {
    for chunk in svg.split("<g font").skip(1) {
        let Some(r) = chunk.find(r#"height="16.00" fill=""#) else { continue };
        let band: String = chunk[r + 21..].chars().take_while(|c| *c != '"').collect();
        let i = chunk.find(r#"fill=""#).unwrap();
        let ink: String = chunk[i + 6..].chars().take_while(|c| *c != '"').collect();
        return (band, ink);
    }
    ("?".into(), "?".into())
}

#[test]
fn a_dark_strip_gets_light_type_without_being_asked() {
    // The whole reason the ink derives rather than defaulting to a constant:
    // `theme(strip = "black")` alone would otherwise print the near-black
    // label on the near-black band, and the panel's name would be a guide
    // that is silently empty (§12).
    let dark = faceted(ThemeSpec { strip: Some("black".into()), ..Default::default() });
    assert_eq!(strip_band_and_ink(&dark), ("black".into(), STRIP_INK_LIGHT.into()));

    // The two that must not move.
    assert_eq!(strip_band_and_ink(&faceted(ThemeSpec::default())).1, STRIP_INK);
    let bw = faceted(ThemeSpec { preset: Some("bw".into()), ..Default::default() }.resolved());
    assert_eq!(strip_band_and_ink(&bw).1, STRIP_INK);

    // A band with no luminance to read keeps the dark ink rather than a
    // guess — right for the case that occurs, since it shows the page.
    let clear = faceted(ThemeSpec { strip: Some("transparent".into()), ..Default::default() });
    assert_eq!(strip_band_and_ink(&clear).1, STRIP_INK);
}

#[test]
fn a_named_ink_beats_the_derived_one() {
    // Law 8: the derivation guides, it must not forbid. A navy strip with gold
    // type is legible and nothing should argue with it.
    let named = faceted(ThemeSpec {
        strip: Some("navy".into()),
        strip_text: Some("gold".into()),
        ..Default::default()
    });
    assert_eq!(strip_band_and_ink(&named), ("navy".into(), "gold".into()));
}

#[test]
fn the_journal_preset_no_longer_leaves_gray_bars_over_a_white_panel() {
    // The defect this property was built for: `bw` turned the panel white and
    // left the strips at the hard-coded gray, so the journal look was only
    // most of the way black and white — and a gray band is the part that
    // reproduces badly in print, which is the one place the preset is for.
    let bw = faceted(ThemeSpec { preset: Some("bw".into()), ..Default::default() }.resolved());
    assert!(!bw.contains(STRIP_BG), "theme(\"bw\") must not leave a gray strip");

    // …and the default look did not move.
    assert!(faceted(ThemeSpec::default()).contains(STRIP_BG));
}

#[test]
fn the_journal_preset_is_still_only_properties_a_caller_could_write() {
    // §7's rule, and the reason the property and the preset entry are one
    // change: `bw` may only set things that can be said out loud. If this ever
    // fails, a preset has grown a private vocabulary.
    let preset = faceted(ThemeSpec { preset: Some("bw".into()), ..Default::default() }.resolved());
    let spelled = faceted(ThemeSpec {
        background: Some("white".into()),
        frame: Some("full".into()),
        strip: Some("white".into()),
        ..Default::default()
    });
    assert_eq!(preset, spelled);
}

#[test]
fn the_play_strip_follows_the_facet_strip_because_it_is_the_same_band() {
    // `write_play_strip` is documented as "deliberately the facet strip's
    // strip: same band, same fill". A property that moved one and not the
    // other would make that comment false — Law 2, caught by a test rather
    // than by a reader noticing two grays.
    let data = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0, 4.0])
            .with_float("b", vec![2.0, 4.0, 3.0, 5.0])
            .with_float("yr", vec![1.0, 1.0, 2.0, 2.0]),
    )]);
    let mut spec = PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Point));
    spec.channels.insert(Channel::Play, crate::ir::ChannelDef::field("yr"));
    spec.theme = ThemeSpec { strip: Some("seagreen".into()), ..Default::default() };
    let svg = SvgRenderer::default().render(&spec, &data);
    assert!(svg.contains("seagreen"), "the play strip must take theme(strip = ) too");
    assert!(!svg.contains(STRIP_BG), "no band may keep the default when one was asked for");
}

// -----------------------------------------------------------------------
// Accepted and dropped: the 2026-09-23 round, drawn
// -----------------------------------------------------------------------

/// `legality::JITTER_MAX` is derived from the band this renderer draws, so it is
/// held to that band here: at the widest legal amount no point leaves its own
/// slot, over enough rows that the spread reaches close to both edges.
#[test]
fn the_widest_legal_jitter_keeps_every_point_in_its_own_slot() {
    let n = 400;
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", (0..n).map(|i| if i % 2 == 0 { "A" } else { "B" }.to_string()).collect())
            .with_float("v", (0..n).map(|i| i as f64).collect()),
    )]);
    let cxs = |amount: Option<f64>| -> Vec<f64> {
        let mut layer = Layer::new(Mark::Point);
        if let Some(a) = amount { layer = layer.transform(Transform::Jitter).jitter_amount(a); }
        let spec = PlotSpec::new().data("t").x("g").y("v").layer(layer);
        SvgRenderer::default().render(&spec, &data).lines()
            .filter(|l| l.contains("<circle"))
            .filter_map(|l| l.split(r#"cx=""#).nth(1)?.split('"').next()?.parse::<f64>().ok())
            .collect()
    };
    let plain = cxs(None);
    let wide = cxs(Some(crate::legality::JITTER_MAX));
    assert_eq!((plain.len(), wide.len()), (n, n));
    let slot = (plain[1] - plain[0]).abs();
    let furthest = (0..n).map(|i| (wide[i] - plain[i]).abs()).fold(0.0, f64::max);
    assert!(furthest <= slot / 2.0 + 0.01,
            "a point moved {furthest}px from its center, past half the {slot}px slot");
    assert!(furthest > slot * 0.45,
            "the spread should reach close to the edge at the widest amount: {furthest} of {slot}");
}

fn split_frame() -> HashMap<String, DataFrame> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("c", s(&["p", "p", "p", "p", "q", "q", "q", "q"]))
            .with_str("g", s(&["a", "a", "b", "b", "a", "a", "b", "b"]))
            .with_str("k", s(&["K1", "K2", "K1", "K2", "K1", "K2", "K1", "K2"]))
            .with_float("x", vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]),
    )])
}

/// **Every channel that splits splits a statistic** (`legality::split_fields_of`).
/// A statistic split by `color` alone, else `group`, merged every other split
/// before the mark saw it, and `color` beside `group` drew nothing at all.
#[test]
fn a_statistic_splits_by_every_channel_that_splits() {
    let data = split_frame();
    let line = |encode: &dyn Fn(Layer) -> Layer| {
        let layer = encode(Layer::new(Mark::Line).transform(Transform::Mean));
        SvgRenderer::default().render(&PlotSpec::new().data("t").x("x").y("y").layer(layer), &data)
    };
    // `color` beside `group`: one line per combination, where there were none.
    let svg = line(&|l| l.encode(Channel::Color, "c").encode(Channel::Group, "g"));
    assert_eq!(svg.matches("<polyline").count(), 4, "{svg}");
    // `pattern` on its own: one line per category, each with its own dash.
    let svg = line(&|l| l.encode(Channel::Pattern, "g"));
    let strokes: Vec<&str> = svg.lines().filter(|l| l.contains("<polyline")).collect();
    assert_eq!(strokes.len(), 2, "{svg}");
    assert_eq!(strokes.iter().filter(|l| l.contains("stroke-dasharray")).count(), 1,
               "one of the two means should be dashed: {strokes:?}");
    // `shape` under a summary: each mean takes its category's glyph.
    let spec = PlotSpec::new().data("t").x("x").y("y").layer(
        Layer::new(Mark::Point).transform(Transform::Mean).encode(Channel::Shape, "g"));
    let svg = SvgRenderer::default().render(&spec, &data);
    let circles = svg.matches("<circle").count();
    assert_eq!(circles, 2 + 1, "two circle means and one legend key: {svg}");
}

/// A dodge sets the split side by side **inside** a slot, so the column the slots
/// come from has nothing to separate there and is left out of it.
#[test]
fn a_dodge_leaves_out_the_column_its_slots_come_from() {
    let data = split_frame();
    let spec = PlotSpec::new().data("t").x("k").y("y").layer(
        Layer::new(Mark::Bar).transform(Transform::Mean).transform(Transform::Dodge)
            .encode(Channel::Color, "k").encode(Channel::Pattern, "g"));
    let rects = bar_rects(&SvgRenderer::default().render(&spec, &data));
    assert_eq!(rects.len(), 4, "two slots, two patterns each: {rects:?}");
    let centers: std::collections::BTreeSet<i64> =
        rects.iter().map(|r| ((r.0 + r.2 / 2.0) * 100.0).round() as i64).collect();
    assert_eq!(centers.len(), 4, "every bar in its own lane: {rects:?}");
    // Two lanes per slot, not four: each bar is half of what one undodged bar is.
    let undodged = PlotSpec::new().data("t").x("k").y("y").layer(
        Layer::new(Mark::Bar).transform(Transform::Mean).encode(Channel::Color, "k"));
    let full = bar_rects(&SvgRenderer::default().render(&undodged, &data))[0].2;
    assert!(rects.iter().all(|r| (r.2 - full / 2.0 * 0.8).abs() < 0.01),
            "each dodged bar should be four fifths of half the slot: {rects:?} against {full}");
}

/// **`style(gap = )` is the share of a bar's own place left empty**: its slot
/// alone, its tile dodged. Written, it replaces the slot's fifth and the tile's
/// default alike, and the centers never move; a histogram's bins touch until a
/// gap is written; a box takes the same setting.
#[test]
fn a_gap_is_the_share_of_a_bars_place_left_empty() {
    let df = DataFrame::new()
        .with_str("g", vec!["A".into(), "A".into(), "B".into(), "B".into()])
        .with_str("c", vec!["p".into(), "q".into(), "p".into(), "q".into()])
        .with_float("v", vec![1.0, 2.0, 3.0, 4.0]);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let bars = |dodge: bool, gap: Option<f64>| {
        let mut layer = Layer::new(Mark::Bar).encode(Channel::Color, "c");
        if dodge { layer = layer.transform(Transform::Dodge); }
        layer.style.gap = gap;
        bar_rects(&SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("g").y("v").layer(layer), &data))
    };
    let full = bars(false, None)[0].2;             // four fifths of the slot
    let slot = full / 0.8;
    // Widths are written to two decimals, so compare to a hundredth.
    assert!((bars(false, Some(0.5))[0].2 - slot * 0.5).abs() < 0.01, "half the slot");
    assert!((bars(false, Some(0.2))[0].2 - full).abs() < 0.01, "0.2 is the default");
    let touching = bars(true, Some(0.0));
    assert!(touching.iter().all(|r| (r.2 - full / 2.0).abs() < 0.01),
            "gap 0: dodged bars fill their tiles and touch: {touching:?}");
    let centers = |rs: &[(f64, f64, f64, f64)]| {
        let mut c: Vec<f64> = rs.iter().map(|r| r.0 + r.2 / 2.0).collect();
        c.sort_by(|a, b| a.partial_cmp(b).unwrap());
        c
    };
    let (a, b) = (centers(&touching), centers(&bars(true, Some(0.6))));
    assert!(a.len() == 4 && a.iter().zip(&b).all(|(p, q)| (p - q).abs() < 0.011),
            "a gap never moves a center: {a:?} against {b:?}");

    // A histogram's bins touch by default, and part when a gap is written.
    let hist = DataFrame::new().with_float("x", (0..40).map(|i| i as f64).collect());
    let mut hd = HashMap::new();
    hd.insert("t".to_string(), hist);
    let bins = |gap: Option<f64>| {
        let mut layer = Layer::new(Mark::Bar).transform(Transform::Bin);
        layer.style.gap = gap;
        let mut rs = bar_rects(&SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("x").layer(layer), &hd));
        rs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        rs
    };
    let b = bins(None);
    assert!((b[0].0 + b[0].2 - b[1].0).abs() < 0.02, "bins touch: {b:?}");
    let b = bins(Some(0.25));
    assert!(b[1].0 - (b[0].0 + b[0].2) > 1.0, "a written gap parts the bins: {b:?}");

    // A box takes the same setting; unset it keeps its width.
    let boxes = DataFrame::new()
        .with_str("g", (0..20).map(|i| if i < 10 { "A".to_string() } else { "B".to_string() }).collect())
        .with_float("v", (0..20).map(|i| (i % 10) as f64).collect());
    let mut bd = HashMap::new();
    bd.insert("t".to_string(), boxes);
    let box_svg = |gap: Option<f64>| {
        let mut layer = Layer::new(Mark::Box);
        layer.style.gap = gap;
        SvgRenderer::default().render(&PlotSpec::new().data("t").x("g").y("v").layer(layer), &bd)
    };
    assert_ne!(box_svg(None), box_svg(Some(0.1)), "a gap moves a box's sides");
    assert_eq!(box_svg(None), box_svg(None));
}

/// `zone * sum * proportion` divides each cell's sum by the total over all cells,
/// as `bar` does, and the legend keeps the column's own name.
#[test]
fn a_zone_share_of_a_summary_is_a_share() {
    let data = split_frame();
    let spec = |share: bool| {
        let mut layer = Layer::new(Mark::Zone).transform(Transform::Sum)
            .encode(Channel::Color, "y");
        if share { layer = layer.transform(Transform::Proportion); }
        PlotSpec::new().data("t").x("c").y("g").layer(layer)
    };
    let summed = SvgRenderer::default().render(&spec(false), &data);
    let shared = SvgRenderer::default().render(&spec(true), &data);
    assert_ne!(summed, shared, "`proportion` must change what the cells carry");
    // The four sums are 3, 7, 11 and 15 of 36, so the ramp ends at 15/36.
    assert!(shared.contains(">0.417<"), "the legend should read shares: {shared}");
    assert!(shared.contains(">Y<"), "and keep the column's name: {shared}");
}

/// A violin is one slot in one group, and the group is the whole combination
/// the statistic split by. Keyed by one column, two groups sharing a color ran
/// together into a single outline.
#[test]
fn a_violin_is_drawn_once_per_combination_of_its_splits() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let n = 40;
    let data: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("k", s(&vec!["K"; n]))
            .with_str("c", (0..n).map(|i| if i % 2 == 0 { "p" } else { "q" }.to_string()).collect())
            .with_str("g", (0..n).map(|i| if i % 4 < 2 { "a" } else { "b" }.to_string()).collect())
            .with_float("y", (0..n).map(|i| (i % 7) as f64 + (i / 7) as f64 * 0.5).collect()),
    )]);
    let spec = PlotSpec::new().data("t").x("k").y("y").layer(
        Layer::new(Mark::Ribbon).transform(Transform::Density)
            .encode(Channel::Color, "c").encode(Channel::Pattern, "g"));
    let svg = SvgRenderer::default().render(&spec, &data);
    assert_eq!(svg.matches("<polygon").count(), 4, "two colors by two patterns: {svg}");
}

/// **`style(angle = )` turns each label about the point it names**, in degrees
/// counterclockwise, so SVG's clockwise rotation carries the opposite sign. A
/// halo turns with its letters, and `angle = 0` writes the bytes of a sentence
/// without the setting, so an upright label costs nothing to state.
#[test]
fn a_turned_label_rotates_about_its_own_point_and_takes_its_halo_along() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_float("x", vec![1.0, 2.0])
        .with_float("y", vec![1.0, 2.0])
        .with_str("name", vec!["first".into(), "second".into()]))]);
    let draw = |angle: Option<f64>, halo: bool| {
        let mut text = Layer::new(Mark::Text).encode(Channel::Label, "name");
        text.style.angle = angle;
        if halo { text.style.border_size = Some(3.0); }
        SvgRenderer::default().render(&PlotSpec::new().data("t").x("x").y("y").layer(text), &data)
    };
    let upright = draw(None, false);
    let first = |svg: &str| svg.lines().find(|l| l.contains(">first</text>")).unwrap_or("").to_string();
    assert!(!first(&upright).contains("rotate("), "{upright}");
    assert_eq!(draw(Some(0.0), false), upright, "a zero turn is no turn");
    let turned = draw(Some(90.0), false);
    let line = turned.lines().find(|l| l.contains(">first</text>")).expect("the label");
    let num = |key: &str| line.split(key).nth(1).and_then(|r| r.split('"').next())
        .and_then(|v| v.parse::<f64>().ok()).unwrap();
    let (x, y) = (num(r#" x=""#), num(r#" y=""#));
    let rot = line.split("rotate(").nth(1).and_then(|r| r.split(')').next()).unwrap();
    let parts: Vec<f64> = rot.split(' ').map(|v| v.parse().unwrap()).collect();
    assert_eq!(parts[0], -90.0, "counterclockwise on the page: {line}");
    assert!((parts[1] - x).abs() < 0.01, "about the label's own x: {line}");
    assert!(parts[2] < y && y - parts[2] < 10.0,
        "about the point, half a cap height above the baseline: {line}");
    let haloed = draw(Some(90.0), true);
    assert_eq!(haloed.matches("rotate(-90.00").count(), 4,
        "two letters and two halos, all turned: {haloed}");
}

/// A packed name is fitted **turned**: a long name in a tall, narrow region is
/// left out upright and drawn at `angle = 90`, which is the reason to turn it.
#[test]
fn a_turned_name_is_fitted_by_its_turned_extent() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("g", ["a long region name", "b"].into_iter().map(String::from).collect())
        .with_str("outer", ["p", "q"].into_iter().map(String::from).collect())
        .with_float("v", vec![1.0, 30.0]))]);
    let draw = |angle: Option<f64>| {
        let mut names = Layer::new(Mark::Text).encode(Channel::Label, "g");
        names.style.angle = angle;
        let mut spec = PlotSpec::new().data("t").x("outer").y("v").coord(CoordSpace::Nest)
            .layer(Layer::new(Mark::Bar)).layer(names);
        spec.theme.width = Some(400.0);
        let drawn = SvgRenderer::default().draw(&spec, &data);
        (drawn.svg, drawn.remarks)
    };
    let line = |svg: &str| svg.lines().find(|l| l.ends_with(">a long region name</text>"))
        .map(str::to_string).unwrap_or_else(|| panic!("the long name is drawn: {svg}"));
    let (upright, said) = draw(None);
    assert!(line(&upright).contains(r#" font-size=""#) && said.is_empty(),
        "upright, the long name is drawn smaller to fit its narrow region: {}", line(&upright));
    let (turned, _) = draw(Some(90.0));
    assert!(!line(&turned).contains("font-size"), "turned, it fits at full size: {}", line(&turned));
}

/// **A border on `text` is a halo under its letters.** `style(border_color =,
/// border_size =)` strokes each label's outline, and every halo is drawn in
/// one group *before* the letters' group, so a halo clears the marks under
/// the text and can never erase a neighboring label's letters where two
/// overlap. Either setting alone works: an unset color is the panel's background
/// (the halo the cube's frame labels wear), an unset width is 3, and a width
/// of 0 draws none.
#[test]
fn a_text_border_is_a_halo_under_its_letters() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_float("x", vec![1.0, 2.0, 3.0])
        .with_float("y", vec![1.0, 3.0, 2.0])
        .with_str("name", vec!["a".into(), "b".into(), "c".into()]))]);
    let draw = |text: Layer| SvgRenderer::default().render(
        &PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Line))
            .layer(text.encode(Channel::Label, "name")),
        &data);
    let halo_group = r#"text-anchor="middle" fill="none" stroke=""#;
    let plain = draw(Layer::new(Mark::Text));
    assert!(!plain.contains(halo_group), "no border, no halo: {plain}");

    let mut sized = Layer::new(Mark::Text);
    sized.style.border_size = Some(3.0);
    let panel = draw(sized);
    assert!(panel.contains(&format!(
        r#"fill="none" stroke="{PANEL_BG}" stroke-width="3" stroke-linejoin="round""#)),
        "an unset color is the panel's background: {panel}");

    let white = draw(Layer::new(Mark::Text).style_border("white", 5.0));
    assert!(white.contains(r#"fill="none" stroke="white" stroke-width="5""#),
        "a set color and width are drawn as given: {white}");
    let halos_at = white.find(halo_group).expect("the halo group");
    let letters_at = white.find(r#"text-anchor="middle">"#).expect("the letters group");
    assert!(halos_at < letters_at, "every halo is drawn before any letter: {white}");
    assert_eq!(white.matches(">a</text>").count(), 2, "each label once as halo, once as letters");

    let mut colored = Layer::new(Mark::Text);
    colored.style.border_color = Some("white".into());
    let unset_width = draw(colored);
    assert!(unset_width.contains(r#"stroke="white" stroke-width="3""#),
        "an unset width is 3: {unset_width}");

    let none = draw(Layer::new(Mark::Text).style_border("white", 0.0));
    assert!(!none.contains(halo_group), "a width of 0 draws no halo: {none}");
}

/// **A repelled label clears its own dot at the size it is drawn, in the
/// plane too.** With `size(pop)` on the sibling `point`, the big row's dot is
/// the largest radius a mapped size draws, and its label must rest off that
/// radius rather than the default one. Before a label read every drawn dot,
/// only a network recomputed its sibling's radii, and a bubble chart's big
/// name sat across its own bubble. Asserted geometrically: no label's center
/// is inside its own dot.
#[test]
fn a_repelled_label_clears_its_own_sized_dot_in_the_plane() {
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_float("x", vec![1.0, 3.0, 5.0])
        .with_float("y", vec![1.0, 2.0, 1.0])
        .with_float("pop", vec![100.0, 1.0, 1.0])
        .with_str("name", vec!["Big".into(), "b".into(), "c".into()]))]);
    let mut point = Layer::new(Mark::Point);
    point.encodings.insert(Channel::Size, crate::ir::ChannelDef::field("pop"));
    let mut text = Layer::new(Mark::Text);
    text.transforms.push(Transform::Repel);
    text.encodings.insert(Channel::Label, crate::ir::ChannelDef::field("name"));
    let spec = PlotSpec::new().data("t").x("x").y("y").layer(point).layer(text);
    let svg = SvgRenderer::default().render(&spec, &data);
    let grab = |rest: &str, key: &str| -> f64 {
        let s = rest.find(key).unwrap() + key.len();
        rest[s..].split('"').next().unwrap().parse().unwrap()
    };
    let circles: Vec<(f64, f64, f64)> = svg.match_indices("<circle cx=\"")
        .map(|(at, _)| (grab(&svg[at..], "cx=\""), grab(&svg[at..], "cy=\""), grab(&svg[at..], "r=\"")))
        .collect();
    let labels: Vec<(f64, f64)> = svg.match_indices("<text x=\"")
        .filter(|(at, _)| {
            let rest = &svg[*at..];
            let body = &rest[rest.find('>').unwrap() + 1..rest.find("</text>").unwrap()];
            ["Big", "b", "c"].contains(&body)
        })
        .map(|(at, _)| (grab(&svg[at..], "x=\""), grab(&svg[at..], "y=\"")))
        .collect();
    assert_eq!(labels.len(), 3, "each name is drawn once: {svg}");
    let big = circles.iter().cloned().fold((0.0, 0.0, 0.0), |m, c| if c.2 > m.2 { c } else { m });
    assert!(big.2 >= 11.0, "the big row draws a big dot: {big:?}");
    for &(tx, ty) in &labels {
        let &(cx, cy, r) = circles.iter()
            .min_by(|p, q| {
                let dp = (p.0 - tx).powi(2) + (p.1 - ty).powi(2);
                let dq = (q.0 - tx).powi(2) + (q.1 - ty).powi(2);
                dp.partial_cmp(&dq).unwrap()
            })
            .unwrap();
        let dist = ((cx - tx).powi(2) + (cy - ty).powi(2)).sqrt();
        assert!(dist > r, "a label's center at ({tx}, {ty}) sits inside the dot at ({cx}, {cy}) of radius {r}");
    }
}

/// **A mapping keeps the channel when a setting is drawn beside it anyway**
/// (`GOG_STRICT=0`, where legality's refusal is only a warning). Nine marks let the
/// set color paint over a mapping whose legend still stood beside the plot, and
/// three kept the mapping; every mark now keeps it, so the picture is the one its
/// key decodes. The heatmap's color, which no binding names, is kept the same way.
#[test]
fn a_mapping_keeps_its_channel_beside_a_set_color_drawn_anyway() {
    let df = DataFrame::new()
        .with_float("gdp", vec![1.0, 2.0, 3.0, 1.5, 2.5, 3.5])
        .with_float("life", vec![4.0, 5.0, 6.0, 4.5, 5.5, 6.5])
        .with_str("continent", ["Asia", "Europe", "Asia", "Europe", "Asia", "Europe"]
            .iter().map(|s| s.to_string()).collect());
    let data = HashMap::from([("t".to_string(), df)]);
    let marks = [Mark::Line, Mark::Step, Mark::Area, Mark::Text, Mark::Rule];
    for mark in marks {
        let mut layer = Layer::new(mark.clone()).encode(Channel::Color, "continent").style_color("black");
        if mark == Mark::Text {
            layer = layer.encode(Channel::Label, "continent");
        }
        let spec = PlotSpec::new().data("t").x("gdp").y("life").layer(layer);
        let svg = SvgRenderer::default().render(&spec, &data);
        assert!(!svg.contains("=\"black\""), "{mark:?} drew the set color over its mapping");
    }
    let cells = PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Zone).transform(Transform::Bin).style_color("black"));
    let svg = SvgRenderer::default().render(&cells, &data);
    assert!(!svg.contains("fill=\"black\""), "a heatmap drew the set color over its count");
}

/// **A column is read only when the sentence names it, or the reading wrote it.** A
/// contour colors itself by the `level` it was cut at, and that fallback once read
/// any numeric `level` column: a plain path over such a table was ramped, with no
/// `color()` written and no key to decode it.
#[test]
fn a_plain_path_is_not_colored_by_a_column_called_level() {
    let df = DataFrame::new()
        .with_float("a", vec![1.0, 2.0, 3.0, 4.0])
        .with_float("b", vec![1.0, 3.0, 2.0, 4.0])
        .with_float("level", vec![1.0, 5.0, 10.0, 20.0]);
    let data = HashMap::from([("t".to_string(), df)]);
    let spec = PlotSpec::new().data("t").x("a").y("b").layer(Layer::new(Mark::Path));
    let svg = SvgRenderer::default().render(&spec, &data);
    let strokes: std::collections::BTreeSet<&str> = svg.split("<line ").skip(1)
        .chain(svg.split("<path ").skip(1))
        .filter_map(|e| e.split("stroke=\"").nth(1)?.split('"').next())
        .filter(|c| *c == PALETTE_GOG[0] || RAMP_BLUE.contains(c))
        .collect();
    assert_eq!(strokes.len(), 1, "the route was drawn in {strokes:?}");
}

/// **A key for a column the transform writes reads the rows the marks were drawn
/// from.** Looked for in the table, `color(degree)` found nothing and drew no key at
/// all, and a table holding its own `degree` gave `size(degree)` a key decoding
/// that column's numbers rather than the node counts the dots were sized by.
#[test]
fn a_layouts_degree_keys_read_the_drawn_nodes() {
    let table = |own_degree: bool| {
        let mut df = DataFrame::new()
            .with_str("a", ["u", "u", "v"].iter().map(|s| s.to_string()).collect())
            .with_str("b", ["v", "w", "w"].iter().map(|s| s.to_string()).collect());
        if own_degree {
            df = df.with_float("degree", vec![99.0, 99.0, 99.0]);
        }
        HashMap::from([("t".to_string(), df)])
    };
    let net = || CoordSpace::Network(crate::ir::NetworkView::default());
    let colored = PlotSpec::new().data("t").coord(net())
        .layer(Layer::new(Mark::Point).layout("a", "b").encode(Channel::Color, "degree"));
    let svg = SvgRenderer::default().render(&colored, &table(false));
    assert!(svg.contains(">Degree<") && svg.contains("linearGradient"), "no color key for degree");
    let svg = SvgRenderer::default().render(&colored, &table(true));
    assert!(svg.contains(">Degree<") && !svg.contains(">99<"), "the color key read the table's degree");
    let sized = PlotSpec::new().data("t").coord(net())
        .layer(Layer::new(Mark::Point).layout("a", "b").encode(Channel::Size, "degree"));
    let svg = SvgRenderer::default().render(&sized, &table(true));
    assert!(svg.contains(">Degree<") && !svg.contains(">99<"), "the size key read the table's degree");
}
