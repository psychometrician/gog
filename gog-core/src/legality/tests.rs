use super::*;
use crate::ir::{Layer, PlotSpec};

/// **A cell holds one row** (2026-10-03). Rows that share a cell of a plain
/// zone and disagree on a mapped column would be drawn one on another, the
/// last hiding the rest; refused, with the count, and toward `zone * mean`
/// for a number. Rows that agree hide nothing, rows in different panels are
/// in different cells, and a summarizing transform makes one row per cell.
#[test]
fn a_plain_zone_refuses_a_cell_whose_rows_disagree() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let t = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("a", s(&["p", "p", "q"]))
        .with_str("b", s(&["u", "u", "v"]))
        .with_str("g", s(&["one", "two", "one"]))
        .with_str("same", s(&["k", "k", "k"]))
        .with_float("v", vec![1.0, 2.0, 3.0]))]);
    let tile = |color: &str| PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Zone).encode(Channel::Color, color));
    let refused = |d: &[Diagnostic]| d.iter().find(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("one opaque rectangle per row")).map(|x| x.message.clone());

    let m = refused(&check(&tile("v"), &t)).expect("a number that differs inside a cell");
    assert!(m.contains("1 of its 2 cells holds rows with different `color(v)`")
        && m.contains("1 row would be hidden") && m.contains("`zone * mean + color(v)`"), "{m}");
    let m = refused(&check(&tile("g"), &t)).expect("a category that differs inside a cell");
    assert!(m.contains("`facet(<column>)`"), "{m}");
    assert!(refused(&check(&tile("same"), &t)).is_none(), "rows that agree hide nothing");
    assert!(refused(&check(&tile("v").facet_col("g"), &t)).is_none(),
        "rows in different panels are in different cells");
    let mean = PlotSpec::new().data("t").x("a").y("b")
        .layer(Layer::new(Mark::Zone).transform(Transform::Mean).encode(Channel::Color, "v"));
    assert!(refused(&check(&mean, &t)).is_none(), "`zone * mean` draws one row per cell");
}

fn data() -> HashMap<String, DataFrame> {
    let df = DataFrame::new()
        .with_float("gdp", vec![1.0, 2.0, 3.0])
        .with_float("life", vec![4.0, 5.0, 6.0])
        .with_str(
            "continent",
            vec!["Asia".into(), "Europe".into(), "Africa".into()],
        )
        // A *second* categorical column, added when the tile plot shipped: the
        // rule that a category bounds its axis cannot be stated with one, because
        // a zone on `x(continent) + y(continent)` is a diagonal degenerate case
        // rather than the plot the rule is about.
        .with_str(
            "region",
            vec!["North".into(), "South".into(), "North".into()],
        )
        // A third *numeric* column, added when the terraced sheet shipped: a
        // surface reduces a named column on `z` while both floor axes stay
        // numeric, so it is the first mark needing three distinct numbers.
        .with_float("value", vec![7.0, 8.0, 9.0]);
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);
    m
}

fn base() -> PlotSpec {
    PlotSpec::new().data("t").x("gdp").y("life")
}

fn kinds(d: &[Diagnostic]) -> Vec<DiagnosticKind> {
    d.iter().map(|x| x.kind).collect()
}

// -----------------------------------------------------------------------
// Can a value be turned back into the place it was drawn?
// -----------------------------------------------------------------------

/// The page re-derives positions rather than asking the picture, so it needs
/// to be told when a mark does not stand at its own value. Every one of these
/// moves a row somewhere the value alone does not predict.
#[test]
fn a_moved_mark_is_reported_as_moved() {
    for (t, why) in [
        (Transform::Jitter, "jitter"),
        (Transform::Dodge, "dodge"),
        (Transform::Stack, "stack"),
        (Transform::Mean, "summary"),
        (Transform::Bin, "summary"),
    ] {
        let spec = base().layer(Layer::new(Mark::Point).transform(t.clone()));
        assert_eq!(why_not_placed(&spec), Some(why), "{t:?}");
    }
}

/// A mark that draws one shape through many rows has no single row under a
/// pointer, whatever the transforms say.
#[test]
fn a_mark_spanning_rows_is_reported_too() {
    let spec = base().layer(Layer::new(Mark::Line));
    assert_eq!(why_not_placed(&spec), Some("mark"));
}

/// An untouched mark places its rows, and so does a plot where *one* layer
/// does. A jittered cloud under a `text` layer still has the text standing at
/// each row's value, and a pointer landing there has found something real, so
/// refusing the whole plot would take away an answer that was correct.
#[test]
fn one_honest_layer_is_enough() {
    assert_eq!(why_not_placed(&base().layer(Layer::new(Mark::Point))), None);
    let mixed = base()
        .layer(Layer::new(Mark::Point).transform(Transform::Jitter))
        .layer(Layer::new(Mark::Text));
    assert_eq!(why_not_placed(&mixed), None);
}

/// The messages, for an assertion that fails readably.
fn msgs(d: &[Diagnostic]) -> Vec<String> {
    d.iter().map(|x| x.message.clone()).collect()
}

// -----------------------------------------------------------------------
// Coord — a space with no marks in it at all (spec §15)
// -----------------------------------------------------------------------

/// One minimal, otherwise-clean sentence per space, so the assertion below is
/// about the space rather than about the plot being ill-formed. `Space` binds
/// a `z` because that, not the atom, is what makes a plot three-dimensional.
fn spec_in(space: SpaceKind) -> PlotSpec {
    let s = base().layer(Layer::new(Mark::Point));
    match space {
        SpaceKind::Flat => s,
        SpaceKind::Space => {
            s.z("value").coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        }
        SpaceKind::Polar => s.coord(CoordSpace::Polar(crate::ir::PolarView::default())),
        SpaceKind::Nest => s.coord(CoordSpace::Nest),
        SpaceKind::Globe => s.coord(CoordSpace::Globe(crate::ir::GlobeView::default())),
        SpaceKind::Map => s.coord(CoordSpace::Map(crate::ir::MapView::default())),
        // The network's minimum syllable is the edge, its layout, and the
        // space — a bare point has no place here, which its own gate says.
        SpaceKind::Network => base()
            .layer(Layer::new(Mark::Edge).layout("continent", "region"))
            .coord(CoordSpace::Network(crate::ir::NetworkView::default())),
    }
}

/// The whole `Spaces:` line in one test, read off `mark_draws_in_space` rather
/// than typed out: a space no mark stands in must be refused, and a space some
/// mark stands in must not be refused *here*, its own gate owning that.
///
/// `map` and `globe` were accepted and **drawn flat** for the project's life,
/// and no test could have caught it, because the assertion that would have and
/// the function that would have were the same absence. This one flips by itself
/// the day a space gains its first mark, which is how `map` will arrive.
#[test]
fn a_space_no_mark_stands_in_is_refused_rather_than_drawn_flat() {
    for space in ALL_SPACES {
        let mut out = Vec::new();
        check_coord(&mut out, &spec_in(space));
        let drawn = ALL_MARKS.iter().any(|m| mark_draws_in_space(m, space));
        assert_eq!(
            out.is_empty(),
            drawn,
            "`{}` disagrees with the Mark × Space grid: {:?}",
            space_name(space),
            msgs(&out)
        );
    }
}

/// **No space is left undrawn** — `globe` was the last, refused here until
/// 2026-08-17, and this is the second of the two tests written to flip the
/// day a space gained its first mark (`map` flipped the first). A `point` on
/// the globe now passes the whole checker in silence, which is exactly what
/// the space-level gate stepping aside means; `check_coord` stays in the
/// file as the gate for whatever space comes next.
#[test]
fn every_space_now_draws_and_the_gate_is_silent() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .coord(CoordSpace::Globe(crate::ir::GlobeView::default()));
    let out = check(&spec, &data());
    assert!(
        out.is_empty(),
        "a `point` on the globe should draw without a word: {:?}",
        msgs(&out)
    );
}

/// The refusal a user meets in this space now comes from `check_globe`, and
/// it splits two ways the rule table decides: a mark `map` draws is **not
/// built** here (valid grammar the engine does not draw, Unsupported), and a
/// mark that measures along an axis is **ruled out** for `map`'s one reason
/// (Illegal since 2026-09-29). Both say what to write instead.
#[test]
fn a_globe_blank_says_whether_it_is_not_built_or_ruled_out() {
    // `zone` draws only its boundary form here: a mesh has no equal-area
    // spherical grid to stand on, so a zone without a boundary is not built,
    // with the tiling reason named.
    let owed = base()
        .layer(Layer::new(Mark::Zone))
        .coord(CoordSpace::Globe(crate::ir::GlobeView::default()));
    let out = check(&owed, &data());
    assert!(
        out.iter().any(|d| d.kind == DiagnosticKind::Unsupported
            && d.message.contains("sides from a boundary")),
        "`zone` without a boundary should be not built, with direction: {:?}",
        msgs(&out)
    );

    let ruled = base()
        .layer(Layer::new(Mark::Line))
        .coord(CoordSpace::Globe(crate::ir::GlobeView::default()));
    let out = check(&ruled, &data());
    assert!(
        out.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("measures along an axis")
            && d.message.contains("size(<column>)")),
        "`line` on the globe should be ruled out with direction: {:?}",
        msgs(&out)
    );

    // `bar` is neither: it stands here, and what it needs is its measure.
    // Without `z` the spike has nothing to stand up; with it, the whole
    // checker passes in silence.
    let bare = base()
        .layer(Layer::new(Mark::Bar))
        .coord(CoordSpace::Globe(crate::ir::GlobeView::default()));
    let out = check(&bare, &data());
    assert!(
        out.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("measures along the radius")
            && d.message.contains("z(<column>)")),
        "`bar` without `z` should ask for its measure: {:?}",
        msgs(&out)
    );
    let spike = base()
        .layer(Layer::new(Mark::Bar))
        .z("value")
        .coord(CoordSpace::Globe(crate::ir::GlobeView::default()));
    let out = check(&spike, &data());
    assert!(
        out.is_empty(),
        "a spike — `bar` with `z` on the globe — should draw without a word: {:?}",
        msgs(&out)
    );
}

fn map_base() -> PlotSpec {
    PlotSpec::new()
        .data("t")
        .x("gdp")
        .y("life")
        .coord(CoordSpace::Map(crate::ir::MapView::default()))
}

/// **Every one of these was a silent drop the day `map` started drawing.** A
/// space owes its refusals on the day it draws, not afterwards: while nothing
/// stood in `map`, the space-level gate answered everything, and giving it four
/// marks turned one question into five that the engine answered by drawing
/// something wrong and saying nothing.
#[test]
fn a_map_refuses_what_it_cannot_place_and_says_what_to_write_instead() {
    // A mark that measures along an axis, when both are spent on the place.
    let bars = map_base().layer(Layer::new(Mark::Bar));
    let out = check(&bars, &data());
    assert!(
        out.iter().any(|d| d.message.contains("`bar` measures along an axis")
            && d.message.contains("size(<column>)")),
        "a bar was placed on a map: {:?}", msgs(&out)
    );

    // A map and a cube are two spaces, and a plot is drawn in one.
    let cube = map_base().z("value").layer(Layer::new(Mark::Point));
    let out = check(&cube, &data());
    assert!(
        out.iter().any(|d| d.message.contains("asks for two — a map and a cube")),
        "a bound z was ignored on a map: {:?}", msgs(&out)
    );

    // A category has no longitude.
    let cats = PlotSpec::new()
        .data("t")
        .x("continent")
        .y("life")
        .coord(CoordSpace::Map(crate::ir::MapView::default()))
        .layer(Layer::new(Mark::Point));
    let out = check(&cats, &data());
    assert!(
        out.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("is a category, and a `map()` position is a place")),
        "a category was accepted as a place: {:?}", msgs(&out)
    );

    // The projection owns the panel's shape, so the setting is refused rather
    // than accepted and then overridden.
    let mut squashed = map_base().layer(Layer::new(Mark::Point));
    squashed.theme.ratio = Some(1.0);
    let out = check(&squashed, &data());
    assert!(
        out.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("`theme(ratio = )` shapes a panel")),
        "a ratio silently lost a fight with the projection: {:?}", msgs(&out)
    );
}

/// A clamped row is a **moved** row, so Mercator's cut is reported. Neither
/// dropping it (which would misreport how many places there are) nor moving it
/// in silence is allowed.
#[test]
fn mercator_says_which_rows_it_could_not_reach() {
    let df = DataFrame::new()
        .with_float("lon", vec![0.0, 10.0])
        .with_float("lat", vec![0.0, 89.0]);
    let mut d = HashMap::new();
    d.insert("t".to_string(), df);
    let spec = PlotSpec::new()
        .data("t")
        .x("lon")
        .y("lat")
        .coord(CoordSpace::Map(crate::ir::MapView {
            preserve: crate::ir::Preserve::Angle,
        }))
        .layer(Layer::new(Mark::Point));
    let out = check(&spec, &d);
    assert!(
        out.iter().any(|x| x.kind == DiagnosticKind::Assumption
            && x.message.contains("1 row(s) beyond that")),
        "Mercator moved a place and said nothing: {:?}", msgs(&out)
    );

    // Equal Earth reaches both poles, so the same data earns no such report.
    let ok = spec.clone().coord(CoordSpace::Map(crate::ir::MapView::default()));
    let out = check(&ok, &d);
    assert!(
        !out.iter().any(|x| x.message.contains("row(s) beyond that")),
        "equal earth reported a limit it does not have: {:?}", msgs(&out)
    );
}

/// The other half of the same statement: a space that *has* marks must not be
/// refused by the space-level gate, or the refusal would outlive what it was
/// protecting and no plot in it could ever be drawn.
#[test]
fn a_space_with_marks_in_it_is_not_refused_by_the_space_gate() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .coord(CoordSpace::Map(crate::ir::MapView::default()));
    let mut out = Vec::new();
    check_coord(&mut out, &spec);
    assert!(out.is_empty(), "`map()` is still refused wholesale: {:?}", msgs(&out));
}

// -----------------------------------------------------------------------
// Brush — the selection's refusals (spec §15)
// -----------------------------------------------------------------------

/// **Brushability derives; it is not a second table.** A mark can be brushed
/// when one row is one element rather than one vertex, which is the same
/// question a per-row predicate asks. Written as a correspondence over all
/// thirteen rather than a list, so a mark added later gets its verdict without
/// anyone remembering this file exists.
///
/// **`zone` is the first place the correspondence and the property came apart**,
/// on 2026-08-01, and the property is the one that survived (`bar` and `point`
/// followed on 2026-10-09, taking `group` to split a statistic). Refusing `group`
/// was a faithful proxy for *one row is one element* while every source of a
/// zone's sides described a single row's rectangle — a category's slot,
/// `bounds`, `bin`, `density`. A **boundary** is the fifth, it arrives as many
/// rows per shape, and it needs `group` to say which rows those are. So the
/// proxy broke and the mark stayed brushable: a heatmap cell, a tile and a
/// waterfall step are each still one row.
///
/// What that leaves open, and it belongs to the selection rather than to this
/// test: **brushing a choropleth** would select some of a region's vertices,
/// which is not a thing a reader can mean. It is not reachable by accident —
/// a boundary is only read in `map()` — but it is not refused either.
#[test]
fn a_mark_can_be_brushed_exactly_when_one_row_is_one_element() {
    let brushable: Vec<&str> = ALL_MARKS.iter().filter(|m| mark_takes_selection(m))
        .map(mark_name).collect();
    assert_eq!(brushable, vec!["point", "bar", "text", "rule", "zone"],
        "the brushable set is the marks whose rows are elements");
    for m in &ALL_MARKS {
        // `zone` is exempt for the reason above: it answers both questions, and
        // which one applies is decided by where its sides came from. `bar` and
        // `point` are exempt the same way: they take `group` to split what a
        // statistic gathers, and a bare one is refused, so their rows are
        // still elements; `text` too, whose `group` also splits a `dodge`.
        // `edge` is exempt from the other side: one row, one element, and
        // still no data range for a brush to name — its positions are the
        // layout's.
        if matches!(m, Mark::Zone | Mark::Bar | Mark::Point | Mark::Text | Mark::Edge) {
            continue;
        }
        assert_eq!(mark_takes_selection(m),
            rule_for(m, &Channel::Group).obligation == Obligation::Cannot,
            "`{}` disagrees with its own `group` rule", mark_name(m));
    }
}

/// The scrub bar, refused where it is written. It selects frames rather than
/// rows inside one, every frame is already drawn, and so it moves the clock —
/// which belongs to the viewer and not to the sentence.
#[test]
fn brushing_the_played_column_is_the_scrub_bar_and_is_refused() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .channel(Channel::Play, "continent")
        .brush(crate::ir::BrushDef::new("continent").levels(vec!["Asia".into()]));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("scrub bar")
        && d.message.contains("Brush a column")), "{:?}", msgs(&out));
}

/// A map is not brushed: its degrees are projected before anything is drawn,
/// so a range in degrees is no rectangle on the page. Refused, written bound,
/// bare `brush` or traced outline alike, and pointed at a flat plot of the same
/// columns, which draws.
#[test]
fn a_map_is_not_brushed_and_the_direction_draws() {
    let map = || base().layer(Layer::new(Mark::Point))
        .coord(CoordSpace::Map(crate::ir::MapView::default()));
    for spec in [
        map().brush(crate::ir::BrushDef::new("gdp").at(1.0, 2.0)),
        map().brush(crate::ir::BrushDef::new("continent").levels(vec!["Asia".into()])),
    ] {
        let out = check(&spec, &data());
        let d = out.iter().find(|d| d.message.contains("cannot select on a `map()`"))
            .unwrap_or_else(|| panic!("no refusal: {:?}", msgs(&out)));
        assert_eq!(d.kind, DiagnosticKind::Illegal);
        assert!(d.message.contains("`point + x(gdp) + y(life) + brush("), "{}", d.message);
        // One sentence: the brush's own questions are moot on a map.
        assert_eq!(out.iter().filter(|d| d.message.contains("brush")).count(), 1,
                   "{:?}", msgs(&out));
    }
    let flat = base().layer(Layer::new(Mark::Point))
        .brush(crate::ir::BrushDef::new("gdp").at(1.0, 2.0));
    let out = check(&flat, &data());
    assert!(!out.iter().any(Diagnostic::is_fatal), "the direction is refused: {:?}", msgs(&out));
}

/// A brush with no bound selects only what a reader drags, and on a polar plot,
/// in the cube and on a globe a drag cannot select: refused there, bare or
/// named, toward a bound written in the sentence, which draws. A flat plot keeps
/// the bare brush, and a flow keeps it in polar, since a flow selects by a click.
#[test]
fn a_brush_with_no_bound_is_refused_where_a_drag_cannot_select() {
    let point = || base().layer(Layer::new(Mark::Point));
    let spaces = [
        ("On a polar plot a drag never selects",
         point().coord(CoordSpace::Polar(crate::ir::PolarView::default()))),
        ("In the cube a drag turns the plot", point().z("value")),
        ("In the cube a drag turns the plot",
         point().z("value").coord(CoordSpace::Space(crate::ir::SpaceView::default()))),
        ("On a globe a drag turns the globe",
         point().coord(CoordSpace::Globe(crate::ir::GlobeView::default()))),
    ];
    for (why, spec) in spaces {
        for (brush, direction) in [
            (crate::ir::BrushDef::positions(), "`brush(gdp, at = ...)`"),
            (crate::ir::BrushDef::new("life"), "`brush(life, at = ...)`"),
        ] {
            let out = check(&spec.clone().brush(brush), &data());
            let d = out.iter().find(|d| d.message.contains("would never select a row"))
                .unwrap_or_else(|| panic!("{why}: no refusal: {:?}", msgs(&out)));
            assert_eq!(d.kind, DiagnosticKind::Illegal);
            assert!(d.message.contains(why) && d.message.contains(direction), "{}", d.message);
        }
        for brush in [
            crate::ir::BrushDef::new("gdp").at(1.0, 2.0),
            crate::ir::BrushDef::new("continent").levels(vec!["Asia".into()]),
        ] {
            let out = check(&spec.clone().brush(brush), &data());
            assert!(!out.iter().any(|d| d.message.contains("would never select")),
                "{why}: a written bound draws: {:?}", msgs(&out));
        }
    }
    let flat = check(&point().brush(crate::ir::BrushDef::positions()), &data());
    assert!(!flat.iter().any(Diagnostic::is_fatal), "{:?}", msgs(&flat));
    let chord = PlotSpec::new().data("t").x("gdp")
        .layer(Layer::new(Mark::Ribbon).flow(&["continent", "region"]))
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .brush(crate::ir::BrushDef::new("continent"));
    let out = check(&chord, &data());
    assert!(!out.iter().any(|d| d.message.contains("would never select")), "{:?}", msgs(&out));
}

/// A mark whose rows are vertices has no single row to select, and the
/// refusal names every mark that takes a selection and draws it, and no other:
/// a plain `bar` is one since 2026-10-09, and a flow's `ribbon` answers under
/// `flow`, which the list left out while the book said so. It offered `group()` as well,
/// and a grouped line is refused the same way: a group splits a shape into
/// shapes, never into rows.
#[test]
fn a_mark_whose_rows_are_vertices_cannot_be_brushed() {
    let brushed = |layer: Layer| check(&base().layer(layer)
        .brush(crate::ir::BrushDef::new("gdp").at(1.0, 2.0)), &data());
    for layer in [Layer::new(Mark::Line), Layer::new(Mark::Line).encode(Channel::Group, "continent")] {
        let out = brushed(layer);
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("one shape through many rows")
            && !d.message.contains("group()")), "{:?}", msgs(&out));
        for m in ALL_MARKS.iter().filter(|m| is_drawable(m) && mark_takes_selection(m)) {
            let drawn = selection_draws(&Layer::new((*m).clone())).is_none();
            assert_eq!(out.iter().any(|d| d.message.contains(&format!("`{}`", mark_name(m)))),
                drawn, "{} offered wrongly: {:?}", mark_name(m), msgs(&out));
        }
        // An `interval` answers only under `bounds` and a `ribbon` only under
        // `flow`, so each is offered with it, after the marks that answer alone,
        // and never bare: a bare one is refused for the line's reason.
        assert!(out.iter().any(|d| d.message
            .ends_with("`zone`, `interval * bounds` and `ribbon * flow`.")), "{:?}", msgs(&out));
    }
    let out = brushed(Layer::new(Mark::Interval));
    assert!(out.iter().any(|d| d.message.starts_with("gog: an `interval` draws")
        && d.message.contains("`interval * bounds`")), "{:?}", msgs(&out));
    // The article follows the mark: "an `area`", "an `interval`".
    let out = brushed(Layer::new(Mark::Area));
    assert!(out.iter().any(|d| d.message.starts_with("gog: an `area` draws")), "{:?}", msgs(&out));
}

/// A summarized layer beside one that draws its rows: the plot draws, and the
/// engine says which layer it left whole. An **Assumption**, not a refusal —
/// the binding still does something, so refusing it would forbid a legal
/// sentence (Law 8).
#[test]
fn a_summarized_layer_beside_a_brushable_one_is_reported_not_refused() {
    // The tally names its own axis: a column named there would be read by nothing.
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .layer(Layer::new(Mark::Bar).transform(Transform::Count).encode(Channel::Y, "count"))
        .brush(crate::ir::BrushDef::new("gdp").at(1.0, 2.0));
    let out = check(&spec, &data());
    assert!(!out.iter().any(|d| d.is_fatal()), "the plot must still draw: {:?}", msgs(&out));
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Assumption
        && d.message.contains("drawn whole")), "{:?}", msgs(&out));
}

/// **A bound is written in the data's own units, whatever space the axis is
/// in.** A log scale rewrites its column to logarithms before the renderer
/// sees the frame, so a bound compared as written matches nothing at all —
/// every log-scaled plot in the manual drew fully dimmed, and it read as the
/// browser failing to select rather than as the engine testing the wrong
/// numbers.
#[test]
fn a_bound_is_tested_in_the_space_its_column_was_rewritten_into() {
    let df = DataFrame::new()
        .with_float("gdp", vec![100.0, 1000.0, 10000.0, 100000.0])
        .with_float("life", vec![1.0, 2.0, 3.0, 4.0]);
    let brushed = |scale: Option<ScaleType>| {
        let mut spec = base().layer(Layer::new(Mark::Point));
        let mut def = ChannelDef::field("gdp");
        def.scale = scale;
        spec.x = Some(def);
        spec.brush = vec![crate::ir::BrushDef::new("gdp").at(500.0, 50000.0)];
        // The renderer is handed a *logged* column when the axis is log.
        let seen = if spec.x.as_ref().and_then(|d| d.scale.clone()) == Some(ScaleType::Log) {
            crate::scale::log_column(&df, "gdp", 10.0)
        } else {
            df.clone()
        };
        brush_keeps(&spec, &seen).map(|k| k.iter().filter(|&&b| b).count())
    };
    assert_eq!(brushed(None), Some(2), "1000 and 10000 sit inside 500..50000");
    assert_eq!(
        brushed(Some(ScaleType::Log)),
        Some(2),
        "a log axis must select the same two rows, not none of them"
    );
}

/// The same trap, one shape up. A traced outline arrives in the data's own
/// units because that is the only space the browser can state it in, so it
/// lands in whatever space each column was rewritten into — and the two axes
/// are asked separately, because one may be logarithmic and the other not.
#[test]
fn a_traced_region_is_tested_in_the_space_its_columns_were_rewritten_into() {
    let df = DataFrame::new()
        .with_float("gdp", vec![100.0, 1000.0, 10000.0, 100000.0])
        .with_float("life", vec![1.0, 2.0, 3.0, 4.0]);
    let caught = |scale: Option<ScaleType>| {
        let mut spec = base().layer(Layer::new(Mark::Point));
        let mut def = ChannelDef::field("gdp");
        def.scale = scale.clone();
        spec.x = Some(def);
        spec.y = Some(ChannelDef::field("life"));
        // A box drawn round the two middle rows, written as four corners in
        // the columns' own units — dollars and years, never logarithms.
        spec.region = Some(crate::ir::RegionDef::new(
            "gdp", "life",
            vec![[500.0, 1.5], [50000.0, 1.5], [50000.0, 3.5], [500.0, 3.5]],
        ));
        let seen = if scale == Some(ScaleType::Log) {
            crate::scale::log_column(&df, "gdp", 10.0)
        } else {
            df.clone()
        };
        brush_keeps(&spec, &seen).map(|k| k.iter().filter(|&&b| b).count())
    };
    assert_eq!(caught(None), Some(2), "the two middle rows are inside the outline");
    assert_eq!(
        caught(Some(ScaleType::Log)),
        Some(2),
        "a log axis must catch the same two rows, not none of them"
    );
}

/// A range that does not run upward selects nothing, so it is a mistake
/// rather than an empty selection, and the message says which way to write it.
#[test]
fn a_range_that_does_not_run_upward_is_refused() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .brush(crate::ir::BrushDef::new("gdp").at(9.0, 1.0));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("smaller number first")), "{:?}", msgs(&out));
}

/// Names to select on a column of numbers match no row. All four bindings
/// drew `brush(gdp, at = "low")` with nothing selected and no message.
#[test]
fn names_to_select_on_a_column_of_numbers_are_refused() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .brush(crate::ir::BrushDef::new("gdp").levels(vec!["low".into()]));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("`gdp` is a column of numbers")
        && d.message.contains("two numbers, the smaller first")), "{:?}", msgs(&out));
    // Names on a column of categories stay a selection.
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .brush(crate::ir::BrushDef::new("continent").levels(vec!["Asia".into()]));
    assert!(!check(&spec, &data()).iter().any(|d| d.message.contains("column of numbers")));
}

// -----------------------------------------------------------------------
// range's band — the two ends are probabilities (spec §5)
// -----------------------------------------------------------------------

#[test]
fn a_quantile_band_is_accepted() {
    // The sentence the other three are measured against, so a refusal below
    // is about the numbers rather than about the band being unsayable.
    let spec = base().layer(
        Layer::new(Mark::Interval).transform(Transform::Range).band(0.25, 0.75));
    let out = about_the_sentence(check(&spec, &data()));
    assert!(out.is_empty(), "the interquartile band was refused: {:?}", msgs(&out));
}

#[test]
fn a_band_end_outside_zero_to_one_is_refused() {
    // `quantile_sorted` clamps, so 1.5 would silently become the maximum and
    // draw a band nobody asked for. Refused here instead, where it is fatal.
    let spec = base().layer(
        Layer::new(Mark::Interval).transform(Transform::Range).band(0.5, 1.5));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("not a probability")), "{:?}", msgs(&out));
}

#[test]
fn a_band_that_runs_downward_is_refused_and_says_to_swap() {
    let spec = base().layer(
        Layer::new(Mark::Interval).transform(Transform::Range).band(0.75, 0.25));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("Swap them")), "{:?}", msgs(&out));
}

#[test]
fn a_band_with_both_ends_equal_is_pointed_at_a_reduction() {
    // One quantile twice is a number, not a span, and the direction has to
    // name the atom that draws a number rather than repeat the arithmetic.
    let spec = base().layer(
        Layer::new(Mark::Interval).transform(Transform::Range).band(0.5, 0.5));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("`median`")), "{:?}", msgs(&out));
}

// -----------------------------------------------------------------------
// deviation and quantile — the family's two newest members (spec §5)
// -----------------------------------------------------------------------

/// `smooth_band` is a pair transform that fits along a numeric domain: a ribbon
/// fills it and a line traces its two edges; a bar has no pair to draw, a category
/// has no domain to fit along, and it composes with no second measurement or a
/// share. Its wire name keeps its underscore (Law 3).
#[test]
fn a_smooth_band_is_a_pair_that_fits_along_a_numeric_domain() {
    let with = |mark: Mark, ts: &[Transform]| {
        ts.iter().fold(Layer::new(mark), |l, t| l.transform(t.clone()))
    };
    // Five rows, the band's minimum: the shared table's three are too few for it.
    let mut five = HashMap::new();
    five.insert("t".to_string(), DataFrame::new()
        .with_float("gdp", vec![1.0, 2.0, 3.0, 4.0, 5.0])
        .with_float("life", vec![4.0, 6.0, 5.0, 7.0, 6.0]));
    for mark in [Mark::Ribbon, Mark::Line] {
        let d = check(&base().layer(with(mark.clone(), &[Transform::SmoothBand])), &five);
        assert!(!d.iter().any(|x| x.is_fatal()), "{mark:?} * smooth_band: {:?}", msgs(&d));
    }
    let refused = |spec: PlotSpec| check(&spec, &data()).iter().any(|x| x.is_fatal());
    assert!(refused(base().layer(with(Mark::Bar, &[Transform::SmoothBand]))), "a bar");
    assert!(refused(PlotSpec::new().data("t").x("continent").y("life")
        .layer(with(Mark::Ribbon, &[Transform::SmoothBand]))), "a category");
    // Both of the fit's refusals name the transform the reader wrote, and the
    // category one points at the band per category rather than the value.
    let category = msgs(&check(&PlotSpec::new().data("t").x("continent").y("life")
        .layer(with(Mark::Ribbon, &[Transform::SmoothBand])), &data()));
    assert!(category.iter().any(|m| m.contains("`smooth_band` fits a curve of `y` against `x`")
        && m.contains("`interval * confidence`")), "{category:?}");
    let thin = msgs(&check(&base()
        .layer(with(Mark::Ribbon, &[Transform::SmoothBand]).encode(Channel::Color, "region")),
        &data()));
    assert!(thin.iter().any(|m| m.contains("`smooth_band` fits a curve through the rows")),
        "{thin:?}");
    assert!(!thin.iter().chain(&category).any(|m| m.contains("`smooth` fits")), "{thin:?}");
    let chain = check(&base().layer(with(Mark::Ribbon, &[Transform::SmoothBand, Transform::Mean])), &data());
    assert!(msgs(&chain).iter().any(|m| m.contains("`smooth_band` fits a curve")), "{:?}", msgs(&chain));
    assert!(refused(base().layer(with(Mark::Ribbon, &[Transform::SmoothBand, Transform::Proportion]))),
        "a share");
    assert_eq!(serde_json::to_string(&Transform::SmoothBand).unwrap(), r#""smooth_band""#);
}

#[test]
fn a_spread_band_is_accepted() {
    let spec = base().layer(
        Layer::new(Mark::Interval).transform(Transform::Deviation).spread(2.0));
    let out = about_the_sentence(check(&spec, &data()));
    assert!(out.is_empty(), "a spread band was refused: {:?}", msgs(&out));
}

#[test]
fn a_deviation_of_zero_is_refused_toward_the_mean() {
    // A band of width zero is a point, and the sentence that draws a point
    // at the mean is `point * mean`. Refused rather than drawn flat.
    let spec = base().layer(
        Layer::new(Mark::Interval).transform(Transform::Deviation).spread(0.0));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("`point * mean`")), "{:?}", msgs(&out));
}

#[test]
fn a_bare_quantile_is_refused_and_names_median() {
    // The one transform whose parameter is required rather than defaulted:
    // the only sensible default is the middle, and that is already `median`.
    let spec = base().layer(Layer::new(Mark::Bar).transform(Transform::Quantile));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("`median`")), "{:?}", msgs(&out));
}

#[test]
fn a_quantile_outside_zero_to_one_is_refused() {
    let spec = base().layer(
        Layer::new(Mark::Bar).transform(Transform::Quantile).at_quantile(90.0));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("not a probability")), "{:?}", msgs(&out));
}

#[test]
fn a_quantile_away_from_the_plain_points_says_nothing() {
    let spec = base().layer(
        Layer::new(Mark::Bar).transform(Transform::Quantile).at_quantile(0.9));
    let out = about_the_sentence(check(&spec, &data()));
    assert!(out.is_empty(), "the 90th percentile earned a remark: {:?}", msgs(&out));
}

/// The ruling this atom turns on: at the three points where `quantile`
/// duplicates a plain name it **warns and draws**. Not Illegal, because a
/// generated sweep over deciles would die at 0.5; not silent, because two
/// spellings for one result is what Law 1 exists to notice.
#[test]
fn a_quantile_that_duplicates_a_plain_name_warns_and_still_draws() {
    for (p, plain) in [(0.0, "min"), (0.5, "median"), (1.0, "max")] {
        let spec = base().layer(
            Layer::new(Mark::Bar).transform(Transform::Quantile).at_quantile(p));
        let out = check(&spec, &data());
        assert!(
            out.iter().any(|d| d.kind == DiagnosticKind::Assumption
                && d.message.contains(plain)),
            "quantile({p}) did not name `{plain}`: {:?}", msgs(&out)
        );
        assert!(
            !out.iter().any(|d| d.kind == DiagnosticKind::Illegal),
            "quantile({p}) refused instead of drawing: {:?}", msgs(&out)
        );
    }
}

// -----------------------------------------------------------------------
// Nest — the packed space's own refusals (spec §15)
// -----------------------------------------------------------------------

/// The sentence that must stay clean, so every refusal below is about the
/// thing it names rather than about the space being unusable.
fn nest_base() -> PlotSpec {
    PlotSpec::new().data("t").y("gdp").coord(CoordSpace::Nest)
}

#[test]
fn a_packed_bar_with_a_measure_is_accepted() {
    let spec = nest_base()
        .layer(Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "continent"));
    let out = check(&spec, &data());
    assert!(out.is_empty(), "a plain treemap was refused: {:?}", msgs(&out));
}

/// Every layer of a packing packs its own rows, so a summed layer beside a
/// plain one lays out two sets of regions over one rectangle, and the labels
/// landed in other groups' regions. Without the summary both pack one row
/// per region, which is how a treemap is labeled.
#[test]
fn a_nest_refuses_layers_that_pack_different_rows() {
    let spec = nest_base()
        .layer(Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "continent"))
        .layer(Layer::new(Mark::Text).encode(Channel::Label, "continent"));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("`bar * sum` packs one region per group")
            && d.message.contains("`text` packs every row")), "{:?}", msgs(&out));
    let labeled = nest_base()
        .layer(Layer::new(Mark::Bar).encode(Channel::Color, "continent"))
        .layer(Layer::new(Mark::Text).encode(Channel::Label, "continent"));
    assert!(check(&labeled, &data()).is_empty(), "{:?}", msgs(&check(&labeled, &data())));
}

/// Five settings describe an axis, and a packing has none: each was accepted
/// and drew the same bytes as the sentence without it. A preset that fills one
/// in is not the reader's statement and is not refused.
#[test]
fn a_nest_refuses_the_settings_of_an_axis() {
    let treemap = || nest_base()
        .layer(Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "continent"));
    let refused = |spec: PlotSpec, said: &str| {
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal && d.message.contains(said)),
                "{said}: {:?}", msgs(&out));
    };
    let themed = |f: &dyn Fn(&mut ThemeSpec)| {
        let mut spec = treemap();
        f(&mut spec.theme);
        spec
    };
    refused(themed(&|t| t.grid = Some("both".into())), "`theme(grid = )`");
    refused(themed(&|t| t.tick_angle = Some(45.0)), "`theme(tick_angle = )`");
    refused(themed(&|t| t.axis_label = Some("end".into())), "`theme(axis_label = )`");
    refused(themed(&|t| t.frame = Some("full".into())), "`theme(frame = )`");
    let mut counted = treemap();
    counted.y = Some(ChannelDef { tick_count: Some(3), ..ChannelDef::field("gdp") });
    refused(counted, "`y(gdp, tick_count = 3)`");
    let mut bounded = treemap();
    bounded.y = Some(ChannelDef { limits: Some([Some(0.0), Some(10.0)]), ..ChannelDef::field("gdp") });
    refused(bounded, "under `bar * sum`");

    let preset = themed(&|t| t.preset = Some("minimal".into()));
    assert!(check(&preset, &data()).is_empty(), "a preset is not the reader's `grid`");
    // Without a summary a domain leaves rows out, which is not ignoring it.
    let mut identity = nest_base().layer(Layer::new(Mark::Bar).encode(Channel::Color, "continent"));
    identity.y = Some(ChannelDef { limits: Some([Some(0.0), Some(10.0)]), ..ChannelDef::field("gdp") });
    assert!(check(&identity, &data()).iter().all(|d| !d.message.contains("under `bar")));
}

/// A label needs no `x` here — Law 7's third relaxation, and the one this
/// space adds. Flat, `text` requires both positions; a packing places by
/// region, so the one-level treemap's label is well formed with a measure and
/// a string and nothing else.
#[test]
fn a_packed_label_needs_no_domain_axis() {
    let spec = nest_base()
        .layer(Layer::new(Mark::Text).encode(Channel::Label, "continent"));
    let out = check(&spec, &data());
    assert!(out.is_empty(), "a packed label was refused: {:?}", msgs(&out));
}

/// And it still needs the **measure**, which is what the relaxation does not
/// touch: `y` is the only thing a space with no coordinates cannot pack
/// without, so dropping it is refused rather than assumed.
#[test]
fn a_packed_label_still_needs_its_measure() {
    let spec = PlotSpec::new().data("t").coord(CoordSpace::Nest)
        .layer(Layer::new(Mark::Text).encode(Channel::Label, "continent"));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal && d.message.contains("`y()`")),
            "a packing with nothing to size its regions by was accepted: {:?}", msgs(&out));
}

/// `style(nudge = )` steps a label off the point it would cover, and a packed
/// label covers no point. Refused rather than ignored — accepting a setting
/// that changes nothing is the silent drop §12 forbids.
#[test]
fn a_nudge_is_refused_in_a_packed_panel() {
    let mut text = Layer::new(Mark::Text).encode(Channel::Label, "continent");
    text.style.nudge = Some("up".into());
    let spec = nest_base().layer(text);
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
                            && d.message.contains("nudge")),
            "a nudge with nothing to move away from was accepted: {:?}", msgs(&out));
}

/// A packed name that does not fit is drawn smaller, so the refusals of the
/// two offsets a reader reaches for must not say names go missing or advise a
/// smaller `style(size = )`: both did, from the rule before names shrank.
#[test]
fn a_packed_panels_offset_refusals_describe_names_that_shrink() {
    let mut nudged = Layer::new(Mark::Text).encode(Channel::Label, "continent");
    nudged.style.nudge = Some("up".into());
    let repelled = Layer::new(Mark::Text).encode(Channel::Label, "continent")
        .transform(Transform::Repel);
    for (layer, word) in [(nudged, "nudge"), (repelled, "`repel`")] {
        let out = check(&nest_base().layer(layer), &data());
        let m = out.iter().find(|d| d.kind == DiagnosticKind::Illegal && d.message.contains(word))
            .map(|d| d.message.clone())
            .unwrap_or_else(|| panic!("{word} in a packed panel was accepted: {:?}", msgs(&out)));
        assert!(m.contains("smaller") && !m.contains("style(size")
                && !m.contains("missing") && !m.contains("fits more"), "{word}: {m}");
    }
}

/// The whole Mark × Space column in one test, read off `mark_draws_in_space`
/// rather than typed out — the drift `check_polar`'s comment warns about. The
/// name says "the grid" rather than "every mark but bar" because the column
/// gained `text` on 2026-07-27 and a test named after its answer goes stale
/// the moment the answer moves.
#[test]
fn only_the_marks_the_grid_lists_draw_in_a_packed_panel() {
    for m in ALL_MARKS.iter() {
        if !is_drawable(m) { continue; }
        // `edge` is refused by its own sentence in every space but the network,
        // which names `network()`; this loop stands down for it
        // (`placed_by_the_graph`), and the whole check still refuses it.
        if *m == Mark::Edge {
            let spec = nest_base().layer(Layer::new(Mark::Edge));
            assert!(check(&spec, &data()).iter().any(|d| d.kind == DiagnosticKind::Illegal
                && d.message.contains("network()")), "edge in nest must still be refused");
            continue;
        }
        let mut out = Vec::new();
        let spec = nest_base().layer(Layer::new(m.clone()));
        check_nest(&mut out, &spec, &data());
        let refused = out.iter().any(|d| d.message.contains("Drop `nest()`"));
        assert_eq!(
            !refused,
            mark_draws_in_space(m, SpaceKind::Nest),
            "{m:?} in nest: {:?}", msgs(&out)
        );
    }
}

/// A collision modifier presupposes places for marks to collide in, and a
/// packing has none. This is where the build diverged from the design (§15),
/// so it is pinned rather than left to the renderer to shrug off.
#[test]
fn a_collision_modifier_is_refused_in_a_packed_panel() {
    for t in [Transform::Stack, Transform::Dodge, Transform::Jitter] {
        let spec = nest_base().layer(
            Layer::new(Mark::Bar).transform(Transform::Sum).transform(t.clone())
                .encode(Channel::Color, "continent"));
        let out = check(&spec, &data());
        let name = transform_name(&t);
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
                                && d.message.contains(name)),
                "{name} was accepted in a packed panel: {:?}", msgs(&out));
    }
}

#[test]
fn naming_an_axis_of_a_packed_panel_is_refused() {
    for spec in [nest_base().x_label("Wealth"), nest_base().y_label("Wealth")] {
        let spec = spec.layer(Layer::new(Mark::Bar).transform(Transform::Sum));
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
                                && d.message.contains("names an axis")),
                "an axis label was accepted: {:?}", msgs(&out));
    }
}

/// The refusal that exists because the alternative is a *silent* one: a log
/// scale here would be accepted and change nothing on the page.
#[test]
fn a_log_scale_on_a_packed_measure_is_refused() {
    let spec = PlotSpec::new().data("t").y_scaled("gdp", ScaleType::Log)
        .coord(CoordSpace::Nest)
        .layer(Layer::new(Mark::Bar).transform(Transform::Sum));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.message.contains("share of the total")),
            "a log scale was accepted in a packed panel: {:?}", msgs(&out));
}

#[test]
fn a_negative_measure_has_no_area_and_is_refused() {
    let mut d = data();
    d.insert("t".into(), DataFrame::new()
        .with_float("gdp", vec![1.0, -2.0, 3.0])
        .with_str("continent", vec!["Asia".into(), "Europe".into(), "Africa".into()]));
    let spec = nest_base().layer(Layer::new(Mark::Bar).transform(Transform::Sum));
    let out = check(&spec, &d);
    assert!(out.iter().any(|x| x.kind == DiagnosticKind::Illegal
                            && x.message.contains("cannot be negative")),
            "a negative area was accepted: {:?}", msgs(&out));
}

#[test]
fn a_packed_panel_with_nothing_to_measure_is_refused() {
    let spec = PlotSpec::new().data("t").coord(CoordSpace::Nest)
        .layer(Layer::new(Mark::Bar).encode(Channel::Color, "continent"));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.message.contains("has no measure")),
            "a packing with no measure was accepted: {:?}", msgs(&out));
}

/// `order()` sorts a categorical position axis, so a plot with none was
/// accepting it and dropping it. Found via the packed space, which is where a
/// sentence with no bound position is the *ordinary* one, and fixed for every
/// space because the rule was always `order()`'s rather than the packing's.
#[test]
fn order_needs_a_categorical_axis_to_sort() {
    let ordered = |spec: PlotSpec| {
        let mut out = Vec::new();
        check_order(&mut out, &spec.order_desc("gdp"), &data());
        out
    };
    // No categorical position anywhere: refused, in the plane and packed alike.
    for spec in [
        PlotSpec::new().data("t").y("gdp"),
        PlotSpec::new().data("t").y("gdp").coord(CoordSpace::Nest),
        PlotSpec::new().data("t").x("gdp").y("life"),
    ] {
        let out = ordered(spec);
        assert!(out.iter().any(|d| d.message.contains("nothing for it to put in order")),
                "order() with no categorical axis was accepted: {:?}", msgs(&out));
    }
    // A categorical axis is exactly what it wants, in either space.
    for spec in [
        PlotSpec::new().data("t").x("continent").y("gdp"),
        PlotSpec::new().data("t").x("continent").y("gdp").coord(CoordSpace::Nest),
    ] {
        assert!(ordered(spec).is_empty(), "order() was refused where it applies");
    }
}

#[test]
fn a_packing_and_a_cube_are_two_spaces() {
    let spec = nest_base().z("life")
        .layer(Layer::new(Mark::Bar).transform(Transform::Sum));
    let out = check(&spec, &data());
    assert!(out.iter().any(|d| d.message.contains("asks for two")),
            "`nest()` with `z()` was accepted: {:?}", msgs(&out));
}

#[test]
fn clean_scatter_has_no_diagnostics() {
    let spec = base().layer(Layer::new(Mark::Point));
    assert!(check(&spec, &data()).is_empty());
}

/// A transform that invents a position in the plane is not a height for the
/// cube. Each of these stood a bare `space()` in an empty cube and said nothing;
/// each is flat now, with the drawn-flat note, while the 3-D histogram and the
/// counted floor still project. Each is a plot that draws, since the note is not
/// given beside a refusal: the cluster tree is on the complete table.
#[test]
fn a_bare_space_is_flat_unless_a_tally_stands_on_z() {
    let cube = || CoordSpace::Space(crate::ir::SpaceView::default());
    for (spec, tables) in [
        (PlotSpec::new().data("t").x("gdp").coord(cube())
            .layer(Layer::new(Mark::Zone).bounds("life", "value")), data()),
        (PlotSpec::new().data("t").x("gdp").coord(cube())
            .layer(Layer::new(Mark::Ribbon).bounds("life", "value")), data()),
        (tree_spec().coord(cube()), cluster_table()),
        (PlotSpec::new().data("t").x("gdp").coord(cube())
            .layer(Layer::new(Mark::Zone).partition(&["continent", "region"])), data()),
        (PlotSpec::new().data("t").coord(cube())
            .layer(Layer::new(Mark::Ribbon).flow(&["continent", "region"])), data()),
    ] {
        assert_eq!(space_of(&spec), SpaceKind::Flat, "{:?}", spec.layers[0].transforms);
        let d = check(&spec, &tables);
        assert!(!d.iter().any(Diagnostic::is_fatal), "{:?}", msgs(&d));
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Assumption
            && x.message.contains("drawn flat") && x.message.contains("drop `space()`")
            && x.message.contains("to a mark that takes one")),
            "a bare space() over {:?} should be drawn flat, and said: {:?}",
            spec.layers[0].transforms, msgs(&d));
    }
    // Each way into the cube the note names draws.
    for spec in [
        base().z("value").layer(Layer::new(Mark::Point)),
        base().coord(cube()).layer(Layer::new(Mark::Bar).transform(Transform::Bin)),
        PlotSpec::new().data("t").x("continent").y("region").coord(cube())
            .layer(Layer::new(Mark::Bar).transform(Transform::Count)),
    ] {
        let d = check(&spec, &data());
        assert!(!d.iter().any(Diagnostic::is_fatal), "a direction is refused: {:?}", msgs(&d));
    }
    for spec in [
        PlotSpec::new().data("t").x("gdp").y("life").coord(cube())
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin)),
        PlotSpec::new().data("t").x("continent").y("region").coord(cube())
            .layer(Layer::new(Mark::Bar).transform(Transform::Count)),
    ] {
        assert_eq!(space_of(&spec), SpaceKind::Space,
                   "a tally stands on z: {:?}", spec.layers[0].transforms);
    }
}

/// **An ask is pixels, and a page honors each one or refuses** (ruled
/// 2026-09-28). A page with no size of its own is as big as its plots ask,
/// the gaps included; a page stated at a size refuses plots it cannot hold
/// exactly; plots side by side share one height; and a plot that asked for
/// nothing is never left without room.
#[test]
fn an_ask_is_pixels_and_a_page_honors_it_or_refuses() {
    let tall = |h: f64| {
        let mut p = base().layer(Layer::new(Mark::Point));
        p.theme.height = Some(h);
        p
    };
    let free = base().layer(Layer::new(Mark::Point));
    let page = |arrange: crate::ir::Arrange, a: &PlotSpec, b: &PlotSpec, height: Option<f64>| {
        Figure::Page(crate::ir::PageSpec {
            arrange, cells: vec![a.clone().into(), b.clone().into()],
            theme: crate::ir::ThemeSpec { height, ..Default::default() },
        })
    };
    let said = |f: &Figure| msgs(&check_figure(f, &data())).join(" | ");
    let (below, beside) = (crate::ir::Arrange::Below, crate::ir::Arrange::Beside);

    // Two 300 px plots stacked: a 620 px page, no stretching and no refusal.
    let stacked = page(below.clone(), &tall(300.0), &tall(300.0), None);
    assert_eq!(stacked.ask(false), Some(620.0), "each ask plus the gap");
    assert!(said(&stacked).is_empty(), "{}", said(&stacked));
    // The same two on a page stated at 600: the gap is counted, and it cannot hold them.
    let stated = page(below.clone(), &tall(300.0), &tall(300.0), Some(600.0));
    assert!(said(&stated).contains("the 20px between them makes 620px; the arrangement has 600"),
        "{}", said(&stated));
    // Every plot asked, so the page is what to change; with one that did not,
    // the plots that asked are.
    assert!(said(&stated).contains("drop the arrangement's `theme(height = )`, or make a plot's smaller"),
        "{}", said(&stated));
    let over = page(below.clone(), &tall(700.0), &free, None);
    assert!(said(&over).contains("makes 720px; the arrangement has 600")
        && said(&over).contains("must leave room for the ones that do not"),
        "{}", said(&over));
    // A page stated larger than its plots, which all ask: it cannot be both sizes.
    let loose = page(below.clone(), &tall(100.0), &tall(100.0), Some(300.0));
    assert!(said(&loose).contains("and the arrangement states 300"), "{}", said(&loose));
    // A plot that asks for nothing is not left with no room at all.
    let squeezed = page(below.clone(), &tall(580.0), &free, None);
    assert!(said(&squeezed).contains("leaves no room for the plot that asks for no size"),
        "{}", said(&squeezed));
    // Side by side, one height: a plot's height is the page's, and two differ.
    let one = page(beside.clone(), &tall(100.0), &free, None);
    assert_eq!(one.ask(false), Some(100.0), "the page takes the one height asked");
    assert!(said(&one).is_empty(), "{}", said(&one));
    let two = page(beside.clone(), &tall(100.0), &tall(200.0), None);
    assert!(said(&two).contains("share one height, and 100px and 200px are asked for"),
        "{}", said(&two));
}

/// A shared axis lines up across a page, so a plot split into panels along it
/// and one that is not cannot share it: the histogram over a faceted scatter
/// spanned every panel and lined up with none. Both split the same way draw.
#[test]
fn a_page_refuses_a_shared_axis_split_into_panels_differently() {
    let page = |arrange: crate::ir::Arrange, a: &PlotSpec, b: &PlotSpec| {
        Figure::Page(crate::ir::PageSpec {
            arrange, cells: vec![a.clone().into(), b.clone().into()],
            theme: crate::ir::ThemeSpec::default(),
        })
    };
    let hist = PlotSpec::new().data("t").x("gdp")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    let scatter = base().layer(Layer::new(Mark::Point));
    let faceted = scatter.clone().facet_col("continent");
    let d = check_figure(&page(crate::ir::Arrange::Below, &hist, &faceted), &data());
    let said = d.iter().find(|x| x.message.contains("split into panels differently"))
        .unwrap_or_else(|| panic!("no refusal: {:?}", msgs(&d)));
    assert_eq!(said.kind, DiagnosticKind::Illegal);
    assert!(said.message.contains("one is not split into panels, the other split into \
                                   panels by `continent`")
            && said.message.contains("`| facet(continent)` on each"), "{}", said.message);
    // Both split the same way line up one panel over each, and draw.
    let both = page(crate::ir::Arrange::Below, &hist.clone().facet_col("continent"), &faceted);
    assert!(!check_figure(&both, &data()).iter().any(|x| x.message.contains("differently")));
    // A plot beside a one-row facet shares `y` across one row, which lines up.
    let beside = page(crate::ir::Arrange::Beside, &faceted, &scatter);
    assert!(!check_figure(&beside, &data()).iter()
        .any(|x| x.message.contains("differently") && x.message.contains("the y axis")));
}

/// A column read two ways on a page is two axes, not a contradiction (ruled
/// 2026-10-08). Two plots reading `gdp` through different scales, or different
/// limits, were refused, because the page had merged them into one broken axis.
/// The page now shares only a column read the same way, so nothing is refused,
/// and the checks that ask about a shared axis (drawn or left out, split into
/// panels or not) are not asked of two plots that do not share one.
#[test]
fn a_page_reads_one_column_two_ways_without_refusing() {
    let page = |arrange: crate::ir::Arrange, a: &PlotSpec, b: &PlotSpec| {
        Figure::Page(crate::ir::PageSpec {
            arrange, cells: vec![a.clone().into(), b.clone().into()],
            theme: crate::ir::ThemeSpec::default(),
        })
    };
    let point = || Layer::new(Mark::Point);
    let lin = base().layer(point());
    let log = PlotSpec::new().data("t").x_scaled("gdp", ScaleType::Log).y("life").layer(point());
    let ylog = PlotSpec::new().data("t").x("value").y_scaled("life", ScaleType::Log).layer(point());
    let lim = PlotSpec::new().data("t").x_limited("gdp", Some(0.0), Some(2.0)).y("life").layer(point());
    for (arrange, a, b) in [
        (crate::ir::Arrange::Below, &lin, &log),
        (crate::ir::Arrange::Beside, &log, &lin),
        (crate::ir::Arrange::Beside, &ylog, &lin),
        (crate::ir::Arrange::Below, &lim, &lin),
    ] {
        let d = check_figure(&page(arrange.clone(), a, b), &data());
        assert!(!d.iter().any(Diagnostic::is_fatal), "{arrange:?}: {:?}", msgs(&d));
    }
    // Leaving out an axis the other plot draws is a contradiction only when
    // the two share it: read two ways, each plot draws or omits its own.
    let mut quiet_log = log.clone();
    quiet_log.x = quiet_log.x.map(|d| d.with_axis(false));
    let mut drawn_lin = lin.clone();
    drawn_lin.x = drawn_lin.x.map(|d| d.with_axis(true));
    let d = check_figure(&page(crate::ir::Arrange::Below, &quiet_log, &drawn_lin), &data());
    assert!(!d.iter().any(Diagnostic::is_fatal), "two axes, two answers: {:?}", msgs(&d));
    let shared = check_figure(&page(crate::ir::Arrange::Below, &quiet_log, &{
        let mut l = log.clone(); l.x = l.x.map(|d| d.with_axis(true)); l
    }), &data());
    assert!(shared.iter().any(Diagnostic::is_fatal),
            "one shared axis still cannot be both drawn and left out: {:?}", msgs(&shared));
}

/// A smoothed point in the cube lost its third position and drew an empty cube,
/// in silence. It is refused as every smooth in the cube is, and pointed at the
/// bar for a floor summary, since a point was never given the pairwise group-by.
#[test]
fn a_smoothed_point_in_the_cube_is_refused_toward_the_plane() {
    let d = check(&base().z("value")
        .layer(Layer::new(Mark::Point).transform(Transform::Smooth)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("`point * smooth` fits a curve")
        && x.message.contains("`bar * mean + x(<a>) + y(<b>) + z(<column>) + space()`")),
        "point * smooth with z: {:?}", msgs(&d));
    let flat = check(&base().layer(Layer::new(Mark::Point).transform(Transform::Smooth)), &data());
    assert!(!flat.iter().any(Diagnostic::is_fatal), "flat point * smooth: {:?}", msgs(&flat));
    // A band on a point is refused for being a span, and not again for the cube.
    let band = check(&base().z("value")
        .layer(Layer::new(Mark::Point).transform(Transform::SmoothBand)), &data());
    assert!(band.iter().any(|x| x.message.contains("not a span"))
            && !band.iter().any(|x| x.message.contains("fits a curve of one column")),
            "point * smooth_band with z is refused as a span, once: {:?}", msgs(&band));
}

/// Law 7's second half: positions with no mark are refused, as a mark with no
/// positions always was. Each direction is read from the positions named, and
/// each is a sentence the engine draws, which the last loop proves.
#[test]
fn a_plot_with_no_mark_is_refused_and_told_which_mark_to_add() {
    let refused = |spec: PlotSpec, wanted: &str| {
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
                    && d.message.contains("no mark") && d.message.contains(wanted)),
                "wanted a refusal naming {wanted:?}: {:?}", msgs(&out));
    };
    refused(PlotSpec::new().data("t"), "such as `point` with an `x` and a `y`");
    refused(PlotSpec::new().data("t").title("Nothing"), "such as `point`");
    refused(PlotSpec::new().data("t").channel(Channel::Color, "continent"), "such as `point`");
    refused(PlotSpec::new().data("t").x("gdp"), "`bar * bin` shows how the values of `gdp`");
    refused(PlotSpec::new().data("t").y("continent"),
            "`bar * count` counts the rows for each value of `continent`");
    refused(base(), "`x` names `gdp` and `y` names `life`, but no mark draws them");
    refused(base().z("value"), "`point` draws a dot for each row");
    refused(PlotSpec::new().data("t").z("value"), "and the `x` and `y` it needs");

    // Every direction given above draws.
    for spec in [
        base().layer(Layer::new(Mark::Point)),
        base().z("value").layer(Layer::new(Mark::Point)),
        PlotSpec::new().data("t").x("gdp").layer(Layer::new(Mark::Bar).transform(Transform::Bin)),
        PlotSpec::new().data("t").y("continent")
            .layer(Layer::new(Mark::Bar).transform(Transform::Count)),
    ] {
        let out = check(&spec, &data());
        assert!(!out.iter().any(Diagnostic::is_fatal), "a direction is refused: {:?}", msgs(&out));
    }
}

/// An axis holds one kind of value. Two tables that gave one position column
/// two kinds drew the numbers as categories' places, counting from 0 and off
/// the plot, with nothing said; dates read against numbers the same way.
#[test]
fn layers_that_disagree_about_an_axis_kind_are_refused() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let tables: HashMap<String, DataFrame> = HashMap::from([
        ("cats".to_string(), DataFrame::new().with_str("g", s(&["a", "b", "c"]))
            .with_float("v", vec![1.0, 2.0, 3.0])),
        ("nums".to_string(), DataFrame::new().with_float("g", vec![10.0])
            .with_float("v", vec![2.0]).with_str("t", s(&["ten"]))),
        ("pts".to_string(), DataFrame::new().with_float("g", vec![1.0, 2.0, 3.0])
            .with_float("v", vec![1.0, 2.0, 3.0])),
        ("lab".to_string(), DataFrame::new().with_str("g", s(&["b"]))
            .with_float("v", vec![2.0]).with_str("t", s(&["bee"]))),
        ("days".to_string(), DataFrame::new()
            .with_time("g", vec![0.0, 86_400.0], crate::time::TimeUnit::Day)
            .with_float("v", vec![1.0, 2.0])),
    ]);
    let refused = |spec: PlotSpec, wanted: &[&str]| {
        let out = check(&spec, &tables);
        let d = out.iter().find(|d| d.message.contains("axis is read two ways"))
            .unwrap_or_else(|| panic!("no refusal: {:?}", msgs(&out)));
        assert_eq!(d.kind, DiagnosticKind::Illegal);
        for w in wanted {
            assert!(d.message.contains(w), "wanted {w:?} in {}", d.message);
        }
    };
    // Bars over text, and a label placed at a number.
    refused(PlotSpec::new().data("cats").x("g").y("v")
                .layer(Layer::new(Mark::Bar))
                .layer(Layer::new(Mark::Text).data("nums").encode(Channel::Label, "t")),
            &["the `x` axis", "`g` holds text in `cats` (read by `bar`)",
              "`g` holds numbers in `nums` (read by `text`)"]);
    // The reverse, which sent every point off the canvas.
    refused(PlotSpec::new().data("pts").x("g").y("v")
                .layer(Layer::new(Mark::Point))
                .layer(Layer::new(Mark::Text).data("lab").encode(Channel::Label, "t")),
            &["`g` holds numbers in `pts`", "`g` holds text in `lab`"]);
    // A date read against a plain number.
    refused(PlotSpec::new().data("days").x("g").y("v")
                .layer(Layer::new(Mark::Line))
                .layer(Layer::new(Mark::Point).data("pts")),
            &["`g` holds dates in `days`", "`g` holds numbers in `pts`"]);
    // One kind in every table is one axis, and draws.
    let one_kind = PlotSpec::new().data("cats").x("g").y("v")
        .layer(Layer::new(Mark::Bar))
        .layer(Layer::new(Mark::Text).data("lab").encode(Channel::Label, "t"));
    let out = check(&one_kind, &tables);
    assert!(!out.iter().any(|d| d.message.contains("axis is read")), "{:?}", msgs(&out));
}

/// A plot draws one color legend, so a second `color` column reached the
/// reader with no key: through the first column's scale when it held
/// categories, through a ramp of its own when it held numbers. Refused, with
/// a channel of the second column's own; the same column, or a legend turned
/// off on purpose, still draws.
#[test]
fn a_second_column_on_a_legend_channel_is_refused_with_a_channel_of_its_own() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let tables: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("year", vec![2000.0, 2001.0, 2000.0, 2001.0])
            .with_float("life", vec![60.0, 61.0, 70.0, 71.0])
            .with_str("country", s(&["A", "A", "B", "B"]))
            .with_str("continent", s(&["Asia", "Asia", "Asia", "Asia"])),
    )]);
    let base = || PlotSpec::new().data("t").x("year").y("life");
    let refusal = |spec: PlotSpec| {
        let out = check(&spec, &tables);
        out.into_iter().find(|d| d.message.contains("A plot draws one legend for"))
    };
    // A category on the second layer: shown by its own texture, or a facet.
    let d = refusal(base()
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "country"))
        .layer(Layer::new(Mark::Line).encode(Channel::Color, "continent")))
        .expect("two category columns on `color`");
    assert_eq!(d.kind, DiagnosticKind::Illegal);
    for w in ["`color` maps 2 columns in one plot: `country` (on `point`), and \
               `continent` (on `line`)", "it names `country`",
              "`pattern(continent)` on the `line`", "`facet(continent)` for a panel each"] {
        assert!(d.message.contains(w), "wanted {w:?} in {}", d.message);
    }
    // Numbers on the second layer: size or opacity, which draw their own keys.
    let d = refusal(base()
        .layer(Layer::new(Mark::Line).encode(Channel::Color, "country"))
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "year")))
        .expect("a number column after a category column");
    assert!(d.message.contains("`size(year)` on the `point`"), "{}", d.message);
    assert!(!d.message.contains("facet(year)"), "a facet of numbers is refused: {}", d.message);
    // The heatmap under colored points: the points' category takes a shape.
    let d = refusal(base()
        .layer(Layer::new(Mark::Zone).encode(Channel::Color, "life"))
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "country")))
        .expect("a ramp and a category on `color`");
    assert!(d.message.contains("`shape(country)` on the `point`"), "{}", d.message);
    // A sibling channel keys one column the same way, and is refused the same way.
    let d = refusal(base()
        .layer(Layer::new(Mark::Point).encode(Channel::Size, "life"))
        .layer(Layer::new(Mark::Point).encode(Channel::Size, "year")))
        .expect("two columns on `size`");
    assert!(d.message.contains("A plot draws one legend for `size`, and it names `life`"),
            "{}", d.message);
    assert!(d.message.contains("`color(year)` on the `point`"), "{}", d.message);
    // One column on every layer is one legend, and draws.
    assert!(refusal(base()
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "country"))
        .layer(Layer::new(Mark::Line).encode(Channel::Color, "country"))).is_none());
    // A legend turned off on purpose is the author's choice, and draws.
    let mut quiet = crate::ir::ChannelDef::field("continent");
    quiet.legend = Some(false);
    let mut line = Layer::new(Mark::Line);
    line.encodings.insert(Channel::Color, quiet);
    assert!(refusal(base()
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "country"))
        .layer(line)).is_none());
}

// -- the area mark ------------------------------------------------------

#[test]
fn a_plain_area_is_legal() {
    let spec = base().layer(Layer::new(Mark::Area));
    assert!(check(&spec, &data()).is_empty());
}

#[test]
fn an_area_splits_by_a_category_the_way_a_line_does() {
    // Wilkinson 8.1.5: a categorical variable on an aesthetic splits the
    // graphic. Whatever `line` answers here, `area` must answer too.
    let spec = base().layer(Layer::new(Mark::Area).encode(Channel::Color, "continent"));
    // Legal — it draws. (It also warns that the regions overlap; that is
    // `a_split_area_says_the_regions_will_overlap`, and an Assumption is
    // guidance, not a refusal.)
    assert!(check(&spec, &data()).iter().all(|d| !d.is_fatal()));
    // A *category* splits both marks alike, which is the claim above.
    // They part on the other reading of `color`, and the split is stated in
    // `a_region_refuses_the_measured_color_a_stroke_takes`: a stroke has a
    // length for a measure to vary along, a region has an interior.
    for m in [Mark::Area, Mark::Line] {
        let r = rule_for(&m, &Channel::Color);
        assert!(r.renders.is_some(), "{m:?} draws a split");
        assert!(matches!(r.accepts, VarType::Discrete | VarType::Either),
                "{m:?} takes a category on color, got {:?}", r.accepts);
    }
}

#[test]
fn a_region_refuses_the_measured_color_a_stroke_takes() {
    // Where the stroke marks and the region marks part, and why. A stroke has
    // a *length*, so a measure can vary along it and `color` reads off the
    // piece you are looking at. A region has an *interior*: coloring it by a
    // measure is a gradient fill, a different visual and a much larger job
    // than segmenting a stroke. So this is a boundary with a reason behind
    // it, not the leftover of the old discrete-only rule.
    for m in [Mark::Area, Mark::Ribbon] {
        let spec = base().layer(Layer::new(m.clone()).encode(Channel::Color, "gdp"));
        assert!(
            kinds(&check(&spec, &data())).contains(&DiagnosticKind::Illegal),
            "{m:?} should refuse a measured color"
        );
    }
}

#[test]
fn an_area_takes_opacity_as_a_setting_and_refuses_it_as_a_channel() {
    // One region, one fill. A row here is a vertex of the boundary, not a
    // region of its own, so there is nothing for a per-row opacity to vary
    // across — `line`'s reasoning, one dimension up.
    let mapped = base().layer(Layer::new(Mark::Area).encode(Channel::Opacity, "gdp"));
    assert!(kinds(&check(&mapped, &data())).contains(&DiagnosticKind::Illegal));

    let set = base().layer(Layer::new(Mark::Area).style_opacity(0.4));
    assert!(check(&set, &data()).is_empty(), "{:?}", check(&set, &data()));
}

#[test]
fn an_area_has_no_size_to_set_where_a_line_does() {
    // The one place `area` and `line` diverge, and it is not an exception
    // but a consequence: a stroke's width is free, an area's extent is
    // pinned by x, y and the baseline. Wilkinson ch. 10 — "size for area is
    // a data attribute, not an arbitrary value we may change".
    assert!(rule_for(&Mark::Line, &Channel::Size).settable);
    assert!(!rule_for(&Mark::Area, &Channel::Size).settable);

    let spec = base().layer(Layer::new(Mark::Area).style_size(6.0));
    assert!(
        !check(&spec, &data()).is_empty(),
        "setting a size on an area should be refused"
    );
}

#[test]
fn step_is_the_line_family_so_it_shares_lines_rules() {
    // `step` differs from `line` only in the renderer (right angles, not
    // slants), so every legality verdict must match — anything else would be
    // a per-mark exception the No Exceptions law forbids.
    for ch in [Channel::X, Channel::Y, Channel::Color, Channel::Group,
               Channel::Size, Channel::Opacity, Channel::Shape, Channel::Pattern, Channel::Label] {
        let (line, step) = (rule_for(&Mark::Line, &ch), rule_for(&Mark::Step, &ch));
        assert_eq!(line.obligation, step.obligation, "obligation mismatch on {ch:?}");
        assert_eq!(line.settable, step.settable, "settable mismatch on {ch:?}");
        assert_eq!(line.renders.is_some(), step.renders.is_some(), "renders mismatch on {ch:?}");
        // `accepts` too, which this test used to leave unchecked — and it is
        // the field a widening moves. `color` going `Discrete` → `Either` on
        // `line` alone would have been exactly the per-mark exception the
        // test exists to catch, and it would have passed.
        assert_eq!(line.accepts, step.accepts, "accepts mismatch on {ch:?}");
    }
    // And it draws — never refused as unbuilt like `surface`.
    let spec = base().layer(Layer::new(Mark::Step));
    assert!(check(&spec, &data()).is_empty(), "a plain step should be legal: {:?}", check(&spec, &data()));
}

#[test]
fn a_bar_takes_a_border_and_other_marks_refuse_it() {
    // `style(border_color =, border_size =)` is the outline of a filled mark.
    // A bar takes it; a stroke mark (line/step) and an area refuse with
    // direction rather than draw nothing; a bad color is Illegal.
    let ok = base().layer(Layer::new(Mark::Bar).style_border("white", 1.5));
    assert!(check(&ok, &data()).is_empty(), "a bar takes a border: {:?}", check(&ok, &data()));

    for m in [Mark::Line, Mark::Step, Mark::Area] {
        let spec = base().layer(Layer::new(m.clone()).style_border("black", 1.0));
        let d = check(&spec, &data());
        assert!(!d.is_empty(), "{m:?} must refuse a border with direction");
    }

    let bad = base().layer(Layer::new(Mark::Bar).style_border("borgundy", 1.0));
    assert_eq!(kinds(&check(&bad, &data())), vec![DiagnosticKind::Illegal],
        "a bad border color is Illegal");

    // `border_size = 0` means "no border" and is legal — a bar with a fill and
    // no outline; a negative width is not.
    let zero = base().layer(Layer::new(Mark::Bar).style_border("white", 0.0));
    assert!(check(&zero, &data()).is_empty(), "border_size = 0 is legal (no border): {:?}", check(&zero, &data()));
    let neg = base().layer(Layer::new(Mark::Bar).style_border("white", -1.0));
    assert_eq!(kinds(&check(&neg, &data())), vec![DiagnosticKind::Illegal],
        "a negative border width is refused");
}

#[test]
fn a_border_on_a_cross_is_refused_and_a_crossed_category_is_named() {
    // A cross is two strokes with no fill. Set on every point, the border drew
    // nothing and the plot was byte for byte the one without it, silently.
    let crossed = |w: f64| {
        let mut l = Layer::new(Mark::Point).style_border("red", w);
        l.style.shape = Some("cross".into());
        check(&base().layer(l), &data())
    };
    let d = crossed(2.0);
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(d[0].message.contains("`style(shape = \"circle\")`"), "{}", d[0].message);
    assert!(crossed(0.0).is_empty(), "a size of 0 asks for no border: {:?}", msgs(&crossed(0.0)));
    let mut circle = Layer::new(Mark::Point).style_border("red", 2.0);
    circle.style.shape = Some("circle".into());
    assert!(check(&base().layer(circle), &data()).is_empty());

    // Mapped, the fifth category draws as a cross and loses its border while the
    // rest keep theirs: legal, and said, naming the category.
    let five = HashMap::from([("t".to_string(), DataFrame::new()
        .with_float("gdp", vec![1.0, 2.0, 3.0, 4.0, 5.0])
        .with_float("life", vec![4.0, 5.0, 6.0, 7.0, 8.0])
        .with_str("continent", ["Asia", "Europe", "Africa", "Americas", "Oceania"]
            .map(String::from).to_vec()))]);
    let mapped = base().layer(Layer::new(Mark::Point)
        .encode(Channel::Shape, "continent").style_border("red", 2.0));
    let d = check(&mapped, &five);
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Assumption
        && x.message.contains("draws `Oceania` as a `cross`")), "{:?}", msgs(&d));
    assert!(d.iter().all(|x| !x.is_fatal()), "{:?}", msgs(&d));
    // Four categories draw no cross, and say nothing.
    let four = base().layer(Layer::new(Mark::Point)
        .encode(Channel::Shape, "continent").style_border("red", 2.0));
    assert!(check(&four, &data()).iter().all(|x| !x.message.contains("cross")));
}

#[test]
fn a_split_area_says_the_regions_will_overlap() {
    // An Assumption, not a refusal: the picture is legal, but "overlay"
    // rather than "stack" is a choice the engine made, and §12 only lets a
    // default stay silent when there is one sensible value.
    let spec = base().layer(Layer::new(Mark::Area).encode(Channel::Color, "continent"));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Assumption]);
    assert!(!d[0].is_fatal(), "an overlap must never block the render");
    assert!(d[0].message.contains("opacity"), "{}", d[0].message);
}

#[test]
fn a_set_opacity_answers_the_overlap_question() {
    // The caller has already thought about it; saying so again is noise.
    let spec = base().layer(
        Layer::new(Mark::Area).encode(Channel::Color, "continent").style_opacity(0.5),
    );
    assert!(check(&spec, &data()).is_empty());
}

#[test]
fn one_region_cannot_overlap_anything() {
    let spec = base().layer(Layer::new(Mark::Area));
    assert!(check(&spec, &data()).is_empty());
}

#[test]
fn a_refusal_about_an_area_reads_as_english() {
    // Three templates hardcoded "a {mark}", which was invisible until a
    // mark began with a vowel. `area` is the first, and it made the
    // diagnostics read "a area has no size to set".
    let refusals = [
        check(
            &base().layer(Layer::new(Mark::Area).style_size(4.0)),
            &data(),
        ),
        check(
            &base().layer(Layer::new(Mark::Area).encode(Channel::Shape, "continent")),
            &data(),
        ),
        check(
            &PlotSpec::new().data("t").x("gdp").layer(Layer::new(Mark::Area)),
            &data(),
        ),
    ];
    for d in refusals.iter().flatten() {
        assert!(
            !d.message.contains("a area"),
            "ungrammatical refusal: {}", d.message
        );
    }
    // And the article really is being chosen, not just absent.
    let d = check(&base().layer(Layer::new(Mark::Area).style_size(4.0)), &data());
    assert!(d[0].message.contains("an area"), "{}", d[0].message);
}

// -- marks this engine cannot draw *here* --------------------------------

#[test]
fn a_mark_that_cannot_draw_is_refused_before_it_renders_nothing() {
    // The bug this closes: `area` passed the check and drew an empty panel
    // with exit 0 and no diagnostic. The per-channel `renders: None` arm
    // could not catch it — x and y are plot-scoped, so a layer carrying no
    // encodings of its own was never asked a single question.
    //
    // **The list of marks with no renderer is now empty**, `surface` having been
    // the last (2026-07-26), so it is asserted empty rather than looped over: a
    // green run has to mean *nothing in the kernel is unbuilt*, not *the loop
    // found nothing to check*, and a vacuous loop cannot tell the two apart.
    assert!(
        ALL_MARKS.iter().all(is_drawable),
        "a mark with no renderer is back — loop over it here and give it direction"
    );

    // The live form of the same failure is one space over rather than gone: a
    // `surface` draws in the cube and nowhere else, so a flat one would render an
    // empty panel exactly as `area` did. It is refused as **Illegal** and not
    // `Unsupported`, and the difference is the point — a flat sheet is not an
    // unbuilt feature, it is an incomplete syllable (Law 7), the same kind
    // `interval` gets with no range transform.
    let spec = base().layer(Layer::new(Mark::Surface));
    let diags = check(&spec, &data());
    assert_eq!(
        diags.len(), 1,
        "a flat surface should give exactly one diagnostic, got {diags:?}"
    );
    assert_eq!(diags[0].kind, DiagnosticKind::Illegal);
    assert!(
        diags[0].message.contains(mark_name(&Mark::Surface)),
        "the message must name the mark: {}", diags[0].message
    );
}

#[test]
fn a_flat_surface_is_given_both_routes_into_the_cube_and_the_flat_alternative() {
    // Law 5. Every refusal says what to do instead — and this one has three things
    // to say, because "needs `z()`" would be true and useless to a reader who
    // wanted a sheet in the plane.
    let spec = base().layer(Layer::new(Mark::Surface));
    let msg = &check(&spec, &data())[0].message;
    assert!(msg.contains("z("), "bind the height: {msg}");
    assert!(msg.contains("space()"), "or synthesize it under space(): {msg}");
    assert!(msg.contains("`zone`"), "the field in the plane is a zone's: {msg}");
}

/// A grid table and the 3-D spec that draws it as a sheet — the fixture the
/// surface checks need, since `data()`'s three rows are a line and not a mesh.
fn grid_data(nx: usize, ny: usize) -> HashMap<String, DataFrame> {
    let (mut xs, mut ys, mut hs) = (Vec::new(), Vec::new(), Vec::new());
    for j in 0..ny {
        for i in 0..nx {
            xs.push(i as f64);
            ys.push(j as f64);
            hs.push((i * j) as f64);
        }
    }
    HashMap::from([(
        "t".to_string(),
        DataFrame::new().with_float("x", xs).with_float("y", ys).with_float("h", hs),
    )])
}

fn grid_spec() -> PlotSpec {
    PlotSpec::new().data("t").x("x").y("y").z("h")
}

#[test]
fn surface_takes_the_two_floor_transforms_and_the_six_that_reduce_into_them() {
    // **A surface needs a floor whose cells tile without gaps, and two transforms
    // give it one** — `density` as nodes to interpolate between, `bin` as cells to
    // lay lids on. The six reductions ride `bin`, exactly as they ride it on a 3-D
    // `bar` (Law 2). Pinned across the whole row rather than sampled, so a later
    // transform cannot quietly join one.
    for t in USER_TRANSFORMS {
        let expect = match t {
            Transform::Density | Transform::Bin => TransformLegality::Combines,
            Transform::Sum | Transform::Mean | Transform::Median
            | Transform::Max | Transform::Min | Transform::Quantile => TransformLegality::Combines,
            _ => TransformLegality::None,
        };
        assert_eq!(mark_takes_transform(&Mark::Surface, &t), expect,
            "the grid is wrong for surface * {}", transform_name(&t));
    }
    // And what is refused still says which sentence to write instead. `smooth` is
    // the one worth checking: it is refused for a reason a cell cannot fix, so the
    // message has to name the two floors that do tile rather than apologize.
    let spec = grid_spec().layer(Layer::new(Mark::Surface).transform(Transform::Smooth));
    let msg = msgs(&check(&spec, &grid_data(4, 4))).join(" ");
    assert!(msg.contains("surface * bin"), "toward the terraced sheet: {msg}");
    assert!(msg.contains("surface * density"), "or the estimated field: {msg}");
}

#[test]
fn a_cut_floor_is_a_surface_floor_and_a_slotted_one_is_still_not() {
    // The ruling this feature turned on (spec §15). `bin` cuts adjacent cells, so
    // the lids tile into a stepped sheet and the sentence draws; a category owns a
    // *slot* that leaves air, so lids would float apart and there is no sheet.
    let cut = grid_spec()
        .layer(Layer::new(Mark::Surface).transform(Transform::Bin).transform(Transform::Mean));
    assert!(check(&cut, &grid_data(4, 4)).is_empty(),
        "a cut floor tiles: {:?}", msgs(&check(&cut, &grid_data(4, 4))));

    // A category on either position, still refused: the bare `surface * mean` this
    // rule always turned down, for the reason it always gave.
    let df = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into()])
            .with_float("y", vec![0.0, 1.0, 0.0, 1.0])
            .with_float("h", vec![1.0, 2.0, 3.0, 4.0]),
    )]);
    let slotted = PlotSpec::new().data("t").x("g").y("y").z("h")
        .layer(Layer::new(Mark::Surface).transform(Transform::Mean));
    let msg = msgs(&check(&slotted, &df)).join(" ");
    assert!(!msg.is_empty(), "a slotted floor is not a sheet");
}

#[test]
fn a_scatter_cannot_be_a_surface_and_is_told_which_two_sentences_can() {
    // The empty panel this project refuses above all others, arriving as geometry:
    // *n* points in general position describe an *n*×*n* lattice with *n* nodes in
    // it and not one complete block of four. Fatal on that condition exactly —
    // there is no fraction to tune.
    let scatter = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![0.11, 0.42, 0.77, 0.93, 0.28])
            .with_float("y", vec![0.51, 0.13, 0.88, 0.34, 0.67])
            .with_float("h", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
    )]);
    let spec = grid_spec().layer(Layer::new(Mark::Surface));
    let d = check(&spec, &scatter);
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(d[0].message.contains("`point`"), "for a cloud: {}", d[0].message);
    assert!(d[0].message.contains("surface * density"), "to estimate: {}", d[0].message);

    // The same scatter *with* the transform is the sentence the refusal advises,
    // so it must be clean — the check asks about the reader's mesh, never about one
    // a transform is going to make. Getting this wrong refused `surface * density`
    // as a scatter, quoting the sentence that had just been written.
    let estimated = PlotSpec::new().data("t").x("x").y("y")
        .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        .layer(Layer::new(Mark::Surface).transform(Transform::Density));
    assert!(check(&estimated, &scatter).is_empty(), "{:?}", msgs(&check(&estimated, &scatter)));
}

#[test]
fn a_layer_may_name_its_own_measurement_on_color_whichever_channel_carries_it() {
    // **One transform, one behavior, on every mark that can draw it** (Law 2).
    // `density` publishes its estimate under the name `density`, and a reader may
    // say that name out loud on `color`. That worked on a `zone`, which measures
    // *by* color, and was refused on a `surface`, which measures up along `z` — so
    // the exemption was keyed on the mark's measure channel rather than on the
    // column, and the same sentence was legal under one mark and a misspelling
    // under another. The refusal also sent the reader after a typo that was not
    // there, which is the one thing §12 says a message must never do.
    let scatter = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![0.11, 0.42, 0.77, 0.93, 0.28])
            .with_float("y", vec![0.51, 0.13, 0.88, 0.34, 0.67]),
    )]);
    let sheet = |color: &str| {
        PlotSpec::new().data("t").x("x").y("y")
            .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
            .layer(
                Layer::new(Mark::Surface)
                    .transform(Transform::Density)
                    .encode(Channel::Color, color),
            )
    };
    let d = check(&sheet("density"), &scatter);
    assert!(d.is_empty(), "the sheet may be ramped by its own estimate: {:?}", msgs(&d));

    // **Named, not blanket.** The exemption is for this layer's measurement alone,
    // so a misspelling of it — and any other absent column — still lands where it
    // did. Without this the check would wave every color binding through on any
    // mark that measures something, which is a wider hole than the one it closes.
    for wrong in ["denstiy", "nosuchcolumn"] {
        let d = check(&sheet(wrong), &scatter);
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{wrong}: {:?}", msgs(&d));
        assert!(d[0].message.contains("not in the data"), "{wrong}: {}", d[0].message);
    }

    // And the flat readings are untouched — the half that always worked, pinned so
    // widening the question cannot narrow it.
    let flat = PlotSpec::new().data("t").x("x").y("y").layer(
        Layer::new(Mark::Zone).transform(Transform::Density).encode(Channel::Color, "density"),
    );
    assert!(check(&flat, &scatter).is_empty(), "{:?}", msgs(&check(&flat, &scatter)));
}

#[test]
fn a_mark_whose_color_cannot_hold_a_number_says_so_rather_than_blaming_the_spelling() {
    // The other side of the exemption, and the reason it is gated rather than
    // general. A 3-D `bar` measures with `z` exactly as a surface does, and takes a
    // **categorical** color only — a column is a region, which is `area`'s ruling.
    // So `color(count)` there must stay refused: waving it through would hand the
    // renderer a binding it drops in silence, and a silently ignored binding is the
    // silent letter this grammar is built against. What it must not do is call a
    // correctly spelled word a typo.
    let df = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", vec![0.1, 0.4, 0.7, 0.9])
            .with_float("y", vec![0.5, 0.1, 0.8, 0.3]),
    )]);
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        .layer(
            Layer::new(Mark::Bar)
                .transform(Transform::Bin)
                .encode(Channel::Color, "count"),
        );
    let d = check(&spec, &df);
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(!d[0].message.contains("spelling"), "the word is spelled right: {}", d[0].message);
    assert!(d[0].message.contains("categorical"), "say what color needs: {}", d[0].message);
}

#[test]
fn a_grid_with_a_gap_draws_and_counts_the_crossings_it_is_missing() {
    // Legitimate and reported: a response surface can be missing a cell, so
    // refusing it would be taste enforced as legality (Law 8) — but a mesh silently
    // full of holes is how a lattice recovered from rounded coordinates would look
    // like one with a designed opening.
    let full = grid_data(4, 4);
    let df = full.get("t").unwrap();
    let (xs, ys, hs) = (
        df.float_col("x").unwrap().to_vec(),
        df.float_col("y").unwrap().to_vec(),
        df.float_col("h").unwrap().to_vec(),
    );
    let keep: Vec<usize> = (0..xs.len()).filter(|&i| i != 5).collect();
    let holed = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("x", keep.iter().map(|&i| xs[i]).collect())
            .with_float("y", keep.iter().map(|&i| ys[i]).collect())
            .with_float("h", keep.iter().map(|&i| hs[i]).collect()),
    )]);
    let d = check(&grid_spec().layer(Layer::new(Mark::Surface)), &holed);
    assert_eq!(kinds(&d), vec![DiagnosticKind::Assumption], "{:?}", msgs(&d));
    assert!(d[0].message.contains("15 of 16"), "count both: {}", d[0].message);

    // And a whole grid says nothing at all, or the report would be noise.
    assert!(check(&grid_spec().layer(Layer::new(Mark::Surface)), &full).is_empty());
}

#[test]
fn a_surface_refuses_a_category_on_the_floor_and_names_the_bar_that_takes_one() {
    // Where the sheet parts from the 3-D bar: a face spans the gap between two
    // samples, and between two categories there is nothing to span. The refusal
    // gives the mark whose floor *may* be slotted, which is the non-obvious half.
    let mut d3 = grid_data(4, 4);
    let df = d3.remove("t").unwrap();
    let n = df.float_col("y").unwrap().len();
    d3.insert(
        "t".to_string(),
        df.with_str("cat", (0..n).map(|i| format!("g{}", i % 2)).collect()),
    );
    let spec = PlotSpec::new().data("t").x("cat").y("y").z("h")
        .layer(Layer::new(Mark::Surface));
    let d = check(&spec, &d3);
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(d[0].message.contains("bar * count"), "toward the 3-D bar: {}", d[0].message);

    // **And the sentence it names has to be one the reader can write.** This
    // assertion used to read `bar * bin`, pinning a direction that was *itself
    // refused* — `bin` cuts a continuous axis, so anyone who followed it hit a
    // second wall naming the same category (fixed 2026-07-28). String-matching a
    // message proves only that the words are there; building the recommended
    // spec and checking it is what proves the advice.
    let recommended = PlotSpec::new().data("t").x("cat").y("cat2")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count))
        .coord(CoordSpace::Space(crate::ir::SpaceView { turn: 30.0, tilt: 25.0 }));
    let mut d4 = grid_data(4, 4);
    let df4 = d4.remove("t").unwrap();
    let n4 = df4.float_col("y").unwrap().len();
    d4.insert("t".to_string(), df4
        .with_str("cat", (0..n4).map(|i| format!("g{}", i % 2)).collect())
        .with_str("cat2", (0..n4).map(|i| format!("h{}", i % 3)).collect()));
    assert!(check(&recommended, &d4).iter().all(|x| !x.is_fatal()),
        "the refusal's own direction must draw: {:?}", msgs(&check(&recommended, &d4)));
}

#[test]
fn levels_are_refused_on_a_surface_toward_the_two_marks_that_band_a_field() {
    // A level set is a region *in the plane*, and a surface has already spent the
    // third axis on the measurement — so a band on a sheet could only be a color,
    // which is `zone`'s reading of the same request. Refused with direction rather
    // than accepted and ignored, which is what `zone * density(levels = )` itself
    // did for a while.
    let spec = PlotSpec::new().data("t").x("x").y("y")
        .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        .layer({
            let mut l = Layer::new(Mark::Surface).transform(Transform::Density);
            l.density = Some(crate::ir::DensitySpec {
                adjust: None, bandwidth: None, levels: Some(5), compare: None, reach: None, support: (None, None),
            });
            l
        });
    let d = check(&spec, &grid_data(6, 6));
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(d[0].message.contains("zone * density"), "{}", d[0].message);
    assert!(d[0].message.contains("path * density"), "{}", d[0].message);
}

#[test]
fn path_takes_only_the_field_transform_and_the_refusal_names_line() {
    // `density` and `cluster` are the **two** columns of the grid a path takes,
    // and every other is `None` — pinned across the whole row rather than a
    // sample, so a later transform cannot quietly join one. Both exceptions
    // clear the row's own bar: their output is vertices in traversal order,
    // not one summary per key — the contour's rings, the tree's elbows.
    for t in USER_TRANSFORMS {
        let expect = if t == Transform::Density || t == Transform::Cluster {
            TransformLegality::Combines
        } else {
            TransformLegality::None
        };
        assert_eq!(mark_takes_transform(&Mark::Path, &t), expect,
            "the grid is wrong for path * {}", transform_name(&t));
        if expect == TransformLegality::None {
            let spec = base().layer(Layer::new(Mark::Path).transform(t.clone()));
            let d = check(&spec, &data());
            assert!(d.iter().any(|x| x.is_fatal()),
                "path * {} should be refused: {:?}", transform_name(&t), msgs(&d));
        }
    }
    // Law 5: the refusal names the mark to use instead.
    let spec = base().layer(Layer::new(Mark::Path).transform(Transform::Mean));
    assert!(check(&spec, &data())[0].message.contains("`line"),
        "path * mean should point at line: {:?}", msgs(&check(&spec, &data())));
    // And the one it takes is *not* refused — the contour, which needs both
    // positions and gets them from `base()`.
    let ok = base().layer(Layer::new(Mark::Path).transform(Transform::Density));
    assert!(!check(&ok, &data()).iter().any(|x| x.is_fatal()),
        "path * density is the contour: {:?}", msgs(&check(&ok, &data())));
}

// -----------------------------------------------------------------------
// Cluster — the tree of merges and the seriated tile plot (spec §5)
// -----------------------------------------------------------------------

/// A complete 3-leaf × 2-coordinate table, plus the numeric column a wrong
/// sentence needs. Complete on purpose: every data-fact refusal below edits
/// exactly one thing away from this.
fn cluster_table() -> HashMap<String, DataFrame> {
    let df = DataFrame::new()
        .with_str("food", vec!["rice".into(), "rice".into(),
                               "oats".into(), "oats".into(),
                               "beans".into(), "beans".into()])
        .with_str("nutrient", vec!["iron".into(), "protein".into(),
                                   "iron".into(), "protein".into(),
                                   "iron".into(), "protein".into()])
        .with_float("amount", vec![0.8, 2.7, 1.8, 3.4, 2.1, 8.5])
        .with_float("price", vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
    HashMap::from([("t".to_string(), df)])
}

fn tree_spec() -> PlotSpec {
    PlotSpec::new().data("t").x("food")
        .layer(Layer::new(Mark::Path).cluster(Some("amount"), Some("nutrient")))
}

fn tile_spec() -> PlotSpec {
    PlotSpec::new().data("t").x("food").y("nutrient")
        .layer(Layer::new(Mark::Zone).cluster(None, Some("nutrient"))
            .encode(Channel::Color, "amount"))
}

/// The two readings draw and every sentence-fact malformation refuses with
/// its direction — the wrong mark toward the two sentences that exist, the
/// bound distance axis, the missing leaf axis, the unnamed value, the
/// profile axis naming the leaves, and the aesthetics no reading holds.
#[test]
fn a_cluster_refuses_its_sentence_malformations_with_direction() {
    // Both readings are legal as written.
    assert!(!check(&tree_spec(), &cluster_table()).iter().any(|d| d.is_fatal()),
        "the tree draws: {:?}", msgs(&check(&tree_spec(), &cluster_table())));
    assert!(!check(&tile_spec(), &cluster_table()).iter().any(|d| d.is_fatal()),
        "the seriated tile plot draws: {:?}", msgs(&check(&tile_spec(), &cluster_table())));

    // A mark with no reading, sent to the two that have one.
    let d = check(&PlotSpec::new().data("t").x("food")
        .layer(Layer::new(Mark::Bar).cluster(Some("amount"), Some("nutrient"))),
        &cluster_table());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("path * cluster") && x.message.contains("zone * cluster")),
        "bar under cluster is directed to the two readers: {:?}", msgs(&d));

    // Both positions bound: the distance axis is the tree's own, so a column of
    // the table there is refused, toward the name the tree writes.
    let d = check(&PlotSpec::new().data("t").x("food").y("amount")
        .layer(Layer::new(Mark::Path).cluster(Some("amount"), Some("nutrient"))),
        &cluster_table());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("+ y(distance, limits = …)`")),
        "a table column on the distance axis is refused: {:?}", msgs(&d));

    // No position bound: the leaves have no axis.
    let d = check(&PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Path).cluster(Some("amount"), Some("nutrient"))),
        &cluster_table());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("leaves")),
        "no leaf axis is refused: {:?}", msgs(&d));

    // Nothing names the value.
    let d = check(&PlotSpec::new().data("t").x("food")
        .layer(Layer::new(Mark::Path).cluster(None, Some("nutrient"))),
        &cluster_table());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("value column")),
        "an unnamed value is refused: {:?}", msgs(&d));

    // The profile axis is the leaf axis.
    let d = check(&PlotSpec::new().data("t").x("food")
        .layer(Layer::new(Mark::Path).cluster(Some("amount"), Some("food"))),
        &cluster_table());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("its own profile")),
        "over = the leaf axis is refused: {:?}", msgs(&d));

    // The aesthetics the tree cannot split by, and the plane it stays in.
    let aesthetic_cases: [(fn(Layer) -> Layer, &str); 3] = [
        (|l| l.encode(Channel::Color, "nutrient"), "no column is left"),
        (|l| l.encode(Channel::Group, "nutrient"), "no column is left"),
        (|l| l.encode(Channel::Z, "price"), "no depth"),
    ];
    for (build, needle) in aesthetic_cases {
        let d = check(&PlotSpec::new().data("t").x("food")
            .layer(build(Layer::new(Mark::Path)
                .cluster(Some("amount"), Some("nutrient")))),
            &cluster_table());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains(needle)),
            "expected `{needle}`: {:?}", msgs(&d));
    }

    // The waits: polar, play, facet — valid grammar, not drawn yet.
    let polar = PlotSpec::new().data("t").x("food")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(Layer::new(Mark::Path).cluster(Some("amount"), Some("nutrient")));
    assert!(check(&polar, &cluster_table()).iter()
        .any(|x| x.kind == DiagnosticKind::Unsupported),
        "a circular tree waits: {:?}", msgs(&check(&polar, &cluster_table())));
    let mut faceted = tree_spec();
    faceted.facet = Some(crate::ir::FacetSpec {
        col: Some("nutrient".into()), ..Default::default()
    });
    assert!(check(&faceted, &cluster_table()).iter()
        .any(|x| x.kind == DiagnosticKind::Unsupported),
        "a faceted cluster waits: {:?}", msgs(&check(&faceted, &cluster_table())));

    // `order()` beside `cluster` is two orders for one axis; sorting the
    // profile axis of the tile plot by itself is not.
    let ordered = tree_spec().order("food");
    assert!(check(&ordered, &cluster_table()).iter().any(|x|
        x.kind == DiagnosticKind::Illegal && x.message.contains("one order")),
        "order + cluster refuses: {:?}", msgs(&check(&ordered, &cluster_table())));
    let profile_sorted = tile_spec().order_desc("nutrient");
    assert!(!check(&profile_sorted, &cluster_table()).iter().any(|d| d.is_fatal()),
        "ordering the profile axis stays legal: {:?}",
        msgs(&check(&profile_sorted, &cluster_table())));
}

/// The tile reading's own names: the profile axis must be named and be one
/// of the two positions, and a second value name must agree with `color`.
#[test]
fn a_clustered_tile_plot_names_its_profile_axis_and_agrees_with_color() {
    let d = check(&PlotSpec::new().data("t").x("food").y("nutrient")
        .layer(Layer::new(Mark::Zone).cluster(None, None)
            .encode(Channel::Color, "amount")),
        &cluster_table());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("which one profiles")),
        "an unnamed profile axis is refused: {:?}", msgs(&d));

    let d = check(&PlotSpec::new().data("t").x("food").y("nutrient")
        .layer(Layer::new(Mark::Zone).cluster(None, Some("region"))
            .encode(Channel::Color, "amount")),
        &cluster_table());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("neither axis")),
        "a profile axis off the plot is refused: {:?}", msgs(&d));

    let d = check(&PlotSpec::new().data("t").x("food").y("nutrient")
        .layer(Layer::new(Mark::Zone).cluster(Some("price"), Some("nutrient"))
            .encode(Channel::Color, "amount")),
        &cluster_table());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("different columns")),
        "a value disagreeing with color is refused: {:?}", msgs(&d));

    let d = check(&PlotSpec::new().data("t").x("food").y("nutrient")
        .layer(Layer::new(Mark::Zone).cluster(None, Some("nutrient"))),
        &cluster_table());
    assert!(d.iter().any(|x| x.is_fatal()),
        "no value and no color is refused: {:?}", msgs(&d));
}

/// The data facts: a column that is not there or not the right kind, a
/// numeric leaf axis, one leaf, a profile with a hole, a cell said twice —
/// each named, each with a direction that keeps the work in the data.
#[test]
fn a_cluster_refuses_its_data_malformations_by_name() {
    let refused = |spec: &PlotSpec, table: &HashMap<String, DataFrame>, needle: &str| {
        let d = check(spec, table);
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains(needle)),
            "expected `{needle}`: {:?}", msgs(&d));
    };

    // Unknown and mistyped columns.
    refused(&PlotSpec::new().data("t").x("food")
        .layer(Layer::new(Mark::Path).cluster(Some("mass"), Some("nutrient"))),
        &cluster_table(), "does not have");
    refused(&PlotSpec::new().data("t").x("food")
        .layer(Layer::new(Mark::Path).cluster(Some("nutrient"), None)),
        &cluster_table(), "holds categories");
    refused(&PlotSpec::new().data("t").x("food")
        .layer(Layer::new(Mark::Path).cluster(Some("amount"), Some("price"))),
        &cluster_table(), "indexed by names");

    // A numeric leaf axis has no levels to join.
    refused(&PlotSpec::new().data("t").x("price")
        .layer(Layer::new(Mark::Path).cluster(Some("amount"), Some("nutrient"))),
        &cluster_table(), "no levels to join");

    // One leaf joins nothing.
    let one = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("food", vec!["rice".into(), "rice".into()])
        .with_str("nutrient", vec!["iron".into(), "protein".into()])
        .with_float("amount", vec![0.8, 2.7]))]);
    refused(&tree_spec(), &one, "at least two leaves");

    // A hole in a profile, named by its pair.
    let holed = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("food", vec!["rice".into(), "rice".into(), "oats".into()])
        .with_str("nutrient", vec!["iron".into(), "protein".into(), "iron".into()])
        .with_float("amount", vec![0.8, 2.7, 1.8]))]);
    refused(&tree_spec(), &holed, "no value at `nutrient` = protein");

    // A cell said twice, named by its pair.
    let doubled = HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("food", vec!["rice".into(), "rice".into(), "rice".into(),
                               "oats".into(), "oats".into()])
        .with_str("nutrient", vec!["iron".into(), "iron".into(), "protein".into(),
                                   "iron".into(), "protein".into()])
        .with_float("amount", vec![0.8, 0.9, 2.7, 1.8, 3.4]))]);
    refused(&tree_spec(), &doubled, "holds 2 rows");

    // Several rows per leaf with no profile axis named: the one implicit
    // coordinate is duplicated, so the duplicate refusal carries it.
    let unaligned = PlotSpec::new().data("t").x("food")
        .layer(Layer::new(Mark::Path).cluster(Some("amount"), None));
    refused(&unaligned, &cluster_table(), "one value");
}

#[test]
fn a_paths_positions_are_the_glyph_marks_row_not_the_line_familys() {
    // §6's role test, asserted against `point` rather than restated, so the
    // two cannot drift: a path's two axes are both *positions* (neither is a
    // domain, because nothing is sorted), which is exactly a glyph's case.
    // Compared with `line` in the same breath so the parting is explicit.
    for ch in [Channel::X, Channel::Y] {
        let p = rule_for(&Mark::Point, &ch);
        let path = rule_for(&Mark::Path, &ch);
        assert_eq!((path.obligation, path.accepts, path.renders),
                   (p.obligation, p.accepts, p.renders),
                   "path's {ch:?} must match point's");
    }
    // And it is a real difference, not a coincidence: `line`'s y is narrower.
    assert_eq!(rule_for(&Mark::Line, &Channel::Y).accepts, VarType::Continuous);
    assert_eq!(rule_for(&Mark::Path, &Channel::Y).accepts, VarType::Either);
}

#[test]
fn arrow_is_a_path_and_edge_setting_and_every_other_mark_refuses_it_with_direction() {
    // The two strokes whose ends the data chose: a `path` runs from its first
    // row to its last, an `edge` from `layout(from, to)`'s first column to its
    // second (`edge` joined 2026-09-24).
    for m in &ALL_MARKS {
        if !is_drawable(m) { continue }
        let style = StyleSpec { arrow: Some("end".into()), ..Default::default() };
        let mut layer = Layer::new(m.clone());
        layer.style = style;
        // `interval`/`ribbon` need a range transform, `text` a label — give
        // them their minimum syllable so the only thing under test is `arrow`.
        let layer = match m {
            Mark::Interval | Mark::Ribbon => layer.transform(Transform::Range),
            Mark::Text => { layer.encode(Channel::Label, "continent") }
            _ => layer,
        };
        let d = check(&base().layer(layer), &data());
        let refused = d.iter().any(|x| x.message.contains("`style(arrow = )`"));
        assert_eq!(refused, !matches!(m, Mark::Path | Mark::Edge),
            "{m:?} arrow: refused={refused}, but only path and edge take it: {:?}", msgs(&d));
        if refused {
            assert!(d.iter().any(|x| x.message.contains("Use `path`")),
                "{m:?}: the refusal must give direction: {:?}", msgs(&d));
        }
    }
}

#[test]
fn an_arrow_end_that_is_not_an_end_is_refused_with_the_three_that_are() {
    let mut layer = Layer::new(Mark::Path);
    layer.style = StyleSpec { arrow: Some("head".into()), ..Default::default() };
    let d = check(&base().layer(layer), &data());
    assert!(d.iter().any(|x| x.message.contains("is not an end")
                             && x.message.contains("\"both\"")),
        "an unknown end must be refused with the legal ones: {:?}", msgs(&d));
    // And each legal one passes.
    for end in ARROW_ENDS {
        let mut layer = Layer::new(Mark::Path);
        layer.style = StyleSpec { arrow: Some(end.into()), ..Default::default() };
        assert!(check(&base().layer(layer), &data()).is_empty(), "{end} should be legal");
    }
}

#[test]
fn a_path_has_no_border_because_a_stroke_has_no_border() {
    // The settable rule: `border_*` spans the closed-glyph fills, and a path
    // is a stroke — the same answer `line` gives, which is the point.
    let mut layer = Layer::new(Mark::Path);
    layer.style = StyleSpec { border_color: Some("black".into()), ..Default::default() };
    let d = check(&base().layer(layer), &data());
    assert!(d.iter().any(|x| x.message.contains("has no separate border")),
        "path should refuse a border: {:?}", msgs(&d));
}

#[test]
fn a_path_draws_and_is_no_longer_refused() {
    // The other half of the change above: `path` used to be refused with a
    // direction toward `line`, and that refusal is now the mark itself.
    let spec = base().layer(Layer::new(Mark::Path));
    assert!(check(&spec, &data()).is_empty(), "{:?}", check(&spec, &data()));
}

// -- scales -----------------------------------------------------------

#[test]
fn a_log_scale_over_positive_numbers_is_fine() {
    let spec = PlotSpec::new().data("t")
        .x_scaled("gdp", ScaleType::Log).y("life")
        .layer(Layer::new(Mark::Point));
    assert!(check(&spec, &data()).is_empty());
}

#[test]
fn a_logarithm_of_text_is_illegal() {
    let spec = PlotSpec::new().data("t")
        .x_scaled("continent", ScaleType::Log).y("life")
        .layer(Layer::new(Mark::Point));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("is text"), "{}", d[0].message);
}

#[test]
fn a_log_scale_names_the_rows_it_cannot_place() {
    let df = DataFrame::new()
        .with_float("v", vec![1.0, 0.0, -4.0, 100.0])
        .with_float("y", vec![1.0, 2.0, 3.0, 4.0]);
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);

    let spec = PlotSpec::new().data("t")
        .x_scaled("v", ScaleType::Log).y("y")
        .layer(Layer::new(Mark::Point));
    let d = check(&spec, &m);
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    // Directional: says how many, how bad, and what to do about it.
    assert!(d[0].message.contains("2 of 4"), "{}", d[0].message);
    assert!(d[0].message.contains("-4"), "{}", d[0].message);
}

#[test]
fn a_transforms_output_axis_is_not_judged_on_its_raw_column() {
    // `bar * sum` over profits that are individually negative but sum to a
    // positive total is well formed. Refusing it on the raw column would be
    // a false alarm — the values that get scaled do not exist yet.
    let df = DataFrame::new()
        .with_str("cat", vec!["a".into(), "a".into()])
        .with_float("profit", vec![-50.0, 150.0]);
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);

    let spec = PlotSpec::new().data("t")
        .x("cat").y_scaled("profit", ScaleType::Log)
        .layer(Layer::new(Mark::Bar).transform(Transform::Sum));
    assert!(check(&spec, &m).is_empty());
}

#[test]
fn a_transforms_key_axis_is_judged_on_its_raw_column() {
    // The other side of the same rule: `bin` groups by x *before* the scale
    // is irrelevant — the raw column is exactly what gets logged, so a
    // non-positive value there is knowable in advance.
    let df = DataFrame::new().with_float("v", vec![1.0, -1.0, 10.0]);
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);

    let spec = PlotSpec::new().data("t")
        .x_scaled("v", ScaleType::Log)
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    let d = check(&spec, &m);
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
}

/// The third of the three scales the **column's type** chooses, and now the
/// third that may be said out loud for nothing. `linear` on a number and
/// `time` on a date have had this allowance and a test apiece since they
/// shipped; `category` on text was refused until 2026-07-28, which made it the
/// Law-2 exception the other two tests were quietly measuring against.
#[test]
fn a_text_column_needs_no_category_scale_said_out_loud() {
    let spec = PlotSpec::new().data("t")
        .x_scaled("continent", ScaleType::Category).y("life")
        .layer(Layer::new(Mark::Point));
    assert!(check(&spec, &data()).is_empty());
}

/// **Not Unsupported.** Until 2026-07-28 this said "not yet", promising a
/// feature §18 has now ruled out: a scale says *how* a measured column is
/// placed, and whether an axis measures at all is the column's type — the rule
/// `log`-on-text, `linear`-on-a-date and `time`-on-a-number already enforce.
/// One direction per column type, each asserted on what it tells the caller to
/// **do** (§12), because the kind is the half a `kinds()` assertion can see and
/// the direction is the half it cannot.
#[test]
fn the_category_refusal_gives_each_column_type_its_own_direction() {
    let refusal = |field: &str, frames| {
        let spec = PlotSpec::new().data("t")
            .x_scaled(field, ScaleType::Category).y("life")
            .layer(Layer::new(Mark::Point));
        let d = check(&spec, frames);
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{field}");
        d[0].message.clone()
    };

    let frames = data();

    // Number: removing it hands back the continuous axis the caller was
    // escaping, so the message must not say to — it must say `factor`.
    let number = refusal("gdp", &frames);
    assert!(number.contains("Store `gdp` as text or categories") && !number.contains("factor("),
        "{number}");
    assert!(
        !number.contains("a text column already gets"),
        "the number branch must not repeat the text column's advice: {number}"
    );

    // A date is not a category yet either, and "make it a factor" is not the
    // sentence — one slot per distinct moment is one slot per row.
    let mut dated_life = dated();
    let df = dated_life.remove("t").unwrap().with_float("life", vec![4.0, 5.0, 6.0]);
    dated_life.insert("t".to_string(), df);
    let date = refusal("day", &dated_life);
    assert!(date.contains("one slot per row"), "{date}");
    assert!(!date.contains("factor(day)"), "{date}");

    // Both directions end in "drop the scale", so **follow one exactly** and
    // the plot must draw. String-matching a direction proves the words are
    // present and nothing about whether they are advice — the upgrade the
    // surface refusal's entry asked every `*_with_direction` test to make.
    for m in [&number, &date] {
        assert!(m.contains("drop the scale"), "{m}");
    }
    let converted = DataFrame::new()
        .with_str("gdp", vec!["1".into(), "2".into(), "3".into()])
        .with_float("life", vec![4.0, 5.0, 6.0]);
    let mut after = HashMap::new();
    after.insert("t".to_string(), converted);
    let followed = PlotSpec::new().data("t")
        .x("gdp").y("life")
        .layer(Layer::new(Mark::Point));
    assert!(
        check(&followed, &after).is_empty(),
        "doing what the refusal says must render: {:?}",
        check(&followed, &after)
    );

    // The axis a transform **writes** gets a fourth sentence, and needs one:
    // the column's type is not what lands there, so "make `life` text" would
    // be advice about the wrong numbers — and following it would break the
    // summary that produced them. This is the same distinction `check_scale`
    // draws for `log`, one arm over.
    let synth = PlotSpec::new().data("t")
        .x("continent").y_scaled("life", ScaleType::Category)
        .layer(Layer::new(Mark::Bar).transform(Transform::Mean));
    let d = check(&synth, &frames);
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("the transform computed"), "{}", d[0].message);
    assert!(!d[0].message.contains("factor(life)"), "{}", d[0].message);
}

// -- time -------------------------------------------------------------

fn dated() -> HashMap<String, DataFrame> {
    let df = DataFrame::new()
        .with_time("day", vec![0.0, 86_400.0, 172_800.0], crate::time::TimeUnit::Day)
        .with_float("sales", vec![3.0, 5.0, 4.0]);
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);
    m
}

/// A plain-number width does not say its unit, and a time axis is measured in
/// seconds: `bin(width = 7)` on dates cut bins seven seconds wide, an empty
/// panel of 0-pixel bars. Refused wherever the cut lands on the time axis, with
/// the count as the direction; a count, a bare `bin` and a width on a number
/// all still draw.
#[test]
fn a_plain_width_cannot_cut_a_time_axis() {
    let frames = || {
        let mut m = dated();
        let df = m.remove("t").unwrap()
            .with_time("when", vec![3_600.0, 90_000.0, 176_400.0], crate::time::TimeUnit::Second);
        m.insert("t".to_string(), df);
        m
    };
    let binned = |mark: Mark, bins: Option<usize>, width: Option<f64>| {
        let mut l = Layer::new(mark).transform(Transform::Bin);
        l.bin = Some(crate::ir::BinSpec { bins, width, tiling: None });
        l
    };
    let refused = |spec: PlotSpec| -> Option<String> {
        check(&spec, &frames()).into_iter()
            .find(|d| d.kind == DiagnosticKind::Illegal && d.message.contains("does not say what unit"))
            .map(|d| d.message)
    };

    let m = refused(PlotSpec::new().data("t").x("day").layer(binned(Mark::Bar, None, Some(7.0))))
        .expect("a width on dates is refused");
    assert!(m.contains("`bin(width = 7)`") && m.contains("`x(day)` holds dates")
        && m.contains("7 seconds wide") && m.contains("`bin(30)`"), "{m}");
    let m = refused(PlotSpec::new().data("t").x("when").layer(binned(Mark::Bar, None, Some(3600.0))))
        .expect("a width on timestamps is refused too");
    assert!(m.contains("`x(when)` holds times"), "{m}");
    // Wherever the cut lands: the key of a bar on its side, and either axis of a mesh.
    assert!(refused(PlotSpec::new().data("t").y("day").layer(binned(Mark::Bar, None, Some(7.0)))).is_some());
    assert!(refused(PlotSpec::new().data("t").x("day").y("sales")
        .layer(binned(Mark::Zone, None, Some(7.0)))).is_some());

    // What still draws: a count, `bins` winning over a width, and a width on a number.
    for spec in [
        PlotSpec::new().data("t").x("day").layer(binned(Mark::Bar, Some(30), None)),
        PlotSpec::new().data("t").x("day").layer(binned(Mark::Bar, None, None)),
        PlotSpec::new().data("t").x("day").layer(binned(Mark::Bar, Some(30), Some(7.0))),
        PlotSpec::new().data("t").x("sales").layer(binned(Mark::Bar, None, Some(1.0))),
    ] {
        let d = check(&spec, &frames());
        assert!(!d.iter().any(|x| x.message.contains("does not say what unit")), "{:?}", msgs(&d));
    }
}

#[test]
fn a_date_column_needs_no_scale_said_out_loud() {
    // The time scale comes from the column's type, like `category` does.
    let spec = PlotSpec::new().data("t")
        .x("day").y("sales")
        .layer(Layer::new(Mark::Line));
    assert!(check(&spec, &dated()).is_empty());
}

#[test]
fn saying_time_on_a_date_column_costs_nothing() {
    // Same allowance `linear` gets on a number: true, and means nothing extra.
    let spec = PlotSpec::new().data("t")
        .x_scaled("day", ScaleType::Time).y("sales")
        .layer(Layer::new(Mark::Line));
    assert!(check(&spec, &dated()).is_empty());
}

#[test]
fn a_time_scale_on_a_plain_number_is_illegal_with_direction() {
    // The engine cannot know whether 20656 is a day, a second, or a year —
    // the calendar comes from the column's type, and the message says how
    // to give it one.
    let spec = PlotSpec::new().data("t")
        .x_scaled("sales", ScaleType::Time).y("sales")
        .layer(Layer::new(Mark::Point));
    let d = check(&spec, &dated());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("Store it as dates") && !d[0].message.contains("as.Date"),
        "{}", d[0].message);
}

#[test]
fn a_moment_in_time_has_no_logarithm() {
    let spec = PlotSpec::new().data("t")
        .x_scaled("day", ScaleType::Log).y("sales")
        .layer(Layer::new(Mark::Line));
    let d = check(&spec, &dated());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("date"), "{}", d[0].message);
}

#[test]
fn linear_cannot_quietly_undate_an_axis() {
    // `scale = "linear"` on a date would mean raw epoch seconds. Honoring
    // it silently draws an axis labeled 1.7B; refusing says what to do.
    let spec = PlotSpec::new().data("t")
        .x_scaled("day", ScaleType::Linear).y("sales")
        .layer(Layer::new(Mark::Point));
    let d = check(&spec, &dated());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("store the column as numbers")
        && !d[0].message.contains("as.numeric"), "{}", d[0].message);
}

#[test]
fn a_bar_cannot_measure_a_date() {
    // A bar's length is an amount; a moment is a point. Both orientations.
    let vertical = PlotSpec::new().data("t")
        .x("sales").y("day")
        .layer(Layer::new(Mark::Bar));
    let d = check(&vertical, &dated());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("amount"), "{}", d[0].message);
}

#[test]
fn bars_may_sit_on_dates_and_measure_numbers() {
    // The position axis is exactly where a date belongs.
    let spec = PlotSpec::new().data("t")
        .x("day").y("sales")
        .layer(Layer::new(Mark::Bar));
    assert!(check(&spec, &dated()).is_empty());
}

#[test]
fn saying_linear_out_loud_costs_nothing() {
    let spec = PlotSpec::new().data("t")
        .x_scaled("gdp", ScaleType::Linear).y("life")
        .layer(Layer::new(Mark::Point));
    assert!(check(&spec, &data()).is_empty());
}

#[test]
fn every_channel_that_measures_can_take_a_scale() {
    // Orthogonality: a scale answers "how far along?", so it belongs to
    // every channel that carries a magnitude, not only the axes.
    for ch in [Channel::Color, Channel::Size, Channel::Opacity] {
        let spec = base().layer(
            Layer::new(Mark::Point).encode_scaled(ch.clone(), "gdp", ScaleType::Log),
        );
        assert!(check(&spec, &data()).is_empty(), "{ch:?} should accept a log scale");
    }
}

#[test]
fn a_channel_that_only_distinguishes_cannot_take_a_scale() {
    // `shape` answers "which one?", and there is no distance between circle
    // and square for a logarithm to compress.
    let spec = base().layer(
        Layer::new(Mark::Point)
            .encode_scaled(Channel::Shape, "continent", ScaleType::Log),
    );
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("distinguishes categories"), "{}", d[0].message);
}

#[test]
fn a_log_color_still_needs_a_numeric_column() {
    let spec = base().layer(
        Layer::new(Mark::Point)
            .encode_scaled(Channel::Color, "continent", ScaleType::Log),
    );
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("is text"), "{}", d[0].message);
}

#[test]
fn size_on_categorical_is_illegal() {
    let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Size, "continent"));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("needs a continuous"));
    // The message must be directional, not merely a complaint, and name only
    // what a point takes: it offered `pattern`, which a point refuses.
    assert!(d[0].message.contains("Use `color` or `shape` to distinguish categories"),
        "{}", d[0].message);
}

/// **A transform that refuses a mark in its own check is refused once.** Each
/// of `path`, `rule`, `text`, `zone` and `surface` answered `bounds`,
/// `partition`, `flow` and `layout` in its own arm as well, so `surface *
/// bounds` printed two refusals of `bounds`, and a `text` was told that `bounds`
/// "replaces those rows with one summary per key".
#[test]
fn a_transform_that_refuses_a_mark_itself_is_refused_once() {
    for mark in [Mark::Path, Mark::Rule, Mark::Text, Mark::Zone, Mark::Surface] {
        for t in USER_TRANSFORMS.iter().filter(|t| refuses_marks_itself(t)) {
            if mark_takes_transform(&mark, t) != TransformLegality::None { continue }
            let d = check(&PlotSpec::new().data("t").x("gdp").y("life").z("value")
                .layer(Layer::new(mark.clone()).transform(t.clone())), &data());
            let n = transform_name(t);
            let naming: Vec<_> = d.iter()
                .filter(|x| x.is_fatal() && x.message.contains(&format!("`{n}`"))).collect();
            assert_eq!(naming.len(), 1, "{mark:?} {t:?}: {:?}", msgs(&d));
        }
    }
}

/// **A path is sent to `line` only with a statistic.** `path * flow`, `path *
/// layout` and `path * partition` were told "Use `line * <t>`", which a line
/// refuses too, beside each transform's own refusal; `path * bounds` got two
/// refusals as well. `path * count` was also read as a field and told to write
/// `path * count + x(<column>) + y(<column>)`, a sentence refused in turn.
#[test]
fn a_path_is_sent_to_line_only_with_a_statistic() {
    let net = CoordSpace::Network(crate::ir::NetworkView::default());
    for (spec, owner) in [
        (PlotSpec::new().data("t").layer(Layer::new(Mark::Path).flow(&["continent", "region"])),
         "`flow` lays"),
        (PlotSpec::new().data("t").coord(net)
            .layer(Layer::new(Mark::Path).layout("continent", "region")), "`layout` places"),
        (PlotSpec::new().data("t").y("gdp")
            .layer(Layer::new(Mark::Path).partition(&["continent", "region"])), "`partition` divides"),
        (PlotSpec::new().data("t").x("continent")
            .layer(Layer::new(Mark::Path).bounds("life", "value")), "`bounds` supplies"),
    ] {
        let d = check(&spec, &data());
        assert_eq!(d.len(), 1, "{owner}: {:?}", msgs(&d));
        assert!(d[0].message.contains(owner) && !d[0].message.contains("Use `line *"),
            "{:?}", d[0]);
    }
    for spec in [
        PlotSpec::new().data("t").x("continent").layer(Layer::new(Mark::Path).transform(Transform::Count)),
        PlotSpec::new().data("t").x("gdp").y("life").layer(Layer::new(Mark::Path).transform(Transform::Count)),
    ] {
        let d = check(&spec, &data());
        assert_eq!(d.len(), 1, "{:?}", msgs(&d));
        assert!(d[0].message.contains("Use `line * count`"), "{:?}", d[0]);
    }
}

/// **A flow in `polar()` is refused once, in words true of every layer.** The
/// refusal said "the bands bent round a rim" for a `zone`, which draws the
/// stages' slots, and a `text`, which draws their names, once per layer.
#[test]
fn a_flow_in_polar_is_refused_once_for_every_layer() {
    let polar = || CoordSpace::Polar(crate::ir::PolarView::default());
    let flow = |m: Mark| Layer::new(m).flow(&["continent", "region"]);
    let spec = PlotSpec::new().data("t").coord(polar())
        .layer(flow(Mark::Ribbon)).layer(flow(Mark::Zone));
    let d = check(&spec, &data());
    let said: Vec<_> = d.iter().filter(|x| x.message.contains("chord diagram")).collect();
    assert_eq!(said.len(), 1, "{:?}", msgs(&d));
    assert!(said[0].message.contains("a flow bent round a rim")
        && !said[0].message.contains("bands"), "{:?}", said[0]);
}

/// **A brush on a flow names a column every band holds one value of**
/// (2026-10-03). A stage passes, and so does a column of categories that
/// colors the bands, since it splits each band into parts that hold one value
/// each. A column of categories that is neither would cut across the bands,
/// and a number cannot split one at all, so both are refused with what to
/// brush instead. Bare `brush` passes on a flow with no bound count: the
/// reader names the column by clicking one of its slots.
#[test]
fn a_brush_on_a_flow_names_a_stage_or_a_column_that_splits_its_bands() {
    let flow = |m: Mark| Layer::new(m).flow(&["continent", "region"]);
    let flows = || PlotSpec::new().data("t").y("gdp")
        .layer(flow(Mark::Ribbon)).layer(flow(Mark::Zone));
    let unsupported = |spec: &PlotSpec| check(spec, &data()).into_iter()
        .filter(|x| x.message.contains("on a flow")).collect::<Vec<_>>();

    let stage = flows().brush(crate::ir::BrushDef::new("region").levels(vec!["North".into()]));
    let d = check(&stage, &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "a stage is brushed: {:?}", msgs(&d));
    assert!(d.iter().all(|x| !x.message.contains("drawn whole")),
        "every flow layer answers, so none is reported left whole: {:?}", msgs(&d));

    let mut colored = flows();
    colored.layers[0] = flow(Mark::Ribbon).encode(Channel::Color, "continent");
    let split = colored.brush(crate::ir::BrushDef::new("continent"));
    assert!(unsupported(&split).is_empty(), "a coloring column is brushed too");

    let measured = flows().brush(crate::ir::BrushDef::new("life").at(4.0, 5.0));
    let said = unsupported(&measured);
    assert_eq!(said.len(), 1, "{:?}", msgs(&check(&measured, &data())));
    assert_eq!(said[0].kind, DiagnosticKind::Unsupported);
    assert!(said[0].message.contains("`life` measures")
        && said[0].message.contains("brush(continent, at = ...)")
        && !said[0].message.contains("`color` or `pattern`"),
        "a number is told to brush a stage, never to color the bands: {:?}", said[0]);

    let other = Layer::new(Mark::Ribbon).flow(&["continent", "continent2"]);
    let mut d = data();
    let t = d.remove("t").unwrap();
    let t = t.with_str("continent2", vec!["x".into(), "y".into(), "x".into()]);
    d.insert("t".to_string(), t);
    let across = PlotSpec::new().data("t").y("gdp").layer(other)
        .brush(crate::ir::BrushDef::new("region").levels(vec!["North".into()]));
    let said: Vec<_> = check(&across, &d).into_iter()
        .filter(|x| x.message.contains("on a flow")).collect();
    assert_eq!(said.len(), 1, "a column of categories that is not a stage: {said:?}");
    assert!(said[0].message.contains("map `region` to the `ribbon` layer's `color` or `pattern`"),
        "{:?}", said[0]);

    let tally = PlotSpec::new().data("t").layer(flow(Mark::Ribbon)).brush(crate::ir::BrushDef::positions());
    let d = check(&tally, &data());
    assert!(!d.iter().any(|x| x.is_fatal()),
        "a bare brush on a flow needs no bound position: {:?}", msgs(&d));
}

/// **A shared flow draws in `polar()`, and a stage flow there points at it**
/// (2026-10-03). Bent, a stage flow keeps a ring or an angle per stage, a
/// circular alluvial, so it refuses as unbuilt and names `shared = TRUE` for two
/// columns that name one set of places; a shared flow's one axis bends into the
/// ring, the chord diagram. A shared flow joins two columns, and a third is
/// refused. A brush on the flow's own `name` passes, though no table has it.
#[test]
fn a_shared_flow_draws_in_polar_and_joins_two_columns() {
    let polar = || CoordSpace::Polar(crate::ir::PolarView::default());
    let shared = |m: Mark| Layer::new(m).flow_shared("continent", "region");
    let chord = PlotSpec::new().data("t").y("gdp").coord(polar())
        .layer(shared(Mark::Ribbon)).layer(shared(Mark::Zone));
    let d = check(&chord, &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "the chord diagram draws: {:?}", msgs(&d));

    let stage = PlotSpec::new().data("t").y("gdp").coord(polar())
        .layer(Layer::new(Mark::Ribbon).flow(&["continent", "region"]));
    let said = msgs(&check(&stage, &data()));
    assert!(said.iter().any(|m| m.contains("a ring for each stage")
        && m.contains("`flow(continent, region)` with `shared` set to true")), "{said:?}");

    let mut three = Layer::new(Mark::Ribbon).flow(&["continent", "region", "life"]);
    three.flow.as_mut().unwrap().shared = true;
    let said = msgs(&check(&PlotSpec::new().data("t").y("gdp").layer(three), &data()));
    assert!(said.iter().any(|m| m.contains("between two columns only")), "{said:?}");

    let named = PlotSpec::new().data("t").y("gdp").layer(shared(Mark::Ribbon))
        .brush(crate::ir::BrushDef::new("name").levels(vec!["Asia".into()]));
    let d = check(&named, &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "a brush on `name`: {:?}", msgs(&d));
}

/// **A shared or layered flow's names are not moved, so `repel` there is
/// refused** (2026-10-04, the author's ruling for the shared flow; the layered
/// flow drew the same bytes too). A stage flow's names sit in their slots,
/// where `repel` moves them, so it stays legal there.
#[test]
fn repel_on_a_shared_or_layered_flows_names_is_refused() {
    let names = |l: Layer| l.transform(Transform::Repel).encode(Channel::Label, "name");
    let arcs = PlotSpec::new().data("t").y("gdp")
        .layer(Layer::new(Mark::Zone).flow_shared("continent", "region"))
        .layer(names(Layer::new(Mark::Text).flow_shared("continent", "region")));
    let said = msgs(&check(&arcs, &data()));
    assert!(said.iter().any(|m| m.contains("a shared flow's names are not moved")
        && m.contains("`theme(width = )`") && m.contains("`polar()`")), "{said:?}");

    // A layered flow's names stand beside their slots, placed the same way.
    let layered = |m: Mark| {
        let mut l = Layer::new(m).flow(&["continent", "region"]);
        l.flow.as_mut().unwrap().layered = true;
        l
    };
    let sankey = PlotSpec::new().data("t").y("gdp")
        .layer(layered(Mark::Zone)).layer(names(layered(Mark::Text)));
    let said = msgs(&check(&sankey, &data()));
    assert!(said.iter().any(|m| m.contains("a layered flow's names are not moved")
        && m.contains("`theme(height = )`")), "{said:?}");

    let stages = PlotSpec::new().data("t").y("gdp")
        .layer(Layer::new(Mark::Zone).flow(&["continent", "region"]))
        .layer(names(Layer::new(Mark::Text).flow(&["continent", "region"])));
    let said = msgs(&check(&stages, &data()));
    assert!(!said.iter().any(|m| m.contains("names are not moved")), "{said:?}");
}

/// **A layered flow is the Sankey diagram, between two columns, drawn flat**
/// (2026-10-03). It draws with a brush on `name`; a third column, `polar()`,
/// and `shared = TRUE` beside it are refused, and links that form a
/// cycle are refused with the cycle named and the shared flow offered.
#[test]
fn a_layered_flow_is_flat_between_two_columns_and_has_no_cycle() {
    let layered = |m: Mark| Layer::new(m).flow_layered("continent", "region");
    let sankey = PlotSpec::new().data("t").y("gdp").layer(layered(Mark::Ribbon)).layer(layered(Mark::Zone))
        .brush(crate::ir::BrushDef::new("name").levels(vec!["Asia".into()]));
    let d = check(&sankey, &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "the Sankey diagram draws: {:?}", msgs(&d));

    let mut both = layered(Mark::Ribbon);
    both.flow.as_mut().unwrap().shared = true;
    let said = msgs(&check(&PlotSpec::new().data("t").y("gdp").layer(both), &data()));
    assert!(said.iter().any(|m| m.contains("Keep one")), "{said:?}");

    let mut three = Layer::new(Mark::Ribbon).flow(&["continent", "region", "life"]);
    three.flow.as_mut().unwrap().layered = true;
    let said = msgs(&check(&PlotSpec::new().data("t").y("gdp").layer(three), &data()));
    assert!(said.iter().any(|m| m.contains("a layered flow between two columns only")), "{said:?}");

    let bent = PlotSpec::new().data("t").y("gdp").layer(layered(Mark::Ribbon))
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()));
    let said = msgs(&check(&bent, &data()));
    assert!(said.iter().any(|m| m.contains("a layered flow in `polar()`")), "{said:?}");

    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let ring = HashMap::from([("r".to_string(), DataFrame::new()
        .with_str("a", s(&["A", "B", "C"])).with_str("b", s(&["B", "C", "A"]))
        .with_float("n", vec![1.0, 2.0, 3.0]))]);
    let looped = PlotSpec::new().data("r").y("n")
        .layer(Layer::new(Mark::Ribbon).flow_layered("a", "b"));
    let d = check(&looped, &ring);
    assert!(d.iter().any(|x| x.is_fatal() && x.message.contains("A → B → C → A")
        && x.message.contains("Set `shared` to true in place of `layered`: `flow(a, b)`")),
        "{:?}", msgs(&d));
}

/// **A transform written twice is told so once.** The pair sentences were written
/// for two different transforms: `mean * mean` was offered "`bar * mean` or `bar *
/// mean`", and `proportion * proportion` was told about `stack(share = TRUE)`.
#[test]
fn a_transform_written_twice_is_told_so_once() {
    for (mark, t) in [
        (Mark::Bar, Transform::Mean), (Mark::Bar, Transform::Sum), (Mark::Bar, Transform::Count),
        (Mark::Bar, Transform::Bin), (Mark::Area, Transform::Density),
        (Mark::Bar, Transform::Proportion), (Mark::Interval, Transform::Range),
    ] {
        let layer = Layer::new(mark.clone()).transform(t.clone()).transform(t.clone());
        let d = check(&PlotSpec::new().data("t").x("continent").y("life").layer(layer), &data());
        let n = transform_name(&t);
        let said: Vec<_> = d.iter().filter(|x| x.message.contains(&format!("names `{n}` twice")))
            .collect();
        assert_eq!(said.len(), 1, "{t:?}: {:?}", msgs(&d));
        assert!(said[0].message.ends_with(&format!("Keep one: `{} * {n}`.", mark_name(&mark))),
            "{:?}", said[0]);
        assert!(d.iter().all(|x| !x.message.contains("stack(share = TRUE)")
            && !x.message.contains(&format!("or `{} * {n}`", mark_name(&mark)))),
            "{t:?}: {:?}", msgs(&d));
    }
}

/// **No message the engine writes spells R's `TRUE` or `FALSE`.** The engine
/// speaks to four languages and those two words are R's alone, so a Python,
/// Julia or JavaScript reader told to write `shared = TRUE` is told to write R.
/// 0.4.0 put such settings into words ("with its `share` set to true"), and
/// 0.5.0's flow and axis messages brought the spelling back in nine places,
/// found only by asking every new refusal in all four bindings. A message is
/// built in many places, so this reads the source rather than the messages: every
/// line outside comments and outside each file's tests, in every crate. A module's
/// own `tests.rs` is tests from its first line, so it is not read at all.
#[test]
fn no_engine_message_spells_r_true_or_false() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs")
                && path.file_name().is_some_and(|n| n != "tests.rs")
            {
                out.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in ["src", "../gog-cli/src", "../gog-wasm/src"] {
        walk(&root.join(dir), &mut files);
    }
    assert!(files.len() > 20, "the walk found {} files; it is looking in the wrong place",
        files.len());
    let word = |line: &str, w: &str| line.match_indices(w).any(|(i, _)| {
        let before = line[..i].chars().last();
        let after = line[i + w.len()..].chars().next();
        let joins = |c: char| c.is_alphanumeric() || c == '_';
        !before.is_some_and(joins) && !after.is_some_and(joins)
    });
    let mut found = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for (n, line) in text.lines().enumerate() {
            // A file's tests begin at its unindented `#[cfg(test)]`; an indented
            // one marks a single helper and the scan reads on past it.
            if line.starts_with("#[cfg(test)]") {
                break;
            }
            let code = line.trim_start();
            if !code.starts_with("//") && (word(code, "TRUE") || word(code, "FALSE")) {
                found.push(format!("{}:{}: {code}", file.display(), n + 1));
            }
        }
    }
    assert!(found.is_empty(), "an engine message spells R's TRUE or FALSE; say the \
        setting in words, as in \"with its `share` set to true\":\n{}", found.join("\n"));
}

/// **A channel a mark does not have is refused toward the marks that do.** The
/// mapping refusal and its `style()` sibling said "use a mark that has one" and
/// named none. The list is read off `rule_for`, and no refused pair ends on an
/// empty one.
#[test]
fn a_missing_feature_is_refused_toward_the_marks_that_have_it() {
    for m in ALL_MARKS.iter().filter(|m| is_drawable(m)) {
        for c in &ALL_CHANNELS {
            let r = rule_for(m, c);
            if r.obligation == Obligation::Cannot {
                assert!(!marks_that_take(c, true).is_empty(), "{m:?} {c:?}: nothing maps it");
            }
            if !r.settable && matches!(c, Channel::Color | Channel::Size | Channel::Shape
                | Channel::Pattern | Channel::Opacity) {
                assert!(!marks_that_take(c, false).is_empty(), "{m:?} {c:?}: nothing sets it");
            }
        }
    }
    let d = check(&base().layer(Layer::new(Mark::Text).encode(Channel::Label, "continent")
        .encode(Channel::Pattern, "continent")), &data());
    assert!(d.iter().any(|x| x.message.contains("cannot be bound to `text`")
        && x.message.contains("use a mark that maps it: `line`, `area`, `bar`, `step`, \
        `interval`, `box`, `ribbon`, `path`, `rule` or `zone`.")), "{:?}", msgs(&d));
    let mut sized = Layer::new(Mark::Bar);
    sized.style.size = Some(3.0);
    let d = check(&PlotSpec::new().data("t").x("continent").y("life").layer(sized), &data());
    assert!(d.iter().any(|x| x.message.contains("has no size to set. Remove it, or use a mark \
        that has one: `point`")), "{:?}", msgs(&d));
}

/// **A graph-placed layer's bound position is refused by the graph alone.** An
/// `edge` bound to `x` was also told it "has no x feature" and sent to every
/// flat mark that maps `x`, beside the network's own refusal of the position.
#[test]
fn a_graph_placed_position_is_refused_by_the_graph_alone() {
    let net = || CoordSpace::Network(crate::ir::NetworkView::default());
    for (coord, layer) in [
        (net(), Layer::new(Mark::Edge).layout("continent", "region")),
        (net(), Layer::new(Mark::Edge)),
        (CoordSpace::Flat, Layer::new(Mark::Edge)),
        (CoordSpace::Flat, Layer::new(Mark::Edge).layout("continent", "region")),
    ] {
        let spec = PlotSpec::new().data("t").x("gdp").y("life").coord(coord.clone())
            .layer(layer.clone());
        let d = check(&spec, &data());
        assert!(d.iter().any(Diagnostic::is_fatal), "{coord:?} {:?} drew", layer.transforms);
        assert!(d.iter().all(|x| !x.message.contains("cannot be bound to `edge`")),
            "{coord:?} {:?}: {:?}", layer.transforms, msgs(&d));
    }
}

/// **Every channel a type refusal offers takes that column on that mark.** The
/// hint was keyed on the channel alone: a numeric `pattern` on a `bar` was
/// offered `size` and `color`, both refused there, and `bar + color(<number>)`
/// was offered nothing though `opacity` draws it. Checked over every drawable
/// mark and every aesthetic, by following each offer.
#[test]
fn every_channel_a_type_refusal_offers_draws() {
    let aesthetics = [Channel::Color, Channel::Size, Channel::Shape, Channel::Pattern,
                      Channel::Opacity];
    let column = |t: VarType| if t == VarType::Continuous { "gdp" } else { "continent" };
    let spec = |m: &Mark, c: &Channel, field: &str| {
        let mut base = PlotSpec::new().data("t").x("continent").y("life");
        if *m == Mark::Surface {
            base = PlotSpec::new().data("t").x("gdp").y("life").z("value");
        }
        base.layer(Layer::new(m.clone()).encode(c.clone(), field))
    };
    let mut offered = 0;
    for m in ALL_MARKS.iter().filter(|m| is_drawable(m) && **m != Mark::Edge) {
        for c in &aesthetics {
            for t in [VarType::Continuous, VarType::Discrete] {
                let r = rule_for(m, c);
                if r.obligation == Obligation::Cannot || r.accepts.accepts(t) {
                    continue;
                }
                for other in channels_that_carry(m, c, t) {
                    let ch = aesthetics.iter().find(|a| channel_name(a) == other).unwrap();
                    let d = check(&spec(m, ch, column(t)), &data());
                    assert!(d.iter().all(|x| !x.message.contains(&format!("`{other}(")) ||
                        !x.is_fatal()),
                        "{m:?}: `{}` refused {t:?} and offered `{other}`, refused too: {:?}",
                        channel_name(c), msgs(&d));
                    offered += 1;
                }
            }
        }
    }
    assert!(offered > 10, "the offers were exercised: {offered}");
    // The two recorded cases, whole.
    let bar = check(&PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Bar).encode(Channel::Color, "gdp")), &data());
    assert!(bar[0].message.contains("Use `opacity` to show a numeric column"), "{:?}", bar[0]);
    let text = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Text).encode(Channel::Label, "continent")
            .encode(Channel::Size, "continent")), &data());
    assert!(text.iter().any(|x| x.message.contains("Use `color` to distinguish categories")),
        "{:?}", msgs(&text));
}

#[test]
fn size_on_bar_is_illegal() {
    let spec = base().layer(Layer::new(Mark::Bar).encode(Channel::Size, "gdp"));
    assert_eq!(kinds(&check(&spec, &data())), vec![DiagnosticKind::Illegal]);
}

#[test]
fn opacity_spans_the_marks_with_parts_small_enough_to_hold_one_value() {
    // **The class, named mark by mark**, because the rule was never "regions refuse
    // it, glyphs take it": it is whether the mark has *parts* a per-row number can
    // land on. A `point` is one part per row and a `bar` one column per row, an
    // `edge` one stroke per row, and a `surface` is a mesh of faces — small enough
    // to each hold one value, which is the same reading that lets a mesh take a
    // measured color where an `area` cannot.
    //
    // This test read `opacity_renders_on_point_and_bar` and named two marks, which
    // is how `surface` sat outside it while sitting inside the class: the sheet
    // could be moved from one side to the other and the assertion stayed green
    // either way. That is `border_spans_*`'s lesson, one channel over, and the
    // drift guard below is the half a hand-written list cannot be.
    // What a caller meets for one mark: the diagnostics that mention `opacity`,
    // and nothing else. Keyed on the message rather than on an empty list so that
    // every mark can be walked under one `base()` — a `surface` wants the cube and
    // an `edge` a layout, and their own minimum-syllable complaints are not what
    // is under test here. The column is numeric, so the categorical refusal cannot
    // fire and confuse the reading.
    let fade = |m: &Mark| -> Vec<DiagnosticKind> {
        let layer = Layer::new(m.clone()).encode(Channel::Opacity, "gdp");
        let layer = match m {
            Mark::Interval | Mark::Ribbon => layer.transform(Transform::Range),
            Mark::Text => layer.encode(Channel::Label, "continent"),
            _ => layer,
        };
        check(&base().layer(layer), &data()).into_iter()
            .filter(|d| d.message.contains("`opacity`"))
            .map(|d| d.kind).collect()
    };

    for m in [Mark::Point, Mark::Bar, Mark::Edge, Mark::Surface] {
        assert!(fade(&m).is_empty(),
            "{m:?} has parts to fade, so a mapped opacity draws with no complaint");
    }
    // One region, one stroke: a row is a *vertex of the boundary* rather than a
    // part of its own, so there is nothing for a per-row value to vary across.
    // Every one of these still takes `style(opacity = )`, which is the whole of
    // what `SET_ONLY` means.
    for m in [Mark::Line, Mark::Step, Mark::Area, Mark::Interval,
              Mark::Box, Mark::Ribbon, Mark::Path, Mark::Rule, Mark::Zone] {
        assert!(fade(&m).contains(&DiagnosticKind::Illegal),
            "{m:?} refuses a mapped opacity with direction, met {:?}", fade(&m));
        assert!(rule_for(&m, &Channel::Opacity).settable,
            "{m:?} still takes a set opacity");
    }

    // And the drift guard the two lists cannot be: walk **every** mark and require
    // the diagnostic a caller actually meets to agree with what `rule_for` says —
    // which is what the book's generated grid prints and what `--rules` dumps. A
    // mark named in neither list above is invisible to both assertions.
    for m in &ALL_MARKS {
        let r = rule_for(m, &Channel::Opacity);
        let met = fade(m);
        let want = match (r.obligation, r.renders) {
            (Obligation::Cannot, _) => Some(DiagnosticKind::Illegal),
            (_, None) => Some(DiagnosticKind::Unsupported),
            (_, Some(_)) => None,
        };
        match want {
            None => assert!(met.is_empty(),
                "{m:?} renders a mapped opacity, so nothing should be said: {met:?}"),
            Some(k) => assert!(met.contains(&k),
                "{m:?} is {:?}/{:?} in the table, so a caller should meet {k:?}, met {met:?}",
                r.obligation, r.renders),
        }
    }
}

#[test]
fn opacity_on_line_is_illegal_for_the_same_reason_as_size() {
    // A polyline is drawn with a single stroke, so a per-row continuous
    // channel has nothing to vary along. `size` and `opacity` must give the
    // same answer here — differing would be a per-channel special case.
    let size = check(
        &base().layer(Layer::new(Mark::Line).encode(Channel::Size, "gdp")),
        &data(),
    );
    let opacity = check(
        &base().layer(Layer::new(Mark::Line).encode(Channel::Opacity, "gdp")),
        &data(),
    );
    assert_eq!(kinds(&size), vec![DiagnosticKind::Illegal]);
    assert_eq!(kinds(&opacity), vec![DiagnosticKind::Illegal]);
}

#[test]
fn opacity_on_a_categorical_column_is_illegal() {
    let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Opacity, "continent"));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("needs a continuous"));
}

#[test]
fn categorical_x_works_for_both_point_and_bar() {
    // A categorical x is a strip plot for `point` and a category axis for
    // `bar`. Both must be accepted — gating it to one mark was a per-mark
    // special case, i.e. a Law of No Exceptions violation.
    for mark in [Mark::Point, Mark::Bar] {
        let spec = PlotSpec::new()
            .data("t")
            .x("continent")
            .y("life")
            .layer(Layer::new(mark.clone()));
        assert!(
            check(&spec, &data()).is_empty(),
            "categorical x should be legal for {:?}",
            mark
        );
    }
}

#[test]
fn categorical_y_works_for_both_point_and_bar() {
    // The mirror of the x case: a categorical y is a horizontal strip plot
    // for `point` and a horizontal category axis for `bar`. `point` refusing
    // it while `bar` accepted it was the same per-mark asymmetry categorical
    // x had — Law 2 (No Exceptions). `line` still refuses (its own test).
    for mark in [Mark::Point, Mark::Bar] {
        let spec = PlotSpec::new()
            .data("t")
            .x("life")
            .y("continent")
            .layer(Layer::new(mark.clone()));
        assert!(
            check(&spec, &data()).is_empty(),
            "categorical y should be legal for {:?}",
            mark
        );
    }
}

#[test]
fn synthesized_y_names_an_output_column_not_an_input() {
    // `bar * bin + x(height) + y(count)` — `count` is the name the transform
    // writes its output to, so it must not be checked against input columns.
    let spec = PlotSpec::new()
        .data("t")
        .x("gdp")
        .y("count")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    assert!(check(&spec, &data()).is_empty());
}

/// A **composed** `proportion` synthesizes nothing, so its `y` is an input name
/// and a misspelling of it must still be caught.
///
/// Found by a reader looking at a plot, which is twice in three sessions for
/// this corner. `bar * sum * proportion + x(continent) + y(pop)` — `pop` having
/// been renamed `population` in the book's own data — drew an **empty panel on
/// fabricated 0..1 axes**, exactly the failure `synthesizes_measure`'s doc
/// comment warns about, because `proportion` was still on its list of
/// inventors. The pair is what makes it obvious: dropping `proportion` from the
/// sentence refused correctly, and a word that measures nothing cannot turn a
/// misspelling into a legal name.
#[test]
fn a_composed_proportion_still_checks_the_column_it_rescales() {
    let missing = |ts: Vec<Transform>| {
        let mut layer = Layer::new(Mark::Bar);
        for t in ts { layer = layer.transform(t); }
        check(&PlotSpec::new().data("t").x("continent").y("nosuchcolumn").layer(layer), &data())
    };
    for ts in [
        vec![Transform::Sum],
        vec![Transform::Sum, Transform::Proportion],
        vec![Transform::Mean, Transform::Proportion],
    ] {
        let d = missing(ts.clone());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                    && x.message.contains("not in the data")),
            "`bar * {}` + y(nosuchcolumn) must refuse: {:?}",
            ts.iter().map(transform_name).collect::<Vec<_>>().join(" * "), msgs(&d));
    }
    // …and a *bare* `proportion` still names its own output, which is the
    // reading that put it on the inventors' list in the first place. Since
    // 2026-10-09 that name is the measure's, `proportion`, and no other.
    let bare = |y: &str| check(&PlotSpec::new().data("t").x("continent").y(y)
        .layer(Layer::new(Mark::Bar).transform(Transform::Proportion)), &data());
    assert!(bare("proportion").is_empty(), "`bar * proportion + y(proportion)` draws");
    assert!(bare("nosuchcolumn").iter().any(|x| x.message.contains("Write `y(proportion)`")),
        "{:?}", msgs(&bare("nosuchcolumn")));
}

#[test]
fn missing_y_is_illegal_unless_a_transform_synthesizes_it() {
    let spec = PlotSpec::new()
        .data("t")
        .x("gdp")
        .layer(Layer::new(Mark::Point));
    assert_eq!(kinds(&check(&spec, &data())), vec![DiagnosticKind::Illegal]);

    let spec = PlotSpec::new()
        .data("t")
        .x("gdp")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count));
    assert!(about_the_sentence(check(&spec, &data())).is_empty());
}

#[test]
fn a_color_passed_to_the_color_channel_points_at_style() {
    // `color("red")` is the natural first guess for setting a constant.
    // The quotes survive deparsing, so gog can tell a value from a column
    // name and say so, instead of sending the reader hunting for a typo.
    let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, "\"red\""));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains(r#"style(color = "red")"#), "got: {}", d[0].message);
    assert!(!d[0].message.contains("spelling"), "got: {}", d[0].message);
}

#[test]
fn a_quoted_column_name_is_told_to_drop_the_quotes() {
    // Law 4: columns are bare names. Quoting one is a habit from other
    // R packages, not a typo, so the message names the real fix.
    let spec = PlotSpec::new()
        .data("t")
        .x("\"gdp\"")
        .y("life")
        .layer(Layer::new(Mark::Point));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("x(gdp)"), "got: {}", d[0].message);
}

#[test]
fn unknown_column_is_illegal() {
    let spec = PlotSpec::new()
        .data("t")
        .x("gdp")
        .y("lifeExp") // real column is `life`
        .layer(Layer::new(Mark::Point));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("not in the data"));
}

#[test]
fn color_on_point_takes_either_column_type() {
    // Was the last `accepts`/`renders` gap: `color` on `point` was declared
    // to take either type and drew only categories. A numeric column now
    // takes the sequential ramp.
    for field in ["gdp", "continent"] {
        let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, field));
        assert!(
            check(&spec, &data()).is_empty(),
            "color({field}) should render: {:?}",
            check(&spec, &data())
        );
    }
}

#[test]
fn a_stroke_takes_a_measured_color_but_still_refuses_a_measured_width() {
    // The two halves of one distinction, asserted together so neither can
    // drift into the other. A stroke's color may be read off the *piece* of
    // it you are looking at, so a measure varies along it; a stroke's width
    // and opacity belong to the whole element, so they cannot. That is why
    // `color` widened to `Either` on the stroke marks while `size` and
    // `opacity` stayed set-only — not a channel-by-channel judgment call.
    for m in [Mark::Line, Mark::Step, Mark::Path] {
        let ramped = check(&base().layer(Layer::new(m.clone()).encode(Channel::Color, "gdp")), &data());
        assert!(
            ramped.iter().all(|d| !d.is_fatal()),
            "{m:?} should take a measured color: {:?}", msgs(&ramped)
        );
        for ch in [Channel::Size, Channel::Opacity] {
            let d = check(&base().layer(Layer::new(m.clone()).encode(ch.clone(), "gdp")), &data());
            assert!(
                kinds(&d).contains(&DiagnosticKind::Illegal),
                "{m:?} should still refuse a mapped {ch:?}: {:?}", msgs(&d)
            );
        }
    }
}

#[test]
fn a_palette_with_nothing_to_color_is_refused_rather_than_dropped() {
    // It rendered byte for byte identically to the same sentence without it,
    // which is the silent drop §12 forbids and which the manual's Design Laws
    // chapter promises never happens in as many words. Inert for every other
    // channel too, measured rather than assumed, so there is no reading under
    // which the sentence means something.
    let mut spec = base().layer(Layer::new(Mark::Point));
    spec.palette = PaletteDef::Named("okabe".into());
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(d[0].message.contains("color(<column>)"), "name the fix: {}", d[0].message);
    assert!(d[0].message.contains("style(color"), "and the setting: {}", d[0].message);

    // A bound `color` is all it takes, from either scope — a plot-scoped one
    // reaches the layer, so the palette has something to hand out.
    let mut spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));
    spec.palette = PaletteDef::Named("okabe".into());
    assert!(check(&spec, &data()).is_empty(), "{:?}", msgs(&check(&spec, &data())));

    // And the default palette says nothing, since every plot carries one.
    let plain = base().layer(Layer::new(Mark::Point));
    assert!(check(&plain, &data()).is_empty(), "{:?}", msgs(&check(&plain, &data())));
}

#[test]
fn a_palette_must_match_the_kind_of_column_it_colors() {
    // A palette is chosen for a column. Asking for the wrong kind is a
    // mistake worth naming, not something to silently resolve either way.
    let mut spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, "gdp"));
    spec.palette = PaletteDef::Named("okabe".into());
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("one color per category"), "got: {}", d[0].message);
    assert!(d[0].message.contains("viridis"), "should name the ramps: {}", d[0].message);

    let mut spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));
    spec.palette = PaletteDef::Named("viridis".into());
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("sequential ramp"), "got: {}", d[0].message);
}

#[test]
fn the_gray_refusal_names_the_channel_the_mark_takes() {
    // `gray` on categories is a print figure, and the refusal names the channel
    // that separates them without color. It named `pattern` on a point plot,
    // which a point refuses; the point's own answer is `shape`.
    let gray = |layers: Vec<Layer>| {
        let mut spec = base();
        for l in layers {
            spec = spec.layer(l.encode(Channel::Color, "continent"));
        }
        spec.palette = PaletteDef::Named("gray".into());
        let d = check(&spec, &data());
        d.iter().find(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("`palette(\"gray\")` is a sequential ramp"))
            .unwrap_or_else(|| panic!("the gray refusal: {:?}", msgs(&d)))
            .message.clone()
    };
    let point = gray(vec![Layer::new(Mark::Point)]);
    assert!(point.contains("`shape(<column>)`") && !point.contains("pattern("), "{point}");
    let line = gray(vec![Layer::new(Mark::Line)]);
    assert!(line.contains("`pattern(<column>)`") && !line.contains("shape("), "{line}");
    // Two marks wanting two channels are each named.
    let both = gray(vec![Layer::new(Mark::Line), Layer::new(Mark::Point)]);
    assert!(both.contains("`pattern(<column>)` on the `line` layer")
        && both.contains("`shape(<column>)` on the `point` layer"), "{both}");
    // A mark with neither gets no print sentence rather than a wrong one.
    let text = gray(vec![Layer::new(Mark::Text).encode(Channel::Label, "continent")]);
    assert!(!text.contains("black and white"), "{text}");
}

#[test]
fn the_ramps_are_accepted_for_the_column_they_suit() {
    for (field, pal) in [("gdp", "blue"), ("gdp", "viridis"), ("continent", "gog")] {
        let mut spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, field));
        spec.palette = PaletteDef::Named(pal.into());
        assert!(check(&spec, &data()).is_empty(), "{pal} should suit {field}");
    }
}

#[test]
fn an_unset_palette_suits_either_column_type() {
    // `Auto` exists so "said nothing" and "asked for the categorical
    // palette" can differ: the first picks a ramp, the second is an error.
    for field in ["gdp", "continent"] {
        let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, field));
        assert!(matches!(spec.palette, PaletteDef::Auto));
        assert!(check(&spec, &data()).is_empty(), "auto should suit {field}");
    }
}

// -- a color bound to a level by name (spec §10) ---------------------

fn named(pairs: &[(&str, &str)]) -> PaletteDef {
    PaletteDef::Levels(LevelColors(
        pairs.iter().map(|(l, c)| (l.to_string(), c.to_string())).collect(),
    ))
}

fn palette_on(field: &str, palette: PaletteDef) -> Vec<Diagnostic> {
    let mut spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, field));
    spec.palette = palette;
    check(&spec, &data())
}

#[test]
fn a_named_palette_that_names_every_level_is_accepted_in_any_order() {
    // The column meets them Asia, Europe, Africa; the names arrive in another
    // order, which is the whole reason to write them.
    let d = palette_on("continent", named(&[
        ("Africa", "seagreen"), ("Europe", "steelblue"), ("Asia", "tomato"),
    ]));
    assert!(d.is_empty(), "{:?}", msgs(&d));
}

#[test]
fn a_name_that_is_no_level_is_refused_with_the_levels_and_the_near_miss() {
    // The typo this rule exists for: it used to color Asia with some other
    // color and say nothing.
    let d = palette_on("continent", named(&[
        ("Asai", "tomato"), ("Europe", "steelblue"), ("Africa", "seagreen"),
    ]));
    // One refusal, not two: the misspelling also leaves Asia without a name,
    // and saying so as well would send the reader to add a second entry.
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    let m = &d[0].message;
    assert!(m.contains("names \"Asai\"") && m.contains("`continent` has no level called that"), "{m}");
    assert!(m.contains("Did you mean \"Asia\"?"), "the near miss is named: {m}");
    assert!(m.contains("\"Asia\", \"Europe\", and \"Africa\""), "the levels, in the column's order: {m}");

    // A name nowhere near a level gets the list and no guess.
    let d = palette_on("continent", named(&[
        ("Antarctica", "white"), ("Asia", "tomato"), ("Europe", "steelblue"), ("Africa", "seagreen"),
    ]));
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(!d[0].message.contains("Did you mean"), "{}", d[0].message);

    // And a near miss is only suggested when that level is still unnamed:
    // pointing at a level that already has its color would have the reader
    // write it twice.
    let d = palette_on("continent", named(&[
        ("Asia", "tomato"), ("Asai", "red"), ("Europe", "steelblue"), ("Africa", "seagreen"),
    ]));
    assert!(!d[0].message.contains("Did you mean"), "{}", d[0].message);
}

#[test]
fn a_drawn_level_left_unnamed_is_refused_by_name() {
    let d = palette_on("continent", named(&[("Asia", "tomato"), ("Europe", "steelblue")]));
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    let m = &d[0].message;
    assert!(m.contains("leaves out \"Africa\""), "{m}");
    assert!(m.contains("without names"), "the other way out is named too: {m}");

    let d = palette_on("continent", named(&[("Asia", "tomato")]));
    assert!(d[0].message.contains("leaves out \"Europe\" and \"Africa\""), "{}", d[0].message);
}

#[test]
fn a_declared_level_with_no_rows_may_be_named_and_need_not_be() {
    // A factor's levels are the column's levels whether or not a row carries
    // one, so naming Oceania is not a typo; and an undrawn level needs no
    // color, since nothing is drawn in it.
    let df = DataFrame::new()
        .with_float("gdp", vec![1.0, 2.0])
        .with_float("life", vec![3.0, 4.0])
        .with_levels(
            "continent",
            vec!["Asia".into(), "Europe".into()],
            vec!["Europe".into(), "Asia".into(), "Oceania".into()],
        );
    let tables: HashMap<String, DataFrame> = [("t".to_string(), df)].into_iter().collect();
    for pairs in [
        &[("Asia", "tomato"), ("Europe", "steelblue"), ("Oceania", "orchid")][..],
        &[("Asia", "tomato"), ("Europe", "steelblue")][..],
    ] {
        let mut spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));
        spec.palette = named(pairs);
        let d = check(&spec, &tables);
        assert!(d.is_empty(), "{pairs:?}: {:?}", msgs(&d));
    }
}

#[test]
fn a_named_palette_on_a_number_is_refused_toward_a_ramp() {
    let d = palette_on("gdp", named(&[("Asia", "tomato")]));
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    let m = &d[0].message;
    assert!(m.contains("`gdp`") && m.contains("numbers have no levels"), "{m}");
    assert!(m.contains("such as \"white\" and \"navy\"") && m.contains("viridis")
        && !m.contains("c("), "{m}");
}

#[test]
fn a_named_palette_refuses_a_name_given_twice_and_a_color_that_is_not_one() {
    let d = palette_on("continent", named(&[
        ("Asia", "tomato"), ("Europe", "steelblue"), ("Africa", "seagreen"), ("Asia", "red"),
    ]));
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(d[0].message.contains("names \"Asia\" twice, as \"tomato\" or \"red\""),
        "{}", d[0].message);

    let d = palette_on("continent", named(&[
        ("Asia", "tomatoe"), ("Europe", "steelblue"), ("Africa", "seagreen"),
    ]));
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    let m = &d[0].message;
    assert!(m.contains("gives \"Asia\" the color \"tomatoe\"") && m.contains("\"tomato\""), "{m}");

    let d = palette_on("continent", PaletteDef::Levels(LevelColors::default()));
    assert!(d[0].message.contains("was given no colors"), "{:?}", msgs(&d));
}

#[test]
fn a_named_palette_with_nothing_to_color_says_so_like_any_palette() {
    let mut spec = base().layer(Layer::new(Mark::Point));
    spec.palette = named(&[("Asia", "tomato")]);
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(d[0].message.contains("nothing here maps `color`"), "{}", d[0].message);
}

#[test]
fn a_named_palette_reads_a_network_s_node_names() {
    // `color(name)` under `layout` colors each node by a name the layout
    // invents from the two endpoint columns, so the table has no `name`
    // column for the levels to be read from. The nodes of this one are Asia,
    // North, Europe, South and Africa.
    let net = CoordSpace::Network(crate::ir::NetworkView::default());
    let check_with = |pairs: &[(&str, &str)]| {
        let mut l = Layer::new(Mark::Point).layout("continent", "region");
        l.encodings.insert(Channel::Color, ChannelDef::field("name"));
        let mut spec = PlotSpec::new().data("t").coord(net.clone()).layer(l);
        spec.palette = named(pairs);
        check(&spec, &data())
    };
    let all = [("Asia", "tomato"), ("North", "gray"), ("Europe", "steelblue"),
               ("South", "gray"), ("Africa", "seagreen")];
    assert!(check_with(&all).is_empty(), "{:?}", msgs(&check_with(&all)));
    let d = check_with(&all[..4]);
    assert!(d.iter().any(|x| x.message.contains("leaves out \"Africa\"")), "{:?}", msgs(&d));
    let d = check_with(&[("Nroth", "gray")]);
    assert!(d.iter().any(|x| x.message.contains("Did you mean \"North\"?")), "{:?}", msgs(&d));
}

// -- a legend turned off (spec §10) ------------------------------------

#[test]
fn a_key_channel_takes_legend_and_says_nothing_about_it() {
    // The author's statement, not an assumption the engine made, so no
    // diagnostic of any kind — Assumption included.
    for (channel, field) in [
        (Channel::Color, "continent"), (Channel::Size, "gdp"), (Channel::Shape, "continent"),
        (Channel::Opacity, "gdp"),
    ] {
        let layer = Layer::new(Mark::Point)
            .encode_def(channel.clone(), ChannelDef::field(field).with_legend(false));
        let d = check(&base().layer(layer), &data());
        assert!(d.is_empty(), "{channel:?}: {:?}", msgs(&d));
    }
    let layer = Layer::new(Mark::Bar).transform(Transform::Count)
        .encode_def(Channel::Pattern, ChannelDef::field("continent").with_legend(false));
    let d = check(&PlotSpec::new().data("t").x("continent").layer(layer), &data());
    assert!(d.is_empty(), "pattern: {:?}", msgs(&d));
}

#[test]
fn legend_on_a_channel_that_draws_no_key_is_refused_with_direction() {
    let tail = "`legend` belongs on the five channels that draw one: `color`, `size`, \
                `shape`, `pattern` and `opacity`.";
    let refused = |spec: PlotSpec, why: &str| {
        let d = check(&spec, &data());
        let hit = d.iter().find(|x| x.kind == DiagnosticKind::Illegal && x.message.contains(tail))
            .unwrap_or_else(|| panic!("no legend refusal: {:?}", msgs(&d)));
        assert!(hit.message.contains(why), "{}", hit.message);
    };
    let off = |f: &str| ChannelDef::field(f).with_legend(false);

    let mut spec = base().layer(Layer::new(Mark::Point));
    spec.x = Some(off("gdp"));
    refused(spec, "`x(gdp)` is given `legend` — `x` is read off its axis");
    let mut spec = base().layer(Layer::new(Mark::Point));
    spec.y = Some(ChannelDef::field("life").with_legend(true));
    refused(spec, "`y(life)` is given `legend`");
    refused(base().layer(Layer::new(Mark::Line).encode_def(Channel::Group, off("continent"))),
        "`group` splits the rows without encoding anything");
    refused(base().layer(Layer::new(Mark::Text).encode_def(Channel::Label, off("continent"))),
        "`label` is the text a `text` mark writes");
    refused(base().layer(Layer::new(Mark::Point).encode_def(Channel::Play, off("continent"))),
        "`play` names each frame");
}

#[test]
fn a_plot_scoped_refusal_is_said_once_however_many_layers_it_reaches() {
    let mut spec = base().layer(Layer::new(Mark::Point)).layer(Layer::new(Mark::Line));
    spec.x = Some(ChannelDef::field("gdp").with_legend(false));
    let d = check(&spec, &data());
    assert_eq!(d.iter().filter(|x| x.message.contains("`x(gdp)` is given `legend`")).count(), 1,
        "{:?}", msgs(&d));
}

#[test]
fn two_bindings_that_disagree_about_one_key_are_refused() {
    let on = ChannelDef::field("continent").with_legend(true);
    let off = ChannelDef::field("continent").with_legend(false);
    let spec = base()
        .layer(Layer::new(Mark::Point).encode_def(Channel::Color, off.clone()))
        .layer(Layer::new(Mark::Line).encode_def(Channel::Color, on));
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("leaves its legend out")
        && x.message.contains("shows it, which are opposite things")
        && x.message.contains("one `color` legend")), "{:?}", msgs(&d));

    // Saying nothing beside `FALSE` is not a disagreement: the key is the
    // channel's, and one binding has said what to do with it.
    let spec = base()
        .layer(Layer::new(Mark::Point).encode_def(Channel::Color, off))
        .layer(Layer::new(Mark::Line).encode(Channel::Color, "continent"));
    let d = check(&spec, &data());
    assert!(d.is_empty(), "{:?}", msgs(&d));
}

// -- an axis turned off (spec §10) -------------------------------------

#[test]
fn a_position_takes_axis_and_says_nothing_about_it() {
    // The author's statement, as `legend = FALSE` is: no diagnostic of any
    // kind, in every space that draws axes.
    for coord in [
        CoordSpace::Flat,
        CoordSpace::Polar(crate::ir::PolarView::default()),
        CoordSpace::Map(crate::ir::MapView::default()),
    ] {
        let mut spec = base().layer(Layer::new(Mark::Point)).coord(coord.clone());
        spec.x = Some(ChannelDef::field("gdp").with_axis(false));
        spec.y = Some(ChannelDef::field("life").with_axis(false));
        let d = check(&spec, &data());
        assert!(d.is_empty(), "{coord:?}: {:?}", msgs(&d));
    }
    let mut cube = base().z("value").layer(Layer::new(Mark::Point));
    cube.z = Some(ChannelDef::field("value").with_axis(false));
    let d = check(&cube, &data());
    assert!(d.is_empty(), "z: {:?}", msgs(&d));
}

#[test]
fn axis_on_a_channel_that_draws_none_is_refused_with_direction() {
    let tail = "`axis` belongs on the three positions, `x`, `y` and `z`.";
    let refused = |spec: PlotSpec, why: &str| {
        let d = check(&spec, &data());
        let hit = d.iter().find(|x| x.kind == DiagnosticKind::Illegal && x.message.contains(tail))
            .unwrap_or_else(|| panic!("no axis refusal: {:?}", msgs(&d)));
        assert!(hit.message.contains(why), "{}", hit.message);
    };
    let off = |f: &str| ChannelDef::field(f).with_axis(false);
    refused(base().layer(Layer::new(Mark::Point).encode_def(Channel::Color, off("continent"))),
        "`color(continent)` with its `legend` set to false is how a legend is left out");
    refused(base().layer(Layer::new(Mark::Point).encode_def(Channel::Size, off("gdp"))),
        "`size` is read from its legend");
    refused(base().layer(Layer::new(Mark::Line).encode_def(Channel::Group, off("continent"))),
        "`group` splits the rows without encoding anything");
    refused(base().layer(Layer::new(Mark::Text).encode_def(Channel::Label, off("continent"))),
        "`label` is the text a `text` mark writes");
    refused(base().layer(Layer::new(Mark::Point).encode_def(Channel::Play, off("continent"))),
        "`play` names each frame");
}

#[test]
fn axis_in_a_space_with_no_axes_is_refused() {
    for (coord, why) in [
        (CoordSpace::Globe(crate::ir::GlobeView::default()), "a `globe()` plot draws none"),
        (CoordSpace::Nest, "a `nest()` packing has none"),
    ] {
        let mut spec = base().layer(Layer::new(Mark::Point)).coord(coord);
        spec.x = Some(ChannelDef::field("gdp").with_axis(false));
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("`x(gdp, axis = )` turns an axis on or off")
            && x.message.contains(why) && x.message.contains("Drop `axis`.")),
            "{why}: {:?}", msgs(&d));
    }
}

#[test]
fn two_bindings_that_disagree_about_one_axis_are_refused() {
    let spec = base()
        .layer(Layer::new(Mark::Point).encode_def(Channel::X, ChannelDef::field("gdp").with_axis(false)))
        .layer(Layer::new(Mark::Line).encode_def(Channel::X, ChannelDef::field("gdp").with_axis(true)));
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("`x(gdp)` leaves its axis out and `x(gdp)` draws it")
        && x.message.contains("a plot draws one `x` axis")), "{:?}", msgs(&d));

    // Saying nothing beside `FALSE` is not a disagreement: the axis is shared,
    // and one binding has said what to do with it.
    let spec = base()
        .layer(Layer::new(Mark::Point).encode_def(Channel::X, ChannelDef::field("gdp").with_axis(false)))
        .layer(Layer::new(Mark::Line));
    let d = check(&spec, &data());
    assert!(d.is_empty(), "{:?}", msgs(&d));
}

#[test]
fn an_axis_left_out_cannot_be_named() {
    let mut spec = base().layer(Layer::new(Mark::Point)).x_label("GDP per person");
    spec.x = Some(ChannelDef::field("gdp").with_axis(false));
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("`x_label()` names the x axis, and `x(gdp)` with its `axis` set \
                               to false leaves")
        && x.message.contains("drop `x_label()`, or set `axis` to true")), "{:?}", msgs(&d));
    // The other axis keeps its name.
    let mut spec = base().layer(Layer::new(Mark::Point)).y_label("Life expectancy");
    spec.x = Some(ChannelDef::field("gdp").with_axis(false));
    let d = check(&spec, &data());
    assert!(d.is_empty(), "{:?}", msgs(&d));
}

#[test]
fn a_page_refuses_one_shared_axis_both_drawn_and_left_out() {
    let page = |a: &PlotSpec, b: &PlotSpec| {
        Figure::Page(crate::ir::PageSpec {
            arrange: crate::ir::Arrange::Below, cells: vec![a.clone().into(), b.clone().into()],
            theme: crate::ir::ThemeSpec::default(),
        })
    };
    let with = |shown: Option<bool>| {
        let mut s = base().layer(Layer::new(Mark::Point));
        s.x = Some(ChannelDef { axis: shown, ..ChannelDef::field("gdp") });
        s
    };
    let d = check_figure(&page(&with(Some(false)), &with(Some(true))), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("`gdp` is on the x axis of two plots in this arrangement")
        && x.message.contains("one leaves the axis out")), "{:?}", msgs(&d));
    // One plot leaving it out beside one that says nothing is the page's
    // reading of one axis, and draws.
    for (a, b) in [(with(Some(false)), with(None)), (with(None), with(Some(false)))] {
        let d = check_figure(&page(&a, &b), &data());
        assert!(d.is_empty(), "{:?}", msgs(&d));
    }
}

// -- orientation -----------------------------------------------------

#[test]
fn orientation_is_read_off_the_bound_types() {
    use Orient::*;
    use VarType::{Continuous, Discrete};
    let cases = [
        // (x, y, expected, why)
        (Some(Discrete), Some(Continuous), Vertical, "categories on x"),
        (Some(Continuous), Some(Discrete), Horizontal, "categories on y"),
        (Some(Continuous), Some(Continuous), Vertical, "neither: keep the default"),
        (Some(Discrete), Some(Discrete), Vertical, "illegal; reported separately"),
        // A synthesizing transform fills the axis with no column of its own,
        // and what it writes is what the bars measure.
        (Some(Discrete), None, Vertical, "`bar * count + x(cat)`"),
        (None, Some(Discrete), Horizontal, "`bar * count + y(cat)`"),
        (Some(Continuous), None, Vertical, "`bar * bin + x(height)`"),
        (None, Some(Continuous), Horizontal, "`bar * bin + y(height)`"),
        (None, None, Vertical, "nothing bound"),
    ];
    for (x, y, want, why) in cases {
        assert_eq!(slot_orient(x, y), want, "{why}: ({x:?}, {y:?})");
    }
}

#[test]
fn two_categorical_axes_leave_nothing_to_measure() {
    let spec = PlotSpec::new()
        .data("t")
        .x("continent")
        .y("continent")
        .layer(Layer::new(Mark::Bar));
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal));
    assert!(
        d[0].message.contains("nothing for it to measure"),
        "got: {}", d[0].message
    );
    // Directional: names the way to get a number out of two categories.
    assert!(d[0].message.contains("bar * count"));
}

/// The relational rule is the *family's*, not one mark's. Every slot mark
/// relaxes `y` to Either (so a category there is the horizontal form), which
/// means every one of them also has to refuse the pair that measures nothing —
/// otherwise the relaxation opens a hole in exactly the marks it widened.
/// Written against the family list so a fourth slot mark cannot skip it.
#[test]
fn every_slot_mark_refuses_two_categorical_axes() {
    for mark in [Mark::Bar, Mark::Box, Mark::Interval] {
        let mut layer = Layer::new(mark.clone());
        // `interval`'s minimum syllable includes a pair transform; without one
        // its own refusal would fire and mask the one under test.
        if mark == Mark::Interval {
            layer = layer.transform(Transform::Range);
        }
        let spec = PlotSpec::new().data("t").x("continent").y("continent").layer(layer);
        let d = check(&spec, &data());
        let m = mark_name(&mark);
        assert!(
            d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("categorical columns on both axes")
                && x.message.contains(m)),
            "`{m}` accepted two categorical axes: {:?}",
            d.iter().map(|x| &x.message).collect::<Vec<_>>()
        );
    }
}

/// **Every way to count two categories that the refusal offers draws.** It said
/// "use `bar * count`", which kept the second category on `y`, where a count
/// draws, and was refused for it. Each sentence it names now is checked here.
#[test]
fn a_bar_over_two_categories_is_offered_counts_that_draw() {
    let d = check(&PlotSpec::new().data("t").x("continent").y("region")
        .layer(Layer::new(Mark::Bar)), &data());
    let said = &d.iter().find(|x| x.message.contains("categorical columns on both axes"))
        .expect("refused").message;
    assert!(said.contains("`bar * count + x(continent)`")
        && said.contains("`bar * count + x(region)`")
        && said.contains("`bar * count + x(continent) + y(region) + space()`"), "{said}");
    let cube = || CoordSpace::Space(crate::ir::SpaceView::default());
    let count = || Layer::new(Mark::Bar).transform(Transform::Count);
    for spec in [
        PlotSpec::new().data("t").x("continent").layer(count()),
        PlotSpec::new().data("t").x("region").layer(count()),
        PlotSpec::new().data("t").x("continent")
            .layer(count().encode(Channel::Color, "region")),
        PlotSpec::new().data("t").x("continent").y("region").coord(cube()).layer(count()),
    ] {
        let d = check(&spec, &data());
        assert!(!d.iter().any(Diagnostic::is_fatal), "a direction is refused: {:?}", msgs(&d));
    }
    // One column on both axes has no pairs: the count is of that column alone.
    let d = check(&PlotSpec::new().data("t").x("continent").y("continent")
        .layer(Layer::new(Mark::Bar)), &data());
    assert!(d[0].message.contains("`bar * count + x(continent)` draws one bar for each")
        && !d[0].message.contains("space()"), "{:?}", d[0]);
}

/// **A tally is not short of a measure**, so two categories under one are refused
/// by the column named where it draws, not as a bar with nothing to measure: that
/// refusal told `bar * count` to use `bar * count`. Over every slot mark and every
/// transform that makes the measure, something still refuses.
#[test]
fn a_tally_over_two_categories_is_not_told_it_measures_nothing() {
    for mark in [Mark::Bar, Mark::Box, Mark::Interval] {
        for layer in [
            Layer::new(mark.clone()).transform(Transform::Count),
            Layer::new(mark.clone()).transform(Transform::Proportion),
            Layer::new(mark.clone()).transform(Transform::Bin),
            Layer::new(mark.clone()).transform(Transform::Density),
            Layer::new(mark.clone()).bounds("life", "value"),
            Layer::new(mark.clone()).partition(&["continent", "region"]),
            Layer::new(mark.clone()).flow(&["continent", "region"]),
        ] {
            let spec = PlotSpec::new().data("t").x("continent").y("region").layer(layer.clone());
            let d = check(&spec, &data());
            assert!(d.iter().any(Diagnostic::is_fatal), "{mark:?} {:?} drew", layer.transforms);
            // A `box` refuses `bin`, and its own summary reads a column, so there
            // the `bin` makes no measure: two categories really are short of one,
            // and saying so beside the refusal of `bin` is the reader's next fix.
            let box_bin = mark == Mark::Box && layer.transforms == [Transform::Bin];
            assert!(box_bin || d.iter().all(|x| !x.message.contains("nothing for it to")),
                "{mark:?} {:?}: {:?}", layer.transforms, msgs(&d));
        }
    }
    // The count's own refusal offers the cube, where both columns stay put.
    for t in [Transform::Count, Transform::Proportion] {
        let d = check(&PlotSpec::new().data("t").x("continent").y("region")
            .layer(Layer::new(Mark::Bar).transform(t.clone())), &data());
        assert_eq!(d.len(), 1, "{t:?}: {:?}", msgs(&d));
        assert!(d[0].message.contains("`+ space()` stands one bar on each pair"), "{:?}", d[0]);
    }
}

/// **A floor summary with no `z` is asked for `z`.** In a written `space()` the two
/// categories are the floor, so the refusal names the one position missing; told
/// that one axis must be a number and to count instead, `bar * mean` was sent
/// away from the plot it was one word short of. The note that the plot "is drawn
/// flat" is not printed beside it, since nothing is drawn.
#[test]
fn a_floor_summary_with_no_z_is_asked_for_z() {
    let cube = || CoordSpace::Space(crate::ir::SpaceView::default());
    for layer in [
        Layer::new(Mark::Bar).transform(Transform::Mean),
        Layer::new(Mark::Bar),
        Layer::new(Mark::Box),
        Layer::new(Mark::Interval).transform(Transform::Range),
    ] {
        let spec = PlotSpec::new().data("t").x("continent").y("region").coord(cube())
            .layer(layer.clone());
        let d = check(&spec, &data());
        assert_eq!(d.len(), 1, "{:?}: {:?}", layer.mark, msgs(&d));
        assert!(d[0].message.contains("add `z(<column>)`")
            && !d[0].message.contains("One axis must be a number"), "{:?}", d[0]);
        // What it asks for draws.
        let asked = PlotSpec::new().data("t").x("continent").y("region").z("value")
            .coord(cube()).layer(layer.clone());
        assert!(!check(&asked, &data()).iter().any(Diagnostic::is_fatal),
            "{:?}: {:?}", layer.mark, msgs(&check(&asked, &data())));
    }
    // Only a bare bar is offered the count, which draws too; a summary keeps its own.
    let bare = check(&PlotSpec::new().data("t").x("continent").y("region").coord(cube())
        .layer(Layer::new(Mark::Bar)), &data());
    assert!(bare[0].message.contains("`bar * count` stands the count up on `z`"), "{bare:?}");
    let mean = check(&PlotSpec::new().data("t").x("continent").y("region").coord(cube())
        .layer(Layer::new(Mark::Bar).transform(Transform::Mean)), &data());
    assert!(!mean[0].message.contains("bar * count"), "{mean:?}");
}

/// The Law-2 parity the horizontal box/interval work closed: a slot mark's two
/// axes have the **same role** — one holds the slots, the other the measure —
/// so any one of them accepting a category on `x` but refusing it on `y` is a
/// gap, not a design. (A *path* is exempt and deliberately absent here: its `x`
/// is permanently the domain and its `y` the measure, different roles, spec §6.)
#[test]
fn a_slot_marks_two_position_axes_carry_the_same_rule() {
    for mark in [Mark::Bar, Mark::Box, Mark::Interval] {
        let (x, y) = (rule_for(&mark, &Channel::X), rule_for(&mark, &Channel::Y));
        let m = mark_name(&mark);
        assert_eq!(x.obligation, y.obligation, "`{m}`: x and y disagree on obligation");
        assert_eq!(x.accepts, y.accepts, "`{m}`: x and y disagree on accepted type");
        assert_eq!(x.renders, y.renders, "`{m}`: x and y disagree on what renders");
    }
}

/// A horizontal box/whisker end to end: a category on `y`, the measure on `x`,
/// and no diagnostic at all. The mirror of the upright form, which is the whole
/// claim — `slot_orient` reads the pair, and there is no `flip` to write.
#[test]
fn a_category_on_y_is_the_horizontal_box_and_whisker() {
    let cases = [
        (Mark::Box, None),
        (Mark::Interval, Some(Transform::Range)),
        (Mark::Interval, Some(Transform::Confidence)),
    ];
    for (mark, t) in cases {
        let mut layer = Layer::new(mark.clone());
        if let Some(t) = t { layer = layer.transform(t); }
        let spec = PlotSpec::new().data("t").x("gdp").y("continent").layer(layer);
        let d = check(&spec, &data());
        assert!(
            d.is_empty(),
            "`{}` refused a category on y: {:?}",
            mark_name(&mark),
            d.iter().map(|x| &x.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_horizontal_bar_may_omit_the_axis_its_transform_writes() {
    // `bar * count + y(cat)` invents x, exactly as `+ x(cat)` invents y.
    // Requiring `x()` here would be a vertical-only assumption.
    let spec = PlotSpec::new()
        .data("t")
        .y("continent")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count));
    assert!(check(&spec, &data()).is_empty(), "{:?}", check(&spec, &data()));

    let spec = PlotSpec::new()
        .data("t")
        .x("continent")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count));
    assert!(check(&spec, &data()).is_empty());
}

#[test]
fn a_categorical_y_is_legal_on_bar_but_still_refused_on_line() {
    // Relaxing `rule_for(Bar, Y)` to Either must not leak into other marks:
    // a line has no notion of sitting on a category.
    assert!(check(
        &PlotSpec::new().data("t").x("gdp").y("continent").layer(Layer::new(Mark::Bar)),
        &data()
    ).is_empty());

    let d = check(
        &PlotSpec::new().data("t").x("gdp").y("continent").layer(Layer::new(Mark::Line)),
        &data(),
    );
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
}

// -- channel scope ---------------------------------------------------

#[test]
fn a_plot_scoped_channel_reaches_every_layer_that_accepts_it() {
    let spec = base()
        .channel(Channel::Color, "continent")
        .layer(Layer::new(Mark::Line))
        .layer(Layer::new(Mark::Point));
    let r = resolve_scopes(&spec);
    for layer in &r.layers {
        assert_eq!(
            layer.encodings.get(&Channel::Color).map(|c| c.field.as_str()),
            Some("continent"),
            "{:?} should have picked up the plot-scoped color",
            layer.mark
        );
    }
    assert!(check(&spec, &data()).is_empty());
}

#[test]
fn a_layer_binding_beats_the_plot_scoped_one() {
    // Nearest wins — the same rule that governs `data()`.
    let spec = base()
        .channel(Channel::Color, "continent")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "country"));
    let r = resolve_scopes(&spec);
    assert_eq!(
        r.layers[0].encodings[&Channel::Color].field,
        "country"
    );
}

#[test]
fn a_plot_scoped_channel_skips_marks_without_the_feature() {
    // This is the case the old backward broadcast could not express: `size`
    // belongs to the points, and the line beside them simply has no size.
    let spec = base()
        .channel(Channel::Size, "gdp")
        .layer(Layer::new(Mark::Line))
        .layer(Layer::new(Mark::Point));
    let r = resolve_scopes(&spec);
    assert!(!r.layers[0].encodings.contains_key(&Channel::Size), "line must be skipped");
    assert!(r.layers[1].encodings.contains_key(&Channel::Size), "point must take it");

    // Skipping is reported, never silent — but it still renders.
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Assumption]);
    assert!(!d[0].is_fatal());
    assert!(d[0].message.contains("`point`") && d[0].message.contains("`line`"));
}

#[test]
fn a_plot_scoped_channel_no_mark_accepts_is_illegal() {
    let spec = base()
        .channel(Channel::Size, "gdp")
        .layer(Layer::new(Mark::Line));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("no mark here has a size feature"));
}

/// A plot-wide channel every mark overrides reaches nothing: it drew byte for
/// byte as the sentence without it, and beside a `line` the note said it
/// applied to the points, which were sized by their own column.
#[test]
fn a_plot_scoped_channel_every_mark_overrides_is_illegal() {
    for lead in [None, Some(Mark::Line)] {
        let mut spec = base().channel(Channel::Size, "gdp");
        if let Some(m) = lead {
            spec = spec.layer(Layer::new(m));
        }
        let spec = spec.layer(Layer::new(Mark::Point).encode(Channel::Size, "life"));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{d:?}");
        assert!(d[0].message.contains("`point` maps `size(life)`")
                && d[0].message.contains("reaches none of them"), "{}", d[0].message);
    }
    // One mark that keeps the plot's column is enough for it to apply.
    let spec = base().channel(Channel::Size, "gdp")
        .layer(Layer::new(Mark::Point).encode(Channel::Size, "gdp"))
        .layer(Layer::new(Mark::Point));
    assert!(check(&spec, &data()).iter().all(|d| !d.message.contains("reaches none")));
}

/// **A zone that reads its positions checks their columns.** Only a zone whose
/// sides are named by `bounds`, or a plain one over a slot, is spared the
/// missing-column check; a density, a bin, a tally or a partition reads `x` and
/// `y` like any other mark. Spared, `zone * density + y(petal_width)` over a
/// table without the column drew an empty panel in silence.
#[test]
fn a_zone_that_reads_its_positions_checks_their_columns() {
    let refused = |spec: PlotSpec, field: &str| {
        let d = check(&spec, &data());
        d.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains(&format!("({field})` refers to a column that is not in the data")))
    };
    for t in [Transform::Density, Transform::Bin] {
        let spec = PlotSpec::new().data("t").x("gdp").y("lif").layer(Layer::new(Mark::Zone).transform(t.clone()));
        assert!(refused(spec, "lif"), "{t:?}");
    }
    let tally = PlotSpec::new().data("t").x("continent").y("regio")
        .layer(Layer::new(Mark::Zone).transform(Transform::Count));
    assert!(refused(tally, "regio"));
    let sunburst = PlotSpec::new().data("t").x("gdpp")
        .layer(Layer::new(Mark::Zone).transform(Transform::Partition).partition(&["continent"]));
    assert!(refused(sunburst, "gdpp"));

    // A zone named by `bounds` reads its own columns: the plot's `x` is absent
    // from its table by design, and nothing is reported missing.
    let mut data = data();
    data.insert("bands".into(), DataFrame::new()
        .with_float("start", vec![1.5]).with_float("end", vec![2.5]));
    let mut band = Layer::new(Mark::Zone).transform(Transform::Bounds).data("bands");
    band.bounds = Some(crate::ir::BoundsSpec {
        start: Some("start".into()), end: Some("end".into()), ..Default::default()
    });
    let spec = base().layer(Layer::new(Mark::Point)).layer(band);
    let d = check(&spec, &data);
    assert!(d.iter().all(|d| !d.message.contains("refers to a column")), "{d:?}");
}

/// **A zone's and a surface's transform refusals are read off the table.** The
/// zone's was one fixed sentence that called `bounds` refused though `zone *
/// bounds` draws, with a list of what a zone takes that had drifted four short;
/// the surface's explained five of the thirteen it refuses. Every refused
/// transform is named with its own reason now, `quantile` on a surface alone
/// falling through to the plain one, which no reason on record explains.
#[test]
fn a_cell_marks_transform_refusal_says_why_and_lists_the_table() {
    let msg = |mark: Mark, t: Transform| {
        let mut out = Vec::new();
        check_marks_that_take_no_transform(&mut out, &Layer::new(mark).transform(t));
        out.pop().map(|d| d.message).unwrap_or_default()
    };
    for t in USER_TRANSFORMS.iter().filter(|t| !is_collision_modifier(t)) {
        for mark in [Mark::Zone, Mark::Surface] {
            if mark_takes_transform(&mark, t) != TransformLegality::None { continue }
            let m = msg(mark.clone(), t.clone());
            // The four that refuse a mark in their own check are left to it, and
            // the whole check still refuses them by name.
            if refuses_marks_itself(t) {
                assert!(m.is_empty(), "{mark:?} {t:?} is its own check's: {m}");
                let d = check(&PlotSpec::new().data("t").x("gdp").y("life").z("value")
                    .layer(Layer::new(mark.clone()).transform(t.clone())), &data());
                assert!(d.iter().any(|x| x.is_fatal()
                    && x.message.contains(&format!("`{}`", transform_name(t)))),
                    "{mark:?} {t:?}: {:?}", msgs(&d));
                continue;
            }
            assert!(m.contains(&format!("`{}`", transform_name(t))), "{mark:?} {t:?}: {m}");
            if *t != Transform::Quantile {
                assert!(!m.contains("is not among"), "{mark:?} {t:?} has no reason: {m}");
            }
        }
    }
    let zone = msg(Mark::Zone, Transform::Smooth);
    for t in ["bin", "density", "bounds", "partition", "flow", "cluster", "quantile"] {
        assert!(zone.contains(&format!("`{t}`")), "a zone takes `{t}`: {zone}");
    }
    assert!(msg(Mark::Surface, Transform::Count).contains("`bin` and `density` tile a floor"));
}

/// **A refused transform is not followed by a missing `y`.** `cluster` on a mark
/// that does not take it is refused with the two marks that do; the missing-`y`
/// refusal beside it said "Add `y(<column>)`", and adding one only reached
/// another refusal. Every mark that must have a `y` printed it.
#[test]
fn a_refused_transform_is_not_followed_by_a_missing_y() {
    for m in [Mark::Line, Mark::Step, Mark::Area, Mark::Point, Mark::Bar] {
        let mut l = Layer::new(m.clone()).transform(Transform::Cluster);
        l.cluster = Some(crate::ir::ClusterSpec {
            value: Some("gdp".into()), over: Some("region".into()), ..Default::default()
        });
        let d = check(&PlotSpec::new().data("t").x("continent").layer(l), &data());
        assert!(d.iter().any(|d| d.message.contains("`cluster` joins the closest leaves")),
            "{m:?}: the transform's own refusal: {d:?}");
        assert!(d.iter().all(|d| !d.message.contains("but none is set")), "{m:?}: {d:?}");
    }
}

/// **A layer the graph places is refused once outside the network, and the one
/// refusal names it.** An `edge` or a `layout` has positions only in
/// `network()`, and its own refusal says so. The checks for other spaces added a
/// second remedy that failed: "Drop `polar()` to draw it flat", "Drop `nest()`",
/// the `space()` note's "drawn flat", and a `map()`'s missing `x` and `y`.
#[test]
fn a_layer_the_graph_places_is_refused_once_outside_the_network() {
    let spaces = [
        CoordSpace::Polar(crate::ir::PolarView::default()),
        CoordSpace::Nest,
        CoordSpace::Space(crate::ir::SpaceView::default()),
        CoordSpace::Map(crate::ir::MapView::default()),
    ];
    for coord in spaces {
        for layer in [Layer::new(Mark::Edge), Layer::new(Mark::Edge).layout("continent", "region"),
                      Layer::new(Mark::Point).layout("continent", "region")] {
            let spec = PlotSpec::new().data("t").coord(coord.clone()).layer(layer.clone());
            let out = check(&spec, &data());
            assert!(out.iter().any(|d| d.message.contains("network()")),
                "{coord:?} {:?}: the layer's own refusal: {:?}", layer.mark, msgs(&out));
            assert_eq!(out.len(), 1, "{coord:?} {:?}: one refusal: {:?}", layer.mark, msgs(&out));
        }
    }
}

#[test]
fn order_no_longer_changes_meaning() {
    // The defect this replaced: `set_channel` reached backwards, so the same
    // atoms in a different order produced different plots. Binding forward
    // only, a layer written after the channel is unaffected by it.
    let a = resolve_scopes(
        &base()
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"))
            .layer(Layer::new(Mark::Line)),
    );
    let b = resolve_scopes(
        &base()
            .layer(Layer::new(Mark::Line))
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "continent")),
    );
    let colored = |s: &PlotSpec| {
        s.layers
            .iter()
            .filter(|l| l.encodings.contains_key(&Channel::Color))
            .map(|l| format!("{:?}", l.mark))
            .collect::<Vec<_>>()
    };
    assert_eq!(colored(&a), vec!["Point"]);
    assert_eq!(colored(&b), vec!["Point"]);
}

#[test]
fn resolving_twice_changes_nothing() {
    // `check` and the renderer both resolve on entry; that must be safe.
    let spec = base()
        .channel(Channel::Color, "continent")
        .layer(Layer::new(Mark::Point));
    let once = resolve_scopes(&spec);
    let twice = resolve_scopes(&once);
    assert_eq!(
        format!("{:?}", once.layers[0].encodings),
        format!("{:?}", twice.layers[0].encodings)
    );
}

// -- set vs map ------------------------------------------------------

#[test]
fn a_constant_is_settable_exactly_where_a_column_is_not_mappable() {
    // The whole point of the distinction. A polyline is one stroke, so
    // `opacity`/`size` cannot vary per row — yet that same one stroke has
    // a width and an opacity, so setting them is fine.
    for ch in [Channel::Opacity, Channel::Size] {
        let mapped = check(
            &base().layer(Layer::new(Mark::Line).encode(ch.clone(), "gdp")),
            &data(),
        );
        assert_eq!(
            kinds(&mapped),
            vec![DiagnosticKind::Illegal],
            "{ch:?} must not be mappable on line"
        );
    }
    let set = check(
        &base().layer(Layer::new(Mark::Line).style_opacity(0.4).style_size(6.0)),
        &data(),
    );
    assert!(set.is_empty(), "style must be accepted on line: {set:?}");
}

#[test]
fn a_set_only_refusal_names_the_setting_a_plain_one_does_not() {
    // The two halves of `Cannot`, told apart by what the reader is handed.
    // `size` on a line is `SET_ONLY`, so the refusal has somewhere to send
    // them; `size` on a bar is `CANNOT`, and inventing a `style()` there
    // would be the second wall §12 warns about — a direction the engine
    // itself refuses.
    let set_only = check(
        &base().layer(Layer::new(Mark::Line).encode(Channel::Size, "gdp")),
        &data(),
    );
    let msg = &set_only[0].message;
    assert!(msg.contains("style(size = )"), "must hand over the setting: {msg}");
    assert!(msg.contains("not one per row"), "must say why mapping fails: {msg}");
    assert!(!msg.contains("has no size feature"), "a line does have a size: {msg}");

    let none = check(
        &base().layer(Layer::new(Mark::Bar).encode(Channel::Size, "gdp")),
        &data(),
    );
    let msg = &none[0].message;
    assert!(msg.contains("has no size feature"), "a bar has none: {msg}");
    assert!(!msg.contains("style("), "nothing to set on a bar either: {msg}");
}

#[test]
fn every_set_only_refusal_offers_its_setting() {
    // The whole class rather than one example, generated from the rule table
    // so a mark added later joins the check by existing. Sixteen pairs today,
    // every one of them `size` or `opacity` on a mark drawn as one stroke or
    // one region.
    let mut seen = 0;
    for m in ALL_MARKS.iter() {
        for c in ALL_CHANNELS.iter() {
            let r = rule_for(m, c);
            if r.obligation != Obligation::Cannot || !r.settable {
                continue;
            }
            seen += 1;
            let name = channel_name(c);
            let d = check(
                &base().layer(Layer::new(m.clone()).encode(c.clone(), "gdp")),
                &data(),
            );
            // Match on the channel under test: a mark can refuse more than
            // one binding at a time, and `edge` refuses the base spec's `x`
            // before it ever reaches `size`.
            let wanted = format!("`{name}` cannot be bound");
            let msg = d
                .iter()
                .find(|d| d.message.contains(&wanted))
                .map(|d| d.message.clone())
                .unwrap_or_else(|| panic!("{name} on {} must refuse", mark_name(m)));
            assert!(
                msg.contains(&format!("style({name} = )")),
                "{name} on {}: {msg}",
                mark_name(m)
            );
        }
    }
    assert!(seen >= 16, "expected the set-only class, found {seen}");
}

#[test]
fn setting_a_feature_the_mark_does_not_have_is_illegal() {
    // `shape` on bar is Cannot for a different reason than `opacity` on
    // line: a bar has no glyph at all, so there is nothing to set either.
    let d = check(
        &base().layer(Layer::new(Mark::Bar).style_shape("square")),
        &data(),
    );
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("has no shape to set"));
}

#[test]
fn mapping_and_setting_the_same_feature_is_illegal() {
    // Honoring one would mean silently dropping the other.
    let d = check(
        &base().layer(
            Layer::new(Mark::Point)
                .encode(Channel::Color, "continent")
                .style_color("red"),
        ),
        &data(),
    );
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("one layer cannot do both"));
}

/// **A color the reading fills by itself is a mapping too** (spec §8, the column
/// rules' third). Every two-dimensional reading measures with `color` unbound, and
/// a set color beside it is the contradiction a written mapping is refused for:
/// drawn, a heatmap painted every cell the set color under a legend that still
/// decoded the count, and a contour dropped the setting without a word.
#[test]
fn a_set_color_over_a_color_the_reading_fills_is_illegal() {
    let readings = [
        (Layer::new(Mark::Zone).transform(Transform::Bin), "count"),
        (Layer::new(Mark::Zone).transform(Transform::Density), "density"),
        (Layer::new(Mark::Path).transform(Transform::Density), "level"),
    ];
    for (layer, measure) in readings {
        let d = check(&base().layer(layer.style_color("black")), &data());
        let said = msgs(&d);
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains(&format!("maps color to its `{measure}` by itself"))
            && x.message.contains("one layer cannot do both")), "{measure}: {said:?}");
    }
    // The tile plot fills color the same way, over two categories.
    let tiles = PlotSpec::new().data("t").x("continent").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Count).style_color("black"));
    assert!(msgs(&check(&tiles, &data())).iter()
        .any(|m| m.contains("maps color to its `count` by itself")));
    // Opacity is not filled by the reading, so setting it is still a setting.
    let mut faded = Layer::new(Mark::Zone).transform(Transform::Bin);
    faded.style.opacity = Some(0.5);
    let d = check(&base().layer(faded), &data());
    assert!(!d.iter().any(|x| x.kind == DiagnosticKind::Illegal), "{:?}", msgs(&d));
}

/// **A channel reads the rows the mark draws** (spec §8, the column rules' fourth).
/// A name the transform writes means the transform's column even when the table
/// holds one by that name, since the transform replaced the rows; `count` and
/// `name` are ordinary names in a table, so this draws, with a note that says which
/// column was read. Before, the measure axis refused it while every other channel
/// took the transform's column without a word.
#[test]
fn a_written_name_the_table_also_holds_draws_with_a_note() {
    let counted = || {
        let mut m = data();
        let t = m.remove("t").unwrap().with_float("count", vec![5.0, 6.0, 7.0])
            .with_str("name", vec!["p".into(), "q".into(), "r".into()])
            .with_float("degree", vec![99.0, 99.0, 99.0]);
        m.insert("t".to_string(), t);
        m
    };
    let note = |d: &[Diagnostic], name: &str| d.iter().any(|x| x.kind == DiagnosticKind::Assumption
        && x.message.contains(&format!("names the `{name}` that"))
        && x.message.contains(&format!("not your table's `{name}` column")));
    // On the measure axis, with the direction a table of tallies needs.
    let bars = PlotSpec::new().data("t").x("continent")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count).encode(Channel::Y, "count"));
    let d = check(&bars, &counted());
    assert!(note(&d, "count") && !d.iter().any(|x| x.kind == DiagnosticKind::Illegal), "{:?}", msgs(&d));
    assert!(msgs(&d).iter().any(|m| m.contains("write `bar * sum + y(count)`")), "{:?}", msgs(&d));
    // On the color a heatmap fills.
    let cells = base().layer(Layer::new(Mark::Zone).transform(Transform::Bin).encode(Channel::Color, "count"));
    let d = check(&cells, &counted());
    assert!(note(&d, "count") && !d.iter().any(|x| x.kind == DiagnosticKind::Illegal), "{:?}", msgs(&d));
    // On the columns a layout publishes about its nodes.
    let net = || CoordSpace::Network(crate::ir::NetworkView::default());
    let nodes = PlotSpec::new().data("t").coord(net())
        .layer(Layer::new(Mark::Point).layout("continent", "region").encode(Channel::Size, "degree"))
        .layer(Layer::new(Mark::Text).layout("continent", "region").encode(Channel::Label, "name"));
    let d = check(&nodes, &counted());
    assert!(note(&d, "degree") && note(&d, "name"), "{:?}", msgs(&d));
    // On the ring axis a partition writes.
    let mut deep = counted();
    let t = deep.remove("t").unwrap().with_float("depth", vec![99.0, 99.0, 99.0]);
    deep.insert("t".to_string(), t);
    let rings = PlotSpec::new().data("t").x("gdp").y("depth")
        .layer(Layer::new(Mark::Zone).partition(&["continent"]));
    let d = check(&rings, &deep);
    assert!(note(&d, "depth"), "{:?}", msgs(&d));
    // A label nobody wrote is filled by the reading, and clashes with nothing.
    let unnamed = PlotSpec::new().data("t").coord(net())
        .layer(Layer::new(Mark::Text).layout("continent", "region"));
    let d = check(&unnamed, &counted());
    assert!(!d.iter().any(|x| x.message.contains("not your table's")), "{:?}", msgs(&d));
    // Without a clash there is nothing to say.
    let d = check(&bars, &data());
    assert!(!d.iter().any(|x| x.message.contains("not your table's")), "{:?}", msgs(&d));
}

/// **The one refusal under the fourth column rule is a real collision**: another
/// channel reads the table's column of the same name, so it is carried into the
/// drawn rows beside the written one, and one name would mean two columns.
#[test]
fn a_written_name_that_another_channel_reads_from_the_table_is_illegal() {
    let mut m = data();
    let t = m.remove("t").unwrap().with_str("count", vec!["low".into(), "high".into(), "low".into()]);
    m.insert("t".to_string(), t);
    let both = PlotSpec::new().data("t").x("count")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count).encode(Channel::Y, "count"));
    let d = check(&both, &m);
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("`x(count)` reads your table's `count` column")
        && x.message.contains("one name would mean two columns")
        && x.message.contains("leave `y()` out")), "{:?}", msgs(&d));
    // The table's column as the category alone is no collision: the tally goes on `y`.
    let one = PlotSpec::new().data("t").x("count")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count));
    let d = check(&one, &m);
    assert!(!d.iter().any(|x| x.kind == DiagnosticKind::Illegal), "{:?}", msgs(&d));
}

/// **`short` writes a label's large numbers short** (spec §10), judged as
/// `decimals` is: drawn on `label`, said not drawn on any other channel, refused
/// on a column of text, and nothing to judge when it is set to false.
#[test]
fn short_writes_a_labels_numbers_and_nothing_else() {
    let labeled = |field: &str, short: bool| PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Text).encode_def(Channel::Label,
            ChannelDef { short: Some(short), ..ChannelDef::field(field) }));
    let d = check(&labeled("life", true), &data());
    assert!(!d.iter().any(|x| x.is_fatal() || x.message.contains("short")), "{:?}", msgs(&d));
    let d = check(&labeled("continent", true), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("`continent` is a categorical column")
        && x.message.contains("no number to write short")), "{:?}", msgs(&d));
    assert!(!check(&labeled("continent", false), &data()).iter().any(|x| x.is_fatal()));
    let mut spec = PlotSpec::new().data("t").y("life").layer(Layer::new(Mark::Point));
    spec.x = Some(ChannelDef { short: Some(true), ..ChannelDef::field("gdp") });
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Unsupported
        && x.message.contains("with `short` set to true")
        && x.message.contains("only on `label()` today")), "{:?}", msgs(&d));
}

/// **`decimals` rounds what a label prints** (spec §10): a reader cannot round a
/// mean gog computes, so the binding carries the places. Only `label` prints a
/// column's numbers as text of its own, a column of text has none to round
/// (a written one included), and more places than a measurement carries is a
/// mistake about the number.
#[test]
fn decimals_round_a_labels_numbers_and_nothing_else() {
    let labeled = |field: &str, d: u32| PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Text).encode_def(Channel::Label,
            ChannelDef { decimals: Some(d), ..ChannelDef::field(field) }));
    let d = check(&labeled("life", 1), &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "{:?}", msgs(&d));
    let d = check(&labeled("continent", 1), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("`continent` is a categorical column, read as names rather than numbers")), "{:?}", msgs(&d));
    let d = check(&labeled("life", 12), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("Write a number from 0 to 10")), "{:?}", msgs(&d));
    // A written name of text: a partition's `name`.
    let d = check(&PlotSpec::new().data("t").x("gdp")
        .layer(Layer::new(Mark::Text).partition(&["continent", "region"])
            .encode_def(Channel::Label, ChannelDef { decimals: Some(1), ..ChannelDef::field("name") })), &data());
    assert!(d.iter().any(|x| x.message.contains("`name` is a categorical column")), "{:?}", msgs(&d));
    // On an axis: said not drawn, with the direction to leave it out.
    let mut spec = PlotSpec::new().data("t").y("life").layer(Layer::new(Mark::Point));
    spec.x = Some(ChannelDef { decimals: Some(1), ..ChannelDef::field("gdp") });
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Unsupported
        && x.message.contains("only on `label()` today")), "{:?}", msgs(&d));
}

/// **A text prints a summary** (spec §8, the rows rule). `text * mean + label(life)`
/// was refused as "no label to draw", which the rows rule made the wrong reason:
/// the summary's rows hold `life`, reduced in place, and the key. A label those rows
/// do not hold is refused toward the two they do, a missing label is told which
/// prints the summary, and a text that summarizes reads its orientation as a bar.
#[test]
fn a_text_prints_the_summary_its_rows_hold() {
    for t in [Transform::Sum, Transform::Mean, Transform::Median, Transform::Max,
              Transform::Min, Transform::Quantile] {
        assert_eq!(mark_takes_transform(&Mark::Text, &t), TransformLegality::Combines, "{t:?}");
    }
    for t in [Transform::Smooth, Transform::Range] {
        assert_eq!(mark_takes_transform(&Mark::Text, &t), TransformLegality::None, "{t:?}");
    }
    // Refused for what each is, not as "one summary per key", which a text prints.
    let refusal = |t: Transform| check(&PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Text).transform(t).encode(Channel::Label, "life")), &data())
        .into_iter().filter(|x| x.is_fatal()).map(|x| x.message).collect::<Vec<_>>().join(" ");
    let m = refusal(Transform::Smooth);
    assert!(m.contains("fits a curve at points of its own") && m.contains("`text * mean"), "{m}");
    let m = refusal(Transform::Range);
    assert!(m.contains("a low and a high value") && m.contains("`text * max`"), "{m}");
    assert!(!m.contains("one summary per key"), "{m}");
    let mean = |label: Option<&str>| {
        let l = Layer::new(Mark::Text).transform(Transform::Mean);
        PlotSpec::new().data("t").x("continent").y("life")
            .layer(match label { Some(f) => l.encode(Channel::Label, f), None => l })
    };
    for ok in ["life", "continent"] {
        let d = check(&mean(Some(ok)), &data());
        assert!(!d.iter().any(|x| x.is_fatal()), "label({ok}): {:?}", msgs(&d));
    }
    let d = check(&mean(Some("region")), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("which have no `region`")
        && x.message.contains("`label(life)` prints each mean")
        && x.message.contains("`label(continent)` names each one")), "{:?}", msgs(&d));
    assert!(!d.iter().any(|x| x.message.contains("no label to draw")), "{:?}", msgs(&d));
    let d = check(&mean(None), &data());
    assert!(d.iter().any(|x| x.message.contains(
        "`label(life)` prints the mean that `text * mean` computes")), "{:?}", msgs(&d));

    // On its side, beside horizontal bars: grouped by the category on `y`.
    let sideways = Layer::new(Mark::Text).transform(Transform::Mean).encode(Channel::Label, "life");
    assert!(reads_orientation(&sideways));
    assert!(!reads_orientation(&Layer::new(Mark::Text)));
    let d = check(&PlotSpec::new().data("t").x("life").y("continent").layer(sideways), &data());
    assert!(!d.iter().any(|x| x.is_fatal() || x.message.contains("every group holds")), "{:?}", msgs(&d));
    let d = check(&PlotSpec::new().data("t").y("continent")
        .layer(Layer::new(Mark::Text).transform(Transform::Count).encode(Channel::Label, "count")), &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "sideways count labels: {:?}", msgs(&d));
}

/// **The labels of dodged bars stand over their own bars** (spec §5; ruled
/// 2026-10-10). `text * dodge` was refused ("a label's width is its word, not a
/// slot"), and `group` on `text` with it, so `bar * mean * dodge + color(era)` could
/// not be labeled. A text now dodges as a bar does, given a split, and `group`
/// splits a text that dodges or summarizes without coloring its labels.
#[test]
fn a_dodged_text_labels_dodged_bars() {
    assert_eq!(mark_takes_transform(&Mark::Text, &Transform::Dodge), TransformLegality::Combines);
    let refused = |l: Layer| check(&PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Bar).transform(Transform::Mean).transform(Transform::Dodge)
            .encode(Channel::Color, "region"))
        .layer(l), &data())
        .into_iter().filter(|x| x.is_fatal()).map(|x| x.message).collect::<Vec<_>>().join(" ");
    let labels = || Layer::new(Mark::Text).transform(Transform::Mean).transform(Transform::Dodge)
        .encode(Channel::Label, "life");
    assert_eq!(refused(labels().encode(Channel::Group, "region")), "");
    assert_eq!(refused(labels().encode(Channel::Color, "region")), "");
    // The split's own column is on the summary's rows, so it can be printed.
    assert_eq!(refused(labels().encode(Channel::Group, "region").encode(Channel::Label, "region")), "");
    let m = refused(labels());
    assert!(m.contains("`text * dodge` has no groups to set side by side")
        && m.contains("`color(<field>)` or `group(<field>)`"), "{m}");
    // `group` with nothing to act on: toward `dodge` beside bars, toward `color` on a scatter.
    let m = refused(Layer::new(Mark::Text).encode(Channel::Label, "life").encode(Channel::Group, "region"));
    assert!(m.contains("over its own bar") && m.contains("`text * mean * dodge`"), "{m}");
    let d = check(&base().layer(Layer::new(Mark::Text).encode(Channel::Label, "continent")
        .encode(Channel::Group, "continent")), &data());
    assert!(d.iter().any(|x| x.is_fatal() && x.message.contains("no category axis")
        && x.message.contains("map `continent` to `color`")), "{:?}", msgs(&d));
}

/// **A number of your table is refused on rows a transform replaced** (spec §8, the
/// column rules' fourth; ruled 2026-10-10). `point * count + size(gdp)` was drawn
/// with `size` dropped and a `gdp` legend beside it, the key decoding sizes no point
/// had. What still reaches those rows keeps drawing: a category the transform groups
/// by, the column the statistic reads, a reading's own measuring column, and the
/// columns `group` and `play` split by before the transform runs.
#[test]
fn a_number_of_your_table_is_refused_on_rows_a_transform_replaced() {
    let refusal = |spec: PlotSpec| {
        check(&spec, &data()).into_iter()
            .find(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("names a column of your table"))
            .map(|x| x.message)
    };
    let by = |x: &str| PlotSpec::new().data("t").x(x);

    let m = refusal(by("continent").layer(Layer::new(Mark::Point).transform(Transform::Count)
        .encode(Channel::Size, "gdp"))).expect("a tally's rows have no gdp");
    assert!(m.contains("`point * count` replaces your rows with its own, one for each \
        `continent`, which have no `gdp`"), "{m}");
    assert!(m.contains("one row for each `continent`, holding its count and a summary of `gdp`. \
        Draw that table with `point`, without `count`, and put the count on `y`.")
        && m.contains("leave `size(gdp)` out"), "{m}");

    let m = refusal(by("continent").y("life").layer(Layer::new(Mark::Bar).transform(Transform::Mean)
        .encode(Channel::Opacity, "gdp"))).expect("a mean's rows have no gdp");
    assert!(m.contains("`bar * mean` replaces your rows") && m.contains("holding the mean of `life`"), "{m}");
    let m = refusal(by("life").layer(Layer::new(Mark::Point).transform(Transform::Bin)
        .encode(Channel::Color, "gdp"))).expect("a bin's rows have no gdp");
    assert!(m.contains("one for each bin"), "{m}");
    let m = refusal(by("gdp").y("life").layer(Layer::new(Mark::Point).transform(Transform::Smooth)
        .encode(Channel::Size, "value"))).expect("a fitted curve's points have no value");
    assert!(m.contains("the points of the curve it fits") && m.contains("a layer of their own"), "{m}");
    // A low and a high value cannot be redrawn by the mark alone, so the direction is
    // the reader's own rows in a layer beside it.
    let m = refusal(by("continent").y("life").layer(Layer::new(Mark::Line).transform(Transform::Range)
        .encode(Channel::Color, "gdp"))).expect("a range's rows have no gdp");
    assert!(m.contains("a low and a high value for each `continent`") && m.contains("a layer of their own"), "{m}");
    // A transform the mark does not take is refused on its own, and only so.
    let d = check(&by("continent").y("life").layer(Layer::new(Mark::Bar).transform(Transform::Range)
        .encode(Channel::Opacity, "gdp")), &data());
    assert!(d.iter().any(|x| x.message.contains("`range` produces a low value")), "{:?}", msgs(&d));
    assert!(!d.iter().any(|x| x.message.contains("names a column of your table")), "{:?}", msgs(&d));
    let m = refusal(PlotSpec::new().data("t").layer(Layer::new(Mark::Bar).transform(Transform::Count)
        .encode(Channel::Opacity, "gdp"))).expect("one row in all has no gdp");
    assert!(m.contains("with a single row, which has no `gdp`"), "{m}");
    // A category beside the number is a key, and the message names it with the axis.
    let m = refusal(by("continent").y("life").layer(Layer::new(Mark::Point).transform(Transform::Mean)
        .encode(Channel::Color, "region").encode(Channel::Size, "gdp"))).expect("still no gdp");
    assert!(m.contains("one for each `continent` and `region`"), "{m}");

    // Still drawn.
    for spec in [
        // A category is carried: the transform counts within it.
        by("continent").layer(Layer::new(Mark::Point).transform(Transform::Count)
            .encode(Channel::Color, "region")),
        // The column the statistic reads holds each mean, on any channel.
        by("continent").y("life").layer(Layer::new(Mark::Point).transform(Transform::Mean)
            .encode(Channel::Size, "life")),
        // A reading that measures with color reads its column there.
        by("continent").y("region").layer(Layer::new(Mark::Zone).transform(Transform::Mean)
            .encode(Channel::Color, "gdp")),
        // No transform, no replaced rows.
        by("gdp").y("life").layer(Layer::new(Mark::Point).encode(Channel::Size, "value")),
    ] {
        let d = check(&spec, &data());
        assert!(!d.iter().any(|x| x.message.contains("names a column of your table")), "{:?}", msgs(&d));
    }
    // `play` cuts the rows into frames before the transform runs.
    let played = by("continent").y("life").layer(Layer::new(Mark::Point).transform(Transform::Mean))
        .channel(Channel::Play, "value");
    assert!(refusal(played).is_none());

    // A written name over a table column of that name keeps its one note, for
    // however many channels say it, and no longer sends the reader to rename it.
    let mut own = data();
    let tab = own.remove("t").unwrap().with_float("count", vec![7.0, 8.0, 9.0]);
    own.insert("t".to_string(), tab);
    let d = check(&by("continent").layer(Layer::new(Mark::Point).transform(Transform::Count)
        .encode(Channel::Y, "count").encode(Channel::Size, "count")), &own);
    let notes: Vec<_> = d.iter().filter(|x| x.message.contains("not your table's")).collect();
    assert_eq!(notes.len(), 1, "{:?}", msgs(&d));
    assert_eq!(notes[0].kind, DiagnosticKind::Assumption);
    assert!(notes[0].message.contains("`y(count)` and `size(count)` name the `count`"), "{:?}", msgs(&d));
    assert!(!d.iter().any(|x| x.is_fatal()), "{:?}", msgs(&d));
    let d = check(&by("continent").layer(Layer::new(Mark::Point).transform(Transform::Count)
        .encode(Channel::Size, "count")), &own);
    assert!(d.iter().any(|x| x.message.contains("summarize it into a table of its own")), "{:?}", msgs(&d));
    assert!(!d.iter().any(|x| x.message.contains("rename it")), "{:?}", msgs(&d));
}

/// **A column a transform writes is a column** (spec §8, the column rules' first).
/// Named on a channel it was not drawn on, it was refused as "not in the data. Check
/// the spelling", which sent the reader after a typo that was not there. A tally's
/// measure is drawn on every aesthetic that draws a number now (the renderer writes
/// it under its own name), the wrong kind gets the channel's type refusal, a pair
/// not drawn yet is refused as not built, and a real misspelling is still one.
#[test]
fn a_written_column_is_judged_by_its_channel_not_called_missing() {
    let one = |spec: PlotSpec| {
        let d = check(&spec, &data());
        assert!(!d.iter().any(|x| x.message.contains("not in the data")), "{:?}", msgs(&d));
        d
    };
    let counted = |mark: Mark| Layer::new(mark).transform(Transform::Count);

    // Drawn: a point sized by its tally, and the count written on each bar.
    let d = one(PlotSpec::new().data("t").x("continent")
        .layer(counted(Mark::Point).encode(Channel::Size, "count")));
    assert!(!d.iter().any(|x| x.is_fatal()), "{:?}", msgs(&d));
    let d = one(PlotSpec::new().data("t").x("continent")
        .layer(counted(Mark::Bar))
        .layer(counted(Mark::Text).encode(Channel::Label, "count")));
    assert!(!d.iter().any(|x| x.is_fatal()), "count labels draw: {:?}", msgs(&d));

    // A counting text with no label is told which label prints the count, and a
    // label its summary rows do not hold is refused toward the two that they do.
    for t in [Transform::Count, Transform::Bin, Transform::Proportion, Transform::Density] {
        assert_eq!(mark_takes_transform(&Mark::Text, &t), TransformLegality::Combines, "{t:?}");
    }
    let d = check(&PlotSpec::new().data("t").x("continent").layer(counted(Mark::Text)), &data());
    assert!(d.iter().any(|x| x.message.contains("`label(count)` prints the count")), "{:?}", msgs(&d));
    assert!(!d.iter().any(|x| x.message.contains("no label to draw")), "{:?}", msgs(&d));
    let d = check(&PlotSpec::new().data("t").x("continent")
        .layer(counted(Mark::Text).encode(Channel::Label, "region")), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("which have no `region`")
        && x.message.contains("`label(count)` prints each count")
        && x.message.contains("`label(continent)` names each one")), "{:?}", msgs(&d));

    // A table holding a `count` of its own: the tally is still the one on the rows
    // the point draws, and the message says which `count` was read.
    let mut own = data();
    let tab = own.remove("t").unwrap().with_float("count", vec![7.0, 8.0, 9.0]);
    own.insert("t".to_string(), tab);
    let d = check(&PlotSpec::new().data("t").x("continent")
        .layer(counted(Mark::Point).encode(Channel::Size, "count")), &own);
    assert!(!d.iter().any(|x| x.is_fatal()), "{:?}", msgs(&d));
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Assumption
        && x.message.contains("not your table's `count` column")), "{:?}", msgs(&d));

    // The wrong kind: the channel's own type refusal.
    let d = one(PlotSpec::new().data("t").x("continent")
        .layer(counted(Mark::Bar).encode(Channel::Color, "count")));
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("names the `count` that `bar * count` writes, a continuous")
        && x.message.contains("needs a categorical (text) column")), "{:?}", msgs(&d));

    // The right kind, not drawn yet: a sunburst giving each node a color of its own.
    let d = one(PlotSpec::new().data("t").x("gdp")
        .layer(Layer::new(Mark::Zone).partition(&["continent", "region"])
            .encode(Channel::Color, "name")));
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Unsupported
        && x.message.contains("`color(name)` names the `name` that `zone * partition` writes")
        && x.message.contains("leave `color(name)` out")), "{:?}", msgs(&d));
    // Drawn: a sunburst colored by its ring, and a ring's number printed on it.
    for (mark, ch) in [(Mark::Zone, Channel::Color), (Mark::Text, Channel::Label)] {
        let d = one(PlotSpec::new().data("t").x("gdp")
            .layer(Layer::new(mark).partition(&["continent", "region"]).encode(ch, "depth")));
        assert!(!d.iter().any(|x| x.is_fatal()), "{:?}", msgs(&d));
    }

    // A node's column asked of the links between nodes.
    let net = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Edge).layout("continent", "region").encode(Channel::Color, "degree"))
        .coord(CoordSpace::Network(crate::ir::NetworkView::default()));
    let d = one(net);
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("an `edge` draws the links between the nodes")), "{:?}", msgs(&d));

    // A slot's own name is what names a node, so flow's refusal of a slot's color
    // stands aside for it, and a band, which crosses every stage, has none.
    let slots = |mark: Mark| PlotSpec::new().data("t")
        .layer(Layer::new(mark).flow(&["continent", "region"]).encode(Channel::Color, "name"));
    let d = one(slots(Mark::Zone));
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Unsupported
        && x.message.contains("names the `name` that `zone * flow` writes")), "{:?}", msgs(&d));
    assert!(!d.iter().any(|x| x.message.contains("does not combine with")), "{:?}", msgs(&d));
    let d = one(slots(Mark::Ribbon));
    assert!(d.iter().any(|x| x.message.contains("a `ribbon` draws the bands")), "{:?}", msgs(&d));

    // A misspelling is still one.
    let d = check(&PlotSpec::new().data("t").x("continent")
        .layer(counted(Mark::Point).encode(Channel::Size, "cuont")), &data());
    assert!(d.iter().any(|x| x.message.contains("`size(cuont)` refers to a column that is not \
        in the data")), "{:?}", msgs(&d));
}

/// **A cluster tree's distance axis takes its name**, `distance`, the column the
/// tree writes, like `y(count)` under a tally: said out loud, and the place to give
/// that axis its settings. Either orientation, and the tree's other aesthetics keep
/// their refusal except for the tree's own column, which is not drawn there yet.
#[test]
fn a_cluster_tree_takes_its_distance_axis_by_name() {
    let tree = || Layer::new(Mark::Path).cluster(Some("amount"), Some("nutrient"));
    for spec in [
        PlotSpec::new().data("t").x("food").y("distance").layer(tree()),
        PlotSpec::new().data("t").x("distance").y("food").layer(tree()),
    ] {
        let d = check(&spec, &cluster_table());
        assert!(!d.iter().any(|x| x.is_fatal()), "{:?}", msgs(&d));
    }
    let sideways = PlotSpec::new().data("t").x("distance").y("food").layer(tree());
    assert_eq!(synth_axis(&sideways, &sideways.layers[0], cluster_table().get("t")), Channel::X);
    // The distance axis alone binds no leaves, and says so once.
    let d = check(&PlotSpec::new().data("t").y("distance").layer(tree()), &cluster_table());
    assert!(d.iter().any(|x| x.message.contains("nothing here binds one")), "{:?}", msgs(&d));
    assert!(!d.iter().any(|x| x.kind == DiagnosticKind::Unsupported
        || x.message.contains("names ``")), "{:?}", msgs(&d));

    let colored = |field: &str| check(&PlotSpec::new().data("t").x("food")
        .layer(tree().encode(Channel::Color, field)), &cluster_table());
    // Drawn: every vertex of the strokes holds the distance it is drawn at.
    let d = colored("distance");
    assert!(!d.iter().any(|x| x.is_fatal() || x.kind == DiagnosticKind::Unsupported), "{:?}", msgs(&d));
    assert!(!d.iter().any(|x| x.message.contains("has nothing to read")), "{:?}", msgs(&d));
    let d = colored("price");
    assert!(d.iter().any(|x| x.message.contains("has nothing to read")), "{:?}", msgs(&d));
}

/// Growing a *which one?* vocabulary and warning past it are one piece of
/// work, so the test asserts them together: the limit the warning names is
/// the length of the array the renderer cycles over, never a literal.
#[test]
fn a_which_one_channel_says_when_its_vocabulary_starts_over() {
    let n = SHAPE_NAMES.len();
    let many: Vec<String> = (0..n + 3).map(|i| format!("c{i}")).collect();
    let df = DataFrame::new()
        .with_float("gdp", (0..many.len()).map(|i| i as f64).collect())
        .with_float("life", (0..many.len()).map(|i| i as f64).collect())
        .with_str("many", many);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);

    let spec = PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Point))
        .channel(Channel::Shape, "many");
    let d = check(&spec, &data);
    let hit = d.iter().find(|d| d.message.contains("starts over"))
        .expect("a column past the vocabulary should be reported");
    assert_eq!(hit.kind, DiagnosticKind::Assumption, "legal, and it draws");
    assert!(hit.message.contains(&format!("{} glyphs", n)), "{}", hit.message);
    assert!(hit.message.contains(&format!("{} categories", n + 3)), "{}", hit.message);
    // §12: a diagnostic says what to do, not only what happened.
    assert!(hit.message.contains("color(many)"), "{}", hit.message);
}

/// The silent half of the same rule. A column that fits draws every category
/// its own kind, so there is nothing to say and nothing is said.
#[test]
fn a_which_one_channel_is_silent_while_its_vocabulary_lasts() {
    let n = SHAPE_NAMES.len();
    let fits: Vec<String> = (0..n).map(|i| format!("c{i}")).collect();
    let df = DataFrame::new()
        .with_float("gdp", (0..n).map(|i| i as f64).collect())
        .with_float("life", (0..n).map(|i| i as f64).collect())
        .with_str("fits", fits);
    let mut data = HashMap::new();
    data.insert("t".to_string(), df);
    let spec = PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Point))
        .channel(Channel::Shape, "fits");
    let d = check(&spec, &data);
    assert!(!d.iter().any(|d| d.message.contains("starts over")), "{:?}", msgs(&d));
}

#[test]
fn a_set_value_is_checked_because_no_data_can_check_it() {
    let cases: Vec<(Layer, &str)> = vec![
        (Layer::new(Mark::Point).style_color("stelblue"), "steelblue"),
        (Layer::new(Mark::Point).style_opacity(3.0), "outside 0–1"),
        (Layer::new(Mark::Point).style_size(0.0), "positive"),
        // `star` joined the vocabulary, so the unknown-name case needs a
        // name that is still absent — a plausible glyph, not a typo.
        (Layer::new(Mark::Point).style_shape("hexagon"), "circle, square"),
    ];
    for (layer, expect) in cases {
        let d = check(&base().layer(layer), &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(
            d[0].message.contains(expect),
            "message should contain {expect:?}, got: {}",
            d[0].message
        );
    }
}


#[test]
fn an_r_color_name_says_which_vocabulary_is_in_force() {
    // `gray80` is an R name, not a CSS one, and it is the first thing an R
    // user reaches for. Suggesting plain "gray" would be a different shade,
    // so the message has to explain the vocabulary instead.
    let d = check(
        &base().layer(Layer::new(Mark::Point).style_color("gray80")),
        &data(),
    );
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("R color name"), "got: {}", d[0].message);
    assert!(d[0].message.contains("lightgray"), "got: {}", d[0].message);
    assert!(!d[0].message.contains("Did you mean"), "got: {}", d[0].message);

    assert_eq!(numbered_shade("gray50"), Some("gray"));
    assert_eq!(numbered_shade("gray100"), Some("gray"));
    // gog writes American spellings, but CSS defines both, so both are
    // still *accepted* from a user and the advice follows the spelling
    // they typed rather than correcting it to ours.
    assert_eq!(numbered_shade("grey80"), Some("grey"));
    // Not every trailing digit is a shade number.
    assert_eq!(numbered_shade("steelblue"), None);
    assert_eq!(numbered_shade("12345"), None);
}

#[test]
fn a_suggested_color_is_always_a_real_one() {
    // CSS has `lightsteelblue` but no `darksteelblue`. A suggestion that
    // fails when the reader retries it is worse than offering none.
    for bad in ["gray80", "gray50", "steelblue3", "orchid4", "tomato2"] {
        let d = check(
            &base().layer(Layer::new(Mark::Point).style_color(bad)),
            &data(),
        );
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        for quoted in d[0].message.split('"').skip(1).step_by(2) {
            // The message echoes the offending value back; skip that and
            // the hex example, and check what remains — the suggestions.
            if quoted == bad || quoted.starts_with('#') {
                continue;
            }
            assert!(
                is_valid_color(quoted),
                "suggested {quoted:?} for {bad:?}, which is not a color: {}",
                d[0].message
            );
        }
    }
}

#[test]
fn suggestion_lists_read_as_a_sentence() {
    let s = |v: &[&str]| or_list(&v.iter().map(|x| x.to_string()).collect::<Vec<_>>());
    assert_eq!(s(&["a"]), "a");
    assert_eq!(s(&["a", "b"]), "a or b");
    assert_eq!(s(&["a", "b", "c"]), "a, b, or c");
}


// -- palette ---------------------------------------------------------

#[test]
fn a_color_name_as_a_palette_is_refused_not_silently_ignored() {
    // Was: `palette("red")` fell through to the default palette and the
    // plot rendered in the wrong colors with nothing said.
    let spec = base()
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));
    let mut spec = spec;
    spec.palette = PaletteDef::Named("red".into());
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    // Directional: names *every* real palette and points at `style()`.
    // Driven off the list rather than spelled out, so a name added to the
    // vocabulary and left out of the message fails here — a refusal that
    // recites two thirds of the options is how a reader concludes the third
    // does not exist.
    for name in CATEGORICAL_PALETTES {
        assert!(
            d[0].message.contains(name),
            "the refusal omits `{name}`: {}",
            d[0].message
        );
    }
    assert!(d[0].message.contains("style(color = \"red\")"));
}

#[test]
fn a_misspelt_palette_color_is_refused() {
    let mut spec = base().layer(Layer::new(Mark::Point));
    spec.palette = PaletteDef::Custom(vec!["red".into(), "stelblue".into()]);
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("steelblue"));
}

#[test]
fn the_named_palettes_and_css_color_lists_still_pass() {
    // The layers bind `color` because this test is about the palette's *name*
    // being valid, and a palette with nothing to color is now its own refusal
    // (`a_palette_with_nothing_to_color_is_refused_rather_than_dropped`). Without
    // the binding this would pass or fail for the wrong reason.
    let colored = || base().layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));
    for name in ["gog", "okabe"] {
        let mut spec = colored();
        spec.palette = PaletteDef::Named(name.into());
        assert!(check(&spec, &data()).is_empty(), "{name} should be a palette");
    }
    let mut spec = colored();
    spec.palette = PaletteDef::Custom(vec!["red".into(), "#4e79a7".into()]);
    assert!(check(&spec, &data()).is_empty());
}

#[test]
fn what_renders_can_also_be_set() {
    // Regularity: if the engine can draw a feature *varying* per row, it can
    // certainly draw it fixed. Only the five style-eligible features are in
    // scope — `x`/`y`/`z` are position, and `group` partitions rather than
    // decorates, so neither is style.
    let marks = [Mark::Point, Mark::Line, Mark::Area, Mark::Bar];
    let features = [Channel::Color, Channel::Size, Channel::Shape, Channel::Opacity, Channel::Pattern];
    for m in &marks {
        for c in &features {
            let r = rule_for(m, c);
            if r.renders.is_some() {
                assert!(r.settable, "{m:?}/{c:?} renders when mapped but is not settable");
            }
        }
    }
    for c in [Channel::X, Channel::Y, Channel::Z, Channel::Group, Channel::Label, Channel::Play] {
        for m in &marks {
            assert!(!rule_for(m, &c).settable, "{m:?}/{c:?} must not be settable");
        }
    }
}

// --- facets ------------------------------------------------------------

#[test]
fn facet_by_a_category_column_is_legal() {
    let spec = base().layer(Layer::new(Mark::Point)).facet_col("continent");
    assert!(check(&spec, &data()).is_empty());
}

/// `tilt` has ends and `turn` does not, and the pair is the point of the test.
///
/// A bearing is periodic, so 5000 is 320 and there is nothing to ask about; an
/// elevation is not, so past 90 the scene hangs upside down and there is no
/// value out there to wrap to. Refusing both alike would teach a rule the
/// grammar does not have, so both halves are asserted together — the range that
/// refuses, and the range that must stay silent.
#[test]
fn a_tilt_past_the_poles_is_illegal_and_a_turn_past_one_lap_is_not() {
    let cube = |view: crate::ir::SpaceView| {
        base().z("value").coord(CoordSpace::Space(view)).layer(Layer::new(Mark::Point))
    };
    let view = |turn, tilt| crate::ir::SpaceView { turn, tilt };

    // The ends are *in* range, which is where the drag clamp already stops.
    for tilt in [0.0, 25.0, 90.0, -90.0] {
        let d = check(&cube(view(30.0, tilt)), &data());
        assert!(d.is_empty(), "tilt {tilt} should be legal: {:?}", msgs(&d));
    }

    // And past them it is refused, with the nearest view named.
    for (tilt, nearest) in [(95.0, "90"), (180.0, "90"), (-400.0, "-90")] {
        let d = check(&cube(view(30.0, tilt)), &data());
        assert_eq!(kinds(&d), [DiagnosticKind::Illegal], "tilt {tilt}");
        assert!(d[0].message.contains(&format!("space(tilt = {nearest})")),
                "tilt {tilt} should point at {nearest}: {}", d[0].message);
        // The direction has to distinguish the two angles, or a reader reads it
        // as "angles are capped" and stops writing `turn` past 360 as well.
        assert!(d[0].message.contains("space(turn = )"),
                "tilt {tilt} should offer the bearing: {}", d[0].message);
    }

    // A number that is not a number cannot be clamped to a nearest view, so the
    // suggestion is the default. `NaN.clamp()` is `NaN`, and a refusal reading
    // `space(tilt = NaN)` is not direction.
    let d = check(&cube(view(30.0, f64::NAN)), &data());
    assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
    // It still echoes what was written, which is how a reader recognizes their
    // own mistake — so `NaN` is expected *before* the fix and forbidden inside
    // it. Asserting on the whole message cannot tell those apart, so this reads
    // the suggestion clause alone.
    let fix = d[0].message.split("Use ").nth(1).unwrap_or("");
    assert!(!fix.contains("NaN"), "no NaN in the fix: {fix}");
    assert!(fix.starts_with(&format!("`space(tilt = {})`",
                                    crate::ir::SpaceView::default().tilt)),
            "should offer the default: {fix}");

    // The other half: a bearing wraps, so no lap count is ever refused.
    for turn in [0.0, 360.0, -360.0, 720.0, -720.0, 5000.0, -5000.0] {
        let d = check(&cube(view(turn, 25.0)), &data());
        assert!(d.is_empty(), "turn {turn} should be legal: {:?}", msgs(&d));
    }
}

/// A fit needs rows, and it needs them in the cell the fit actually runs in.
///
/// The whole-frame count is the easy half. The one that mattered is the split:
/// six rows pass any frame-level minimum while all three of their groups fail,
/// and before this the picture drew three two-vertex polylines that looked like
/// fitted curves beside hundred-vertex ones that were.
#[test]
fn smooth_needs_three_rows_in_every_cell_it_fits() {
    let frame = |n: usize| {
        let mut m = HashMap::new();
        let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
        m.insert("t".to_string(), DataFrame::new()
            .with_float("gdp", xs.clone())
            .with_float("life", xs));
        m
    };
    let fit = || base().layer(Layer::new(Mark::Line).transform(Transform::Smooth));

    for n in [1, 2] {
        let d = check(&fit(), &frame(n));
        assert_eq!(kinds(&d), [DiagnosticKind::Illegal], "{n} rows");
        assert!(d[0].message.contains("at least 3"), "{}", d[0].message);
        assert!(d[0].message.contains(&format!("has {n}.")),
                "should name the count it found: {}", d[0].message);
        // Unsplit, there is no split to drop, so it must not advise dropping one.
        assert!(!d[0].message.contains("Drop the split"), "{}", d[0].message);
    }
    assert!(check(&fit(), &frame(3)).is_empty(), "3 rows is a fit");

    // Split thin: enough rows in the frame, never enough in a group.
    let mut split = HashMap::new();
    split.insert("t".to_string(), DataFrame::new()
        .with_float("gdp", vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0])
        .with_float("life", vec![1.0, 2.0, 2.0, 1.0, 1.0, 3.0])
        .with_str("g", ["a", "a", "b", "b", "c", "c"].iter().map(|s| s.to_string()).collect()));
    let grouped = base().layer(
        Layer::new(Mark::Line).transform(Transform::Smooth).encode(Channel::Group, "g"));
    let d = check(&grouped, &split);
    assert_eq!(kinds(&d), [DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(d[0].message.contains("`g`"), "should name the split: {}", d[0].message);
    assert!(d[0].message.contains("Drop the split"), "{}", d[0].message);

    // Faceting splits before any of it, so a panel is a cell too.
    let faceted = base()
        .facet_col("g")
        .layer(Layer::new(Mark::Line).transform(Transform::Smooth));
    let d = check(&faceted, &split);
    assert!(d.iter().any(|x| x.message.contains("at least 3")),
            "a thin panel is a thin cell: {:?}", msgs(&d));

    // A categorical axis is the *other* check's refusal, and asking twice for one
    // mistake is what §12 forbids as plainly as saying nothing.
    let mut worded = HashMap::new();
    worded.insert("t".to_string(), DataFrame::new()
        .with_str("gdp", ["a", "b"].iter().map(|s| s.to_string()).collect())
        .with_float("life", vec![1.0, 2.0]));
    let d = check(&fit(), &worded);
    assert_eq!(d.iter().filter(|x| x.message.contains("at least 3")).count(), 0,
               "one mistake, one refusal: {:?}", msgs(&d));
}

/// The band needs five rows where its curve needs three, because below five every
/// local fit passes through its own two points and leaves no residual to measure
/// the band's width from. Between the two minimums the refusal offers the curve
/// alone, which is the nearer fix; below both it offers the rows as rows.
#[test]
fn a_band_needs_five_rows_where_its_curve_needs_three() {
    let frame = |n: usize| {
        let mut m = HashMap::new();
        let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
        m.insert("t".to_string(), DataFrame::new()
            .with_float("gdp", xs.clone())
            .with_float("life", xs));
        m
    };
    let band = || base().layer(Layer::new(Mark::Ribbon).transform(Transform::SmoothBand));
    for n in [3, 4] {
        let d = check(&band(), &frame(n));
        assert_eq!(kinds(&d), [DiagnosticKind::Illegal], "{n} rows: {:?}", msgs(&d));
        let m = &d[0].message;
        assert!(m.starts_with("gog: `smooth_band` fits a curve") && m.contains("at least 5")
            && m.contains(&format!("has {n}.")) && m.contains("`line * smooth`"), "{m}");
        assert!(check(&base().layer(Layer::new(Mark::Line).transform(Transform::Smooth)),
            &frame(n)).is_empty(), "the curve alone draws from {n}");
    }
    let d = check(&band(), &frame(2));
    assert!(d[0].message.contains("with `point` and no transform")
        && !d[0].message.contains("`line * smooth`"), "{}", d[0].message);
    assert!(check(&band(), &frame(5)).is_empty(), "5 rows is a band");
}

#[test]
fn facet_by_a_number_column_is_illegal_with_direction() {
    let spec = base().layer(Layer::new(Mark::Point)).facet_col("gdp");
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("Store `gdp` as text or categories")
        && !d[0].message.contains("factor("), "{}", d[0].message);
}

#[test]
fn facet_by_a_date_column_is_illegal_with_direction() {
    let mut m = data();
    let df = m.get_mut("t").unwrap();
    *df = df.clone().with_time("when", vec![0.0, 86400.0, 172800.0], crate::time::TimeUnit::Day);
    let spec = base().layer(Layer::new(Mark::Point)).facet_col("when");
    let d = check(&spec, &m);
    assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("period"), "{}", d[0].message);
}

#[test]
fn facet_by_a_missing_column_is_illegal() {
    let spec = base().layer(Layer::new(Mark::Point)).facet_row("contnent");
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("spelling"), "{}", d[0].message);
}

#[test]
fn the_same_column_on_both_facet_axes_is_illegal() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .facet_col("continent")
        .facet_row("continent");
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
}

/// `wrap` shapes a *line* of panels, so it needs a line. A crossing has
/// already fixed the rectangle with two columns, and a count beside it could
/// only be a second, contradicting statement of the same shape.
#[test]
fn wrapping_a_crossed_facet_is_illegal_with_direction() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .facet_col("continent")
        .facet_row("country")
        .facet_wrap(3);
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("Drop `wrap`"), "{}", d[0].message);
}

#[test]
fn wrapping_nothing_is_illegal_with_direction() {
    let spec = base().layer(Layer::new(Mark::Point)).facet_wrap(3);
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("no \nfacet") || d[0].message.contains("no facet"),
            "{}", d[0].message);
}

#[test]
fn wrapping_after_no_panels_is_illegal_with_direction() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .facet_col("continent")
        .facet_wrap(0);
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
    assert!(d[0].message.contains("wrap = 4"), "{}", d[0].message);
}

// --- free scales -------------------------------------------------------

/// The three positions draw an axis and nothing else does. `limits` reaches
/// all six magnitude channels because each has a domain; a legend is one key
/// for the whole plot, so a per-panel color scale would decode nothing.
#[test]
fn freeing_a_channel_that_is_not_a_position_is_illegal_with_direction() {
    let mut spec = base().layer(Layer::new(Mark::Point)).facet_col("continent");
    spec.channels.insert(Channel::Color,
                         ChannelDef::field("continent").with_free());
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                         && x.message.contains("Write `free` on a position")),
            "{d:?}");
    // No binding's spelling of the flag: all four bindings meet this sentence.
    assert!(d.iter().all(|x| !x.message.contains("free = TRUE")), "{d:?}");
}

#[test]
fn freeing_a_scale_with_no_panels_is_illegal_with_direction() {
    let mut spec = base().layer(Layer::new(Mark::Point));
    spec.y = spec.y.map(|c| c.with_free());
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.message.contains("has one panel")), "{d:?}");
}

/// The argument this one has to make is longer than "you cannot", because
/// the caller is imagining panels and has frames. §16: a frame replaces the
/// one before it, so a scale refitted per frame moves the axis under the data.
#[test]
fn freeing_a_scale_over_frames_rather_than_panels_is_illegal_with_direction() {
    let mut spec = base().layer(
        Layer::new(Mark::Point).encode(Channel::Play, "continent"));
    spec.y = spec.y.map(|c| c.with_free());
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.message.contains("motion would be the scale's")),
            "{d:?}");
}

/// A stated domain *is* a fixed scale, so the two together ask the axis to be
/// two things.
#[test]
fn freeing_a_scale_that_states_its_domain_is_illegal_with_direction() {
    let mut spec = base().layer(Layer::new(Mark::Point)).facet_col("continent");
    spec.y = spec.y.map(|c| ChannelDef { limits: Some([Some(0.0), Some(9e9)]), ..c }
                        .with_free());
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.message.contains("one scale per panel")), "{d:?}");
}

/// A count larger than the level list is not an error — it is a line that
/// never reaches its turn, which is the unwrapped picture.
#[test]
fn wrapping_wider_than_the_ribbon_is_legal() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .facet_col("continent")
        .facet_wrap(99);
    assert!(check(&spec, &data()).is_empty());
}

// --- play --------------------------------------------------------------

/// The whole channel, in one assertion: legal on every mark, either column
/// type, and drawn. `play` is the only row of the table that is identical
/// across all thirteen marks, because a frame is a subset of the rows and
/// there is no geometry that cannot draw a subset — so a mark missing from
/// this loop would be the Law-1 exception, not a special case.
#[test]
fn play_is_legal_and_drawn_on_every_mark_without_exception() {
    for m in &ALL_MARKS {
        let r = rule_for(m, &Channel::Play);
        assert_eq!(r.obligation, Obligation::Can, "{m:?} refuses play");
        assert_eq!(r.accepts, VarType::Either, "{m:?} narrows what play accepts");
        assert_eq!(r.renders, Some(VarType::Either), "{m:?} does not draw play");
        assert!(!r.settable, "{m:?}: a moment is not something to set");
    }
}

/// A number is welcome where `facet` refuses one, and the reason is the cost
/// function rather than the type: panels compete for page area, frames for
/// time. This is the pair of `facet_by_a_number_column_is_illegal`.
#[test]
fn play_by_a_number_column_is_legal_where_facet_refuses_one() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .channel(Channel::Play, "gdp");
    assert!(check(&spec, &data()).is_empty());
}

/// **Legal, not silent**, and the difference is the whole of what this test
/// now says. `continent` here carries no stated levels, so the sequence runs
/// in the order the rows arrive and §12 says so as an Assumption. The plot
/// still draws, which is what "legal" means and what this asserts.
#[test]
fn play_by_a_category_column_is_legal_too() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .channel(Channel::Play, "continent");
    let d = check(&spec, &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "must still draw: {:?}", msgs(&d));
}

/// One column names the panels or names the moments, never both — drawn, it
/// would leave every panel but one empty in every frame.
#[test]
fn one_column_cannot_name_both_the_panels_and_the_moments() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .channel(Channel::Play, "continent")
        .facet_col("continent");
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("name the frames")), "{d:?}");
}

/// A sequence claims an order, so a column that states none is told so.
///
/// **Warned rather than refused, and the book is the reason.** `play.qmd`
/// draws this very plot live, so a reader can watch the points jump between
/// frames and read why the jumps mean nothing. A refusal would leave the
/// chapter asserting that failure instead of showing it.
#[test]
fn a_played_column_with_no_stated_order_says_so() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .channel(Channel::Play, "continent");
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Assumption
        && x.message.contains("no stated order")), "{:?}", msgs(&d));
    // §12: an Assumption renders. A refusal here would take the book's own
    // teaching plot off the page.
    assert!(!d.iter().any(|x| x.is_fatal()), "must still draw: {:?}", msgs(&d));
}

/// **A mark's article follows its name**, in the two refusals that wrote "a" in
/// front of any mark: `interval * density` read "a interval needs two
/// boundaries", and an `area` faceted by its own `x` "a area needs at least two
/// positions".
#[test]
fn a_marks_article_follows_its_name() {
    let d = check(&PlotSpec::new().data("t").x("gdp")
        .layer(Layer::new(Mark::Interval).transform(Transform::Density)), &data());
    assert!(d.iter().any(|x| x.message.contains("and an interval needs two boundaries")),
        "{:?}", msgs(&d));
    let mut spec = PlotSpec::new().data("t").x("gdp").y("life").layer(Layer::new(Mark::Area));
    spec.facet = Some(crate::ir::FacetSpec { col: Some("gdp".into()), ..Default::default() });
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.message.contains("and an area needs at least two positions")),
        "{:?}", msgs(&d));
    assert!(d.iter().all(|x| !x.message.contains(" a area")), "{:?}", msgs(&d));
}

/// **A line cut into panels by its own `x` is refused once, for that.** On a
/// number column the facet's type refusal followed, saying to make the column
/// text, and a line cut by its own `x` holds one position per panel either way.
/// A `point`, which reads no domain, keeps the type refusal.
#[test]
fn a_line_faceted_by_its_own_x_is_refused_once() {
    let faceted = |mark: Mark| {
        let mut spec = PlotSpec::new().data("t").x("gdp").y("life").layer(Layer::new(mark));
        spec.facet = Some(crate::ir::FacetSpec { col: Some("gdp".into()), ..Default::default() });
        check(&spec, &data())
    };
    for mark in [Mark::Line, Mark::Step, Mark::Area] {
        let d = faceted(mark.clone());
        assert_eq!(d.len(), 1, "{mark:?}: {:?}", msgs(&d));
        assert!(d[0].message.contains("both cuts the plot into panels and supplies `x`"),
            "{mark:?}: {:?}", d[0]);
    }
    let d = faceted(Mark::Point);
    assert!(d.iter().any(|x| x.message.contains("splits on a number column")), "{:?}", msgs(&d));
}

/// **A network's notes about its table are said once, name it, and can be
/// followed.** Checked per layer, the self-loop refusal printed once for each
/// of a network's edges, nodes and names, and the missing-endpoint note too. The
/// self-loop refusal offered to carry the fact "as a node's own column", and a
/// node has only its `name` and its `degree`.
#[test]
fn a_networks_table_notes_are_said_once_and_name_the_table() {
    let net = || CoordSpace::Network(crate::ir::NetworkView::default());
    let three = || PlotSpec::new().data("t").coord(net())
        .layer(Layer::new(Mark::Edge).layout("a", "b"))
        .layer(Layer::new(Mark::Point).layout("a", "b"))
        .layer(Layer::new(Mark::Text).layout("a", "b").encode(Channel::Label, "name"));
    let table = |a: [&str; 3], b: [&str; 3]| HashMap::from([("t".to_string(), DataFrame::new()
        .with_str("a", a.iter().map(|s| s.to_string()).collect())
        .with_str("b", b.iter().map(|s| s.to_string()).collect()))]);
    let looped = check(&three(), &table(["u", "v", "w"], ["v", "v", "u"]));
    let said: Vec<_> = looped.iter().filter(|d| d.message.contains("connect a node to itself"))
        .collect();
    assert_eq!(said.len(), 1, "{:?}", msgs(&looped));
    assert!(said[0].message.contains("1 row(s) of `t` connect")
        && !said[0].message.contains("own column"), "{:?}", said[0]);
    let gapped = check(&three(), &table(["u", "", "w"], ["v", "w", "u"]));
    let said: Vec<_> = gapped.iter().filter(|d| d.message.contains("missing an endpoint"))
        .collect();
    assert_eq!(said.len(), 1, "{:?}", msgs(&gapped));
    assert!(said[0].message.contains("1 of 3 row(s) of `t` are missing"), "{:?}", said[0]);
}

/// **A brush on a network is refused by the network alone.** It was first told
/// that an `edge` draws one shape through many rows and to split it with
/// `group()`, which an edge refuses, and a `point * layout` was told to brush
/// the layer that draws the rows themselves, which a network has none of.
#[test]
fn a_brush_on_a_network_is_refused_by_the_network_alone() {
    let net = || CoordSpace::Network(crate::ir::NetworkView::default());
    for brush in [crate::ir::BrushDef::positions(), crate::ir::BrushDef::new("continent")] {
        for layers in [vec![Mark::Edge], vec![Mark::Point], vec![Mark::Edge, Mark::Point]] {
            let mut spec = PlotSpec::new().data("t").coord(net()).brush(brush.clone());
            for m in &layers {
                spec = spec.layer(Layer::new(m.clone()).layout("continent", "region"));
            }
            let d = check(&spec, &data());
            assert_eq!(d.len(), 1, "{layers:?}: {:?}", msgs(&d));
            assert!(d[0].message.contains("a network's positions are the layout's"), "{:?}", d[0]);
        }
    }
}

/// **A `ribbon` with `z` hears only the cube.** It was first told to add a range
/// transform, and `ribbon * range + … + z(<column>)` was then refused by the cube
/// alone. An `interval` does stand in the cube, so it is still asked for its range.
#[test]
fn a_ribbon_with_z_hears_only_the_cube() {
    let raised = |layer: Layer| check(&PlotSpec::new().data("t").x("gdp").y("life").z("value")
        .layer(layer), &data());
    for layer in [Layer::new(Mark::Ribbon), Layer::new(Mark::Ribbon).transform(Transform::Range)] {
        let d = raised(layer.clone());
        assert_eq!(d.len(), 1, "{:?}: {:?}", layer.transforms, msgs(&d));
        assert!(d[0].message.contains("a cube has no left to right"), "{:?}", d[0]);
    }
    let d = raised(Layer::new(Mark::Interval));
    assert!(d.iter().any(|x| x.message.contains("produces those extents")), "{:?}", msgs(&d));
    assert!(!check(&PlotSpec::new().data("t").x("continent").y("life").z("value")
        .layer(Layer::new(Mark::Interval).transform(Transform::Range)), &data())
        .iter().any(Diagnostic::is_fatal), "interval * range stands in the cube");
}

/// **A span mark in a space that never draws it hears only the space.** On a
/// `map()` or a `globe()` an `interval` or a `ribbon` was told first to add a
/// range transform, and `interval * range` was then refused by the space.
#[test]
fn a_span_mark_in_a_space_that_never_draws_it_hears_only_the_space() {
    for coord in [
        CoordSpace::Polar(crate::ir::PolarView::default()),
        CoordSpace::Nest,
        CoordSpace::Map(crate::ir::MapView::default()),
        CoordSpace::Globe(crate::ir::GlobeView::default()),
        CoordSpace::Network(crate::ir::NetworkView::default()),
    ] {
        for mark in [Mark::Interval, Mark::Ribbon] {
            let spec = PlotSpec::new().data("t").x("gdp").y("life").coord(coord.clone())
                .layer(Layer::new(mark.clone()));
            let d = check(&spec, &data());
            assert!(d.iter().any(Diagnostic::is_fatal), "{coord:?} {mark:?} drew");
            let drawn_there = mark_draws_in_space(&mark, space_of(&spec));
            let asked = d.iter().any(|x| x.message.contains("produces those extents"));
            assert_eq!(asked, drawn_there, "{coord:?} {mark:?}: {:?}", msgs(&d));
        }
    }
}

/// **Three globe refusals say what the globe chapter says.** An axis label: a
/// sphere has no straight side, where "no edge" contradicted the disk's edge. A
/// `z` with no spike: the marks drawn on the sphere, where "the surface marks"
/// read as the `surface` mark. A `tilt` past a pole: `turn` is a longitude,
/// where "a bearing" was the chapter's one use of the word.
#[test]
fn the_globe_refusals_use_the_globe_chapters_words() {
    let globe = || PlotSpec::new().data("t").x("gdp").y("life")
        .coord(CoordSpace::Globe(crate::ir::GlobeView::default()))
        .layer(Layer::new(Mark::Point));
    let said = |spec: PlotSpec| msgs(&check(&spec, &data())).join("\n");
    let labeled = said(globe().x_label("GDP"));
    assert!(labeled.contains("no straight side to write one along")
        && !labeled.contains("no edge"), "{labeled}");
    let raised = said(globe().z("value"));
    assert!(raised.contains("to keep the marks drawn on the sphere")
        && !raised.contains("surface marks"), "{raised}");
    let tilted = said(PlotSpec::new().data("t").x("gdp").y("life")
        .coord(CoordSpace::Globe(crate::ir::GlobeView { tilt: 100.0, ..Default::default() }))
        .layer(Layer::new(Mark::Point)));
    assert!(tilted.contains("a longitude wraps and a latitude does not")
        && !tilted.contains("bearing"), "{tilted}");
}

/// **The cube's log-`z` refusal speaks only in the cube.** Every other space
/// refuses a bound `z` in its own words, and a globe refuses its `scale`; this
/// message printed first in each, about an axis the plot was never going to
/// have.
#[test]
fn a_log_z_is_refused_by_the_cube_only_in_the_cube() {
    let log_z = || PlotSpec::new().data("t").x("gdp").y("life")
        .z_scaled("value", ScaleType::Log).layer(Layer::new(Mark::Point));
    for spec in [log_z(), log_z().coord(CoordSpace::Space(crate::ir::SpaceView::default()))] {
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.message.contains("log `z`-axis")), "{:?}", msgs(&d));
    }
    for coord in [
        CoordSpace::Polar(crate::ir::PolarView::default()),
        CoordSpace::Nest,
        CoordSpace::Map(crate::ir::MapView::default()),
        CoordSpace::Globe(crate::ir::GlobeView::default()),
    ] {
        let d = check(&log_z().coord(coord.clone()), &data());
        assert!(d.iter().any(Diagnostic::is_fatal), "{coord:?} drew: {:?}", msgs(&d));
        assert!(d.iter().all(|x| !x.message.contains("log `z`-axis")), "{coord:?}: {:?}", msgs(&d));
    }
    // Under `layout` the space is the network's, which refuses `z` itself.
    let d = check(&PlotSpec::new().data("t").z_scaled("value", ScaleType::Log)
        .coord(CoordSpace::Network(crate::ir::NetworkView::default()))
        .layer(Layer::new(Mark::Point).layout("continent", "region")), &data());
    assert!(d.iter().any(Diagnostic::is_fatal)
        && d.iter().all(|x| !x.message.contains("log `z`-axis")), "{:?}", msgs(&d));
}

/// **A direction to declare an order names how in every language.** Both
/// messages that ask for one said "set the column's factor levels", R's word,
/// printed as well to Python, Julia and JavaScript, which declare it with
/// `ordered()`.
#[test]
fn asking_for_a_declared_order_names_all_four_languages() {
    let played = check(&base().layer(Layer::new(Mark::Point))
        .channel(Channel::Play, "continent"), &data());
    let mut sorted = Vec::new();
    check_order(&mut sorted, &PlotSpec::new().data("t").y("gdp").order_desc("gdp"), &data());
    for said in [msgs(&played), msgs(&sorted)] {
        let said = said.join("\n");
        assert!(said.contains("declare the column's levels") || said.contains("Declare the column's levels"),
            "{said}");
        assert!(said.contains("a factor in R, `ordered()` in Python, Julia and JavaScript")
            && !said.contains("factor levels"), "{said}");
    }
}

/// **The test is a stated order, not a column type**, which is the trap this
/// pins. An ordered category is exactly what a sequence wants, so a factor
/// must stay silent where a bare text column does not.
#[test]
fn a_played_factor_says_nothing_because_its_order_was_chosen() {
    let df = DataFrame::new()
        .with_float("gdp", vec![1.0, 2.0, 3.0])
        .with_float("life", vec![4.0, 5.0, 6.0])
        .with_levels(
            "continent",
            vec!["Asia".into(), "Europe".into(), "Africa".into()],
            vec!["Africa".into(), "Asia".into(), "Europe".into()],
        );
    let mut d2 = HashMap::new();
    d2.insert("t".to_string(), df);
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .channel(Channel::Play, "continent");
    let d = check(&spec, &d2);
    assert!(!d.iter().any(|x| x.message.contains("no stated order")),
        "a stated order must not be second-guessed: {:?}", msgs(&d));
}

/// And a number needs no levels: its order is in the values.
#[test]
fn a_played_number_says_nothing_about_order() {
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .channel(Channel::Play, "gdp");
    let d = check(&spec, &data());
    assert!(!d.iter().any(|x| x.message.contains("no stated order")), "{:?}", msgs(&d));
}

/// A long sequence is legal — Law 8 does not forbid the ugly — but the loop
/// length is a default the caller did not choose, so §12 says it out loud.
#[test]
fn a_long_sequence_says_how_long_it_will_loop_for() {
    let years: Vec<f64> = (0..40).map(|i| 1950.0 + i as f64).collect();
    let n = years.len();
    let df = DataFrame::new()
        .with_float("gdp", vec![1.0; n])
        .with_float("life", vec![2.0; n])
        .with_float("year", years);
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);
    let spec = base().layer(Layer::new(Mark::Point)).channel(Channel::Play, "year");
    let d = check(&spec, &m);
    assert_eq!(kinds(&d), [DiagnosticKind::Assumption], "it draws, and it remarks");
    assert!(d[0].message.contains("40 frames"), "{}", d[0].message);
    assert!(d[0].message.contains("speed"), "and gives the direction: {}", d[0].message);
}

/// And the pace it names has to be faster than the one already written.
///
/// It was the constant 4 for the life of the diagnostic, so a sequence
/// already running at 6 was told to "run it faster with `speed = 4`" — the
/// message asking for half the pace it was reading, under a plot whose
/// author had taken its advice once. Found while writing the play chapter,
/// where the sentence on the page sets `speed = 3` and the note under it
/// named a number that had nothing to do with it.
#[test]
fn the_pace_it_suggests_is_never_slower_than_the_one_already_written() {
    let years: Vec<f64> = (0..170).map(|i| i as f64).collect();
    let n = years.len();
    let df = DataFrame::new()
        .with_float("gdp", vec![1.0; n])
        .with_float("life", vec![2.0; n])
        .with_float("year", years);
    let mut m = HashMap::new();
    m.insert("t".to_string(), df);

    let at = |speed: f64| {
        let spec = base().layer(
            Layer::new(Mark::Point)
                .encode_def(Channel::Play, ChannelDef::field("year").with_speed(speed)),
        );
        check(&spec, &m)
            .into_iter()
            .find(|x| x.kind == DiagnosticKind::Assumption)
            .expect("a long sequence still remarks")
            .message
    };

    // Slower than it needs to be, so it says how much faster: 170 frames
    // over the 30 that pass without remark is six.
    let slow = at(3.0);
    assert!(slow.contains("speed = 6"), "{slow}");

    // At that pace or past it, no pace is offered, because the only one
    // that would help is the one already written. What is left is the other
    // way out, and §12 requires that something be left.
    for already_fast in [6.0, 8.0] {
        let msg = at(already_fast);
        assert!(!msg.contains("speed ="), "no backwards direction: {msg}");
        assert!(msg.contains("coarser column"), "still says what to do: {msg}");
    }
}

/// A short one says nothing: §12's rule is that an unambiguous default is
/// used silently, and a decade at the normal pace is what anyone writing
/// `play(year)` already expects.
#[test]
fn a_short_sequence_is_drawn_without_remark() {
    let spec = base().layer(Layer::new(Mark::Point)).channel(Channel::Play, "gdp");
    assert!(check(&spec, &data()).is_empty());
}

/// `speed` is the narrowest binding parameter: `limits` needs a domain,
/// `tick_count` an axis, and this a duration — which only `play` has.
#[test]
fn speed_on_a_channel_that_does_not_spend_time_is_illegal() {
    let spec = base().layer(
        Layer::new(Mark::Point)
            .encode_def(Channel::Color, ChannelDef::field("continent").with_speed(2.0)),
    );
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("no pace to set")), "{d:?}");
}

#[test]
fn a_speed_of_zero_or_less_is_not_a_pace() {
    for bad in [0.0, -2.0] {
        let spec = base().layer(
            Layer::new(Mark::Point)
                .encode_def(Channel::Play, ChannelDef::field("gdp").with_speed(bad)),
        );
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal),
            "speed = {bad} should be refused: {d:?}");
    }
}

#[test]
fn a_positive_speed_on_play_is_accepted() {
    let spec = base().layer(
        Layer::new(Mark::Point)
            .encode_def(Channel::Play, ChannelDef::field("gdp").with_speed(2.0)),
    );
    assert!(check(&spec, &data()).is_empty());
}

/// The reader-found defect, and the one this whole section nearly shipped:
/// `line + x(year) + play(year)` drew axes, a legend and a frame strip
/// around an **empty panel**, exit 0. Cut the plot by the column that
/// supplies `x` and every frame holds one position, so a function has
/// nothing to read between.
#[test]
fn a_domain_reading_mark_cannot_be_cut_by_the_column_that_supplies_its_x() {
    for mark in [Mark::Line, Mark::Step, Mark::Area, Mark::Ribbon] {
        let spec = PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(mark.clone()).encode(Channel::Play, "gdp"));
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("reads a function along `x`")),
            "{mark:?} should refuse: {d:?}");
    }
}

/// The same defect through the older door. `facet` has had it since
/// faceting shipped, and catching only the door `play` opened would be the
/// per-feature exception Law 1 exists to catch — the two are one partition
/// asked twice (§11).
#[test]
fn the_same_is_true_of_a_facet_on_the_column_that_supplies_x() {
    for spec in [
        base().layer(Layer::new(Mark::Line)).facet_col("gdp"),
        base().layer(Layer::new(Mark::Line)).facet_row("gdp"),
    ] {
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("reads a function along `x`")), "{d:?}");
    }
}

/// A mark that places each row on its own has no domain to lose, so the same
/// sentence is legal — Law 8 forbids nothing that can be drawn. `path` is
/// here too: it connects in the *table's* order and never promised a domain.
#[test]
fn a_mark_that_needs_no_domain_may_be_cut_by_its_own_x() {
    for mark in [Mark::Point, Mark::Bar, Mark::Path] {
        let spec = PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(mark.clone()).encode(Channel::Play, "gdp"));
        let d = check(&spec, &data());
        assert!(!d.iter().any(|x| x.message.contains("reads a function along `x`")),
            "{mark:?} has no domain to lose: {d:?}");
    }
}

/// And the ordinary sentence stays legal: it is the *same column* on both
/// that is refused, not `play` beside a position.
#[test]
fn play_on_a_different_column_from_x_is_untouched() {
    let spec = base()
        .layer(Layer::new(Mark::Line))
        .channel(Channel::Play, "continent");
    let d = check(&spec, &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "must still draw: {:?}", msgs(&d));
}

/// `play` measures nothing, so it runs along no scale — the same answer
/// `shape` and `group` get, and it needed no new code to be true.
#[test]
fn play_reads_no_scale() {
    assert!(!reads_a_scale(&Channel::Play));
    let spec = base().layer(
        Layer::new(Mark::Point)
            .encode_def(Channel::Play, ChannelDef::field("gdp").with_scale(ScaleType::Log)),
    );
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal), "{d:?}");
}

#[test]
fn a_layer_whose_table_lacks_the_facet_column_is_an_assumption() {
    // A second table with no `continent` column: its layer is drawn in
    // every panel, and that chosen default is said out loud, non-fatally.
    let mut m = data();
    m.insert(
        "ref".to_string(),
        DataFrame::new()
            .with_float("gdp", vec![1.0, 2.0])
            .with_float("life", vec![5.0, 5.0]),
    );
    let mut line = Layer::new(Mark::Line);
    line.data = Some("ref".to_string());
    let spec = base()
        .layer(Layer::new(Mark::Point))
        .layer(line)
        .facet_col("continent");
    let d = check(&spec, &m);
    assert_eq!(kinds(&d), [DiagnosticKind::Assumption]);
    assert!(d[0].message.contains("every panel"), "{}", d[0].message);
}

/// Every atom a refusal **names** must be one a reader can actually write.
///
/// `book/check_refusals.R` proves a documented refusal still refuses. Nothing
/// proved anything about the sentence it points *at* — and a refusal's direction
/// is the half of §12 that does the teaching. A direction naming a word the
/// kernel does not have is the `total`-transform prose bug
/// (`book/check_vocabulary.R`) one layer down: in the engine's own text, where
/// no book check can reach it, because a message is a Rust string and not a
/// chunk.
///
/// **What this cannot do, stated so nobody reads it as more than it is.** It
/// checks a direction's *vocabulary*, never its *quality*. Whether the plot a
/// message names is better than the plot it declines is a judgment no test
/// makes: `surface * bin` spent months directing readers to `bar * bin +
/// space()`, whose walls occlude the relief the sheet exists to show, with every
/// check in this file green (spec §15). That gap is open by construction.
#[test]
fn every_atom_named_in_a_refusal_is_one_the_kernel_has() {
    // The kernel's vocabulary, generated from the four lists the book's grids
    // and `rules_matrix` are generated from — so a new atom joins this check by
    // *existing* rather than by being remembered here.
    let mut known: std::collections::HashSet<String> = ALL_MARKS.iter()
        .map(|m| mark_name(m).to_string())
        .chain(ALL_CHANNELS.iter().map(|c| channel_name(c).to_string()))
        .chain(USER_TRANSFORMS.iter().map(|t| transform_name(t).to_string()))
        .chain(ALL_SETTINGS.iter().map(|s| setting_name(*s).to_string()))
        .collect();
    // The plot-level words. These are the **binding** surface rather than the
    // kernel — coordinate spaces, the table, the guides, the page — and the
    // engine has no list of them to generate from, since it receives them as
    // fields on `PlotSpec` rather than as atoms. Hand-written on purpose and
    // deliberately short: if it grows past a glance it wants generating, which
    // is `CONTRIBUTING.md`'s rule about a hand-written list beside a generated
    // one, and this is the list that would lose.
    known.extend(["space", "polar", "nest", "flat", "globe", "map", "network",
                  "data", "facet", "style", "palette", "theme", "title",
                  "x_label", "y_label", "z_label", "render_svg"]
                 .iter().map(|s| s.to_string()));

    // Every word used as an **atom** in a fragment: at paren depth 0, so the
    // arguments are skipped. A word inside `(...)` is a column name, a value or
    // a parameter (`x(life)`, `bin(width = 5)`, `style(color = "red")`), none of
    // which this check has any business ruling on.
    let atoms_in = |frag: &str| -> Vec<String> {
        let (b, mut out, mut depth, mut i) = (frag.as_bytes(), Vec::new(), 0i32, 0usize);
        while i < b.len() {
            match b[i] {
                b'(' => { depth += 1; i += 1 }
                b')' => { depth -= 1; i += 1 }
                c if (c.is_ascii_lowercase() || c == b'_') && depth == 0 => {
                    let s = i;
                    while i < b.len()
                        && (b[i].is_ascii_lowercase() || b[i] == b'_' || b[i].is_ascii_digit())
                    { i += 1 }
                    out.push(frag[s..i].to_string());
                }
                _ => i += 1,
            }
        }
        out
    };

    // A *direction* is a sentence, and a sentence has an operator in it. That
    // filter is what separates `bar * bin + x(<a>)` from the bare references a
    // message also backticks (`count`, `hex`, `GOG_STRICT=0`), which name a
    // thing rather than recommend a plot.
    let sentences = |msg: &str| -> Vec<String> {
        msg.split('`').skip(1).step_by(2)
            .filter(|f| f.contains(" + ") || f.contains(" * "))
            .map(|f| f.to_string())
            .collect()
    };

    let d = data();
    let mut checked = 0usize;
    let mut bad: Vec<String> = Vec::new();
    let note = |msg: &str, ctx: &str, bad: &mut Vec<String>, checked: &mut usize| {
        for frag in sentences(msg) {
            *checked += 1;
            for word in atoms_in(&frag) {
                if !known.contains(&word) {
                    bad.push(format!("{ctx}: `{frag}` names `{word}`, which is not an atom"));
                }
            }
        }
    };

    // The two enumerable surfaces: every mark against every channel, and every
    // mark against every transform. Both numeric and categorical columns, since
    // which one a message refuses on decides which message it writes.
    for m in &ALL_MARKS {
        for c in &ALL_CHANNELS {
            for col in ["gdp", "continent"] {
                let spec = PlotSpec::new().data("t").x("gdp").y("life")
                    .layer(Layer::new(m.clone()).encode(c.clone(), col));
                let ctx = format!("{m:?}/{c:?}/{col}");
                for diag in check(&spec, &d) {
                    note(&diag.message, &ctx, &mut bad, &mut checked);
                }
            }
        }
        for t in USER_TRANSFORMS {
            let spec = PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(m.clone()).transform(t.clone()));
            let ctx = format!("{m:?}*{}", transform_name(&t));
            for diag in check(&spec, &d) {
                note(&diag.message, &ctx, &mut bad, &mut checked);
            }
        }
    }

    // 526 directed sentences at the time of writing, from ~450 refused pairs.
    // The floor is a scan-broke guard, not a target.
    assert!(checked > 400, "only {checked} directed sentences reached — the scan broke");
    assert!(bad.is_empty(), "{} bad direction(s) out of {checked}:\n{}",
            bad.len(), bad.join("\n"));
}

#[test]
fn every_mark_channel_pair_has_a_rule() {
    // Guards the Law of No Exceptions: no (mark, channel) pair may be
    // undefined. If a mark or channel is added, this forces a decision.
    // One shared list with the generated grid and `rules_matrix`, so the
    // book's grid, this guard, and the JSON dump cannot disagree.
    for m in &ALL_MARKS {
        for c in &ALL_CHANNELS {
            let r = rule_for(m, c);
            // A rule may not promise to render a type it does not accept.
            if let Some(renders) = r.renders {
                assert!(
                    r.accepts.accepts(renders) || r.accepts == VarType::Either,
                    "{:?}/{:?}: renders {:?} but only accepts {:?}",
                    m, c, renders, r.accepts
                );
            }
        }
    }
}

#[test]
fn rules_matrix_is_the_whole_table_and_invents_nothing() {
    // The `gog-cli --rules` dump the book's grid is generated from. It must
    // be `rule_for` iterated in full — every mark × every channel, no more,
    // no less — and speak only the wire vocabulary the book maps to glyphs.
    let m = rules_matrix();
    assert_eq!(m.marks.len(), ALL_MARKS.len());
    assert_eq!(m.channels.len(), ALL_CHANNELS.len());
    assert_eq!(m.cells.len(), ALL_MARKS.len() * ALL_CHANNELS.len());
    for cell in &m.cells {
        assert!(
            matches!(cell.obligation, "must" | "can" | "cannot"),
            "unknown obligation {:?}", cell.obligation
        );
        assert!(matches!(cell.accepts, "continuous" | "discrete" | "either"));
        if let Some(r) = cell.renders {
            assert!(matches!(r, "continuous" | "discrete" | "either"));
        }
    }
    // The four corners the book's five-state glyph mapping is pinned to:
    // a required position, a set-only stroke width, a reserved (grammar-legal
    // but undrawn) cell, and an absent feature. If any of these flips, the
    // grid's legend would mislabel a column of cells.
    let cell = |mk: &str, ch: &str| {
        m.cells.iter().find(|c| c.mark == mk && c.channel == ch).unwrap()
    };
    assert_eq!(cell("point", "x").obligation, "must"); // ● required
    let sz = cell("line", "size");
    assert_eq!(sz.obligation, "cannot"); // cannot map…
    assert!(sz.settable); // …but ○ set-only
    // The reserved cell was `bar`/`z` until the 3-D histogram drew it (spec
    // §5/§15), which is this guard working rather than a fixture going stale:
    // the glyph mapping had to be re-pinned to a cell that is still undrawn.
    // `line`/`z` is the one with the sharpest reason behind it — a line reads a
    // domain left to right and the cube has no left to right, so the space curve
    // is `path`'s — which makes it the right example for "grammar-legal, not
    // drawn" rather than merely the next one along.
    //
    // `line`/`z` is ruled out now, and dumped as `cannot` (spec §15), and
    // `text`/`z` was the reserved example until labels drew in the cube
    // (2026-10-09). So it is `rule`/`z`, which waits on occlusion: the grammar
    // allows it and this engine does not draw it.
    assert_eq!(cell("rule", "z").obligation, "can"); // grammar-legal…
    assert!(cell("rule", "z").renders.is_none()); // …but ◌ not drawn
    assert!(cell("text", "z").renders.is_some(), "text should draw in the cube");
    for ch in ["size", "opacity"] {
        assert!(cell("text", ch).renders.is_some(), "text should draw a mapped {ch}");
    }
    for mk in ["line", "step", "area", "ribbon"] {
        assert_eq!(cell(mk, "z").obligation, "cannot", "{mk}: ruled out, so —");
    }
    // And the cell that moved is pinned from the other side, so a regression
    // that quietly un-draws it fails here too.
    assert!(cell("bar", "z").renders.is_some(), "bar should draw in the cube");
    let pat = cell("point", "pattern");
    assert_eq!(pat.obligation, "cannot"); // — absent
    assert!(!pat.settable);
}

#[test]
fn settings_grid_agrees_with_the_style_checks() {
    // `mark_takes_setting` is the one source the generated Mark × Setting grid
    // reads. For the five style-only settings (the channel-backed five come
    // straight from `rule_for(_).settable`, which `check_style` gates on
    // identically) it is written separately from the `check_*` functions that
    // enforce them, so the two are pinned together here: the grid can never
    // promise a setting the engine refuses, nor hide one it allows.
    let border = || StyleSpec { border_color: Some("black".into()), border_size: Some(1.0), ..Default::default() };
    let caps   = || StyleSpec { caps: Some(true), ..Default::default() };
    let center = || StyleSpec { center: Some(true), ..Default::default() };
    let nudge  = || StyleSpec { nudge: Some("up".into()), ..Default::default() };
    let angle  = || StyleSpec { angle: Some(90.0), ..Default::default() };
    type Check = fn(&mut Vec<Diagnostic>, &Mark, &StyleSpec);
    // A plain layer, with no transform: the grid speaks for the mark itself.
    let plain_border: Check = |out, m, s| check_border(out, m, s, &[]);
    let cases: [(Setting, fn() -> StyleSpec, Check); 6] = [
        (Setting::BorderColor, border, plain_border),
        (Setting::BorderSize,  border, plain_border),
        (Setting::Caps,        caps,   check_caps),
        (Setting::Center,      center, check_center),
        (Setting::Nudge,       nudge,  check_nudge),
        (Setting::Angle,       angle,  check_angle),
    ];
    for m in &ALL_MARKS {
        if !is_drawable(m) { continue; } // path/surface are refused before these run
        for &(setting, style, check) in &cases {
            let mut out = Vec::new();
            check(&mut out, m, &style());
            let engine_allows = out.is_empty();
            assert_eq!(
                mark_takes_setting(m, setting), engine_allows,
                "{:?}/{}: table says {}, engine allows {}",
                m, setting_name(setting), mark_takes_setting(m, setting), engine_allows
            );
        }
    }
}

#[test]
fn settings_matrix_covers_every_setting_and_mark() {
    // The `settings` block of the dump the Setting grid is generated from.
    let mtx = rules_matrix();
    assert_eq!(mtx.settings.len(), ALL_SETTINGS.len());
    assert_eq!(mtx.setting_cells.len(), ALL_SETTINGS.len() * ALL_MARKS.len());
    let sc = |s: &str, m: &str| {
        mtx.setting_cells.iter().find(|c| c.setting == s && c.mark == m).unwrap().settable
    };
    // Corners: color on every mark; size set-only on a line but pinned on a bar;
    // border on the three fills only; caps on interval alone; nudge on text alone.
    assert!(sc("color", "point") && sc("color", "ribbon"));
    assert!(sc("size", "line") && !sc("size", "bar"));
    assert!(sc("border_color", "bar") && sc("border_color", "box") && sc("border_color", "point"));
    assert!(!sc("border_color", "line") && !sc("border_color", "area"));
    assert!(sc("caps", "interval") && !sc("caps", "point"));
    assert!(sc("nudge", "text") && !sc("nudge", "line"));
    assert!(sc("angle", "text") && !sc("angle", "point") && !sc("angle", "bar"));
}

/// `style(angle = )` turns a label, so it is `text`'s alone, and a value that is
/// not a number of degrees is refused rather than drawn as upright.
#[test]
fn angle_is_a_text_setting_and_a_number_of_degrees() {
    let with = |mark: Mark, a: f64| {
        let mut layer = Layer::new(mark);
        if layer.mark == Mark::Text { layer = layer.encode(Channel::Label, "continent"); }
        layer.style.angle = Some(a);
        check(&base().layer(layer), &data())
    };
    let d = with(Mark::Text, 90.0);
    assert!(d.is_empty(), "{:?}", msgs(&d));
    let d = with(Mark::Point, 90.0);
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("`style(angle = )` is a `text` setting")
        && x.message.contains("a `point` has no label to turn")), "{:?}", msgs(&d));
    let d = with(Mark::Text, f64::NAN);
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("needs a number of degrees")), "{:?}", msgs(&d));
}

#[test]
fn dodge_needs_a_width_bearing_mark_and_refuses_the_rest_with_direction() {
    // A collision modifier combines with the marks whose geometry it was
    // defined for (Law 1 read correctly). `dodge` subdivides a *width*, and on a
    // point it sets the points apart across their slot, the beeswarm; a
    // continuous mark is refused, with the offset that *does* fit named in the
    // message.
    let split = |m: Mark, x: &str| {
        PlotSpec::new().data("t").x(x).y("life")
            .layer(Layer::new(m).transform(Transform::Dodge).encode(Channel::Color, "continent"))
    };

    // A point swarms: `point * dodge` over a category is the beeswarm, drawn,
    // not refused toward `jitter` as it was before the author's ruling.
    let d = check(&split(Mark::Point, "continent"), &data());
    assert!(
        !d.iter().any(|x| x.is_fatal()),
        "point * dodge over a category should draw the beeswarm: {:?}",
        d.iter().map(|x| &x.message).collect::<Vec<_>>()
    );

    // A filled area is offset by accumulating, so its refusal points at `stack`.
    // A line refuses `stack` too, so its refusal says why a thin stroke needs
    // neither, and points at `area * stack` for piling the groups: it had shared
    // the area's `stack`.
    let d = check(&split(Mark::Area, "gdp"), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("accumulating* (`stack`)")), "{:?}", msgs(&d));
    let d = check(&split(Mark::Line, "gdp"), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("cross rather than cover each other")
        && x.message.contains("`area * stack`")
        && !x.message.contains("(`stack`)")), "{:?}", msgs(&d));
    for m in [Mark::Line, Mark::Area] {
        let stacked = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(m.clone()).transform(Transform::Stack)
                .encode(Channel::Color, "continent")), &data());
        assert_eq!(stacked.iter().any(Diagnostic::is_fatal), m == Mark::Line,
            "{m:?} * stack: {:?}", msgs(&stacked));
    }

    // A filled band has no width to subdivide, and no baseline to stack — its
    // honest overlap answer is transparency, so the refusal points at opacity.
    let d = check(&split(Mark::Ribbon, "gdp"), &data());
    assert!(
        d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("opacity")),
        "ribbon * dodge should be refused toward opacity: {:?}",
        d.iter().map(|x| &x.message).collect::<Vec<_>>()
    );
}

/// `point * dodge` — the beeswarm (spec §5). Legal with no split, because a
/// point's own rows are what collide, and data-aware about its axes: the swarm
/// moves along a category and keeps the other axis true, so exactly one of the
/// two positions must be a category.
#[test]
fn a_swarm_needs_exactly_one_category_axis() {
    let swarm = |x: &str, y: &str| PlotSpec::new().data("t").x(x).y(y)
        .layer(Layer::new(Mark::Point).transform(Transform::Dodge));
    let fatal = |spec: &PlotSpec| check(spec, &data()).into_iter()
        .filter(Diagnostic::is_fatal).map(|d| d.message).collect::<Vec<_>>();
    // One category, on either axis: drawn, with no split asked for.
    for (x, y) in [("continent", "life"), ("life", "continent")] {
        assert!(fatal(&swarm(x, y)).is_empty(), "x({x}) + y({y}): {:?}", fatal(&swarm(x, y)));
    }
    // Two numbers: no slot to swarm in, and the direction is opacity.
    let d = fatal(&swarm("gdp", "life"));
    assert!(d.iter().any(|m| m.contains("no slot to swarm in") && m.contains("opacity")), "{d:?}");
    // Two categories: no measurement to keep true.
    let d = fatal(&swarm("continent", "region"));
    assert!(d.iter().any(|m| m.contains("both `x` and `y` are categories")), "{d:?}");
}

/// Where the swarm is not drawn yet it says so, as Unsupported, rather than
/// drawing the points unmoved: accepted and dropped is §12's forbidden case.
#[test]
fn a_swarm_is_unsupported_in_the_disc_and_the_cube() {
    let layer = || Layer::new(Mark::Point).transform(Transform::Dodge);
    let polar = PlotSpec::new().data("t").x("continent").y("life")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default())).layer(layer());
    let cube = PlotSpec::new().data("t").x("continent").y("life").z("value")
        .coord(CoordSpace::Space(crate::ir::SpaceView::default())).layer(layer());
    for (name, spec) in [("polar", polar), ("space", cube)] {
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Unsupported
                                && x.message.contains("point * dodge")),
                "{name}: {:?}", d.iter().map(|x| &x.message).collect::<Vec<_>>());
    }
}

/// A swarm is placed by its neighbors, so a pass holding half of them would
/// be a different swarm: under a brush it is drawn whole, as `jitter` is.
#[test]
fn a_swarm_does_not_answer_a_selection() {
    let swarm = Layer::new(Mark::Point).transform(Transform::Dodge);
    assert!(selection_draws(&swarm).is_some_and(|why| why.contains("swarmed")));
    assert!(!layer_answers_selection(&swarm));
    // The bars `dodge` has always served are drawn: a bar is measured on the
    // whole share and each pass draws its side of it (2026-10-09), where a
    // swarm's points would themselves move.
    assert!(selection_draws(&Layer::new(Mark::Bar).transform(Transform::Dodge)).is_none());
    assert!(layer_answers_selection(&Layer::new(Mark::Bar).transform(Transform::Dodge)));
}

/// **Two reductions in one layer are refused, on every mark and for every pair.**
///
/// Filed 2026-07-30 as a silent drop and fixed the same day. `bar * sum * mean`
/// used to render byte-identical to `bar * sum`, and `bar * mean * sum`
/// byte-identical to `bar * mean`: `transform::reduces_column` is a `find_map`, so
/// whichever was written first won and the other vanished with no warning and no
/// refusal. That is the silent drop §12 forbids, and it also made false the rule
/// the book states in three places — that the order of transforms never matters.
///
/// Every pair on every mark is named rather than a sample, because the defect was
/// uniform across all of them and a sample is what missed it the first time: the
/// sweep that reported "no counterexamples" had used a *continuous* `x`, where
/// nothing reduces per category and both orders therefore agreed. The fixture
/// here is categorical for that reason.
#[test]
fn two_reductions_in_one_layer_are_refused() {
    let family = [Transform::Sum, Transform::Mean, Transform::Median,
                  Transform::Max, Transform::Min];
    let marks = [Mark::Bar, Mark::Point, Mark::Line, Mark::Area];
    let cat = || PlotSpec::new().data("t").x("continent").y("life");

    let mut pairs = 0;
    for m in &marks {
        for (i, a) in family.iter().enumerate() {
            for b in family.iter().skip(i + 1) {
                for (first, second) in [(a, b), (b, a)] {
                    let d = check(
                        &cat().layer(Layer::new(m.clone())
                            .transform(first.clone())
                            .transform(second.clone())),
                        &data(),
                    );
                    assert!(
                        d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                            && x.message.contains("measures each cell twice")),
                        "{m:?} * {first:?} * {second:?} must refuse, not silently keep one: {:?}",
                        d.iter().map(|x| &x.message).collect::<Vec<_>>()
                    );
                    pairs += 1;
                }
            }
        }
    }
    assert_eq!(pairs, 80, "10 pairs, both orders, four marks");

    // And one reduction on its own is untouched — the fix must not widen into the
    // ordinary summary, which is most of what the aggregation family is for.
    for m in &marks {
        for t in &family {
            let d = check(&cat().layer(Layer::new(m.clone()).transform(t.clone())), &data());
            assert!(
                !d.iter().any(|x| x.message.contains("measures each cell twice")),
                "{m:?} * {t:?} alone must stay legal: {:?}",
                d.iter().map(|x| &x.message).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn dodge_needs_a_split_but_then_composes_cleanly() {
    let cat = || PlotSpec::new().data("t").x("continent").y("life");

    // No color/group: nothing to set beside anything — refused toward adding it.
    let d = check(&cat().layer(Layer::new(Mark::Bar).transform(Transform::Dodge)), &data());
    assert!(
        d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("color")),
        "bar * dodge with no split should ask for color(): {:?}",
        d.iter().map(|x| &x.message).collect::<Vec<_>>()
    );

    // With a split, dodge composes with a box's *own* summary and stays silent —
    // it is a position modifier, orthogonal to the statistic `box` carries.
    let d = check(
        &cat().layer(Layer::new(Mark::Box).transform(Transform::Dodge).encode(Channel::Color, "continent")),
        &data(),
    );
    assert!(d.is_empty(), "box * dodge + color should be legal: {:?}",
        d.iter().map(|x| &x.message).collect::<Vec<_>>());

    // dodge does not satisfy interval's range requirement — a separate law.
    let d = check(
        &cat().layer(Layer::new(Mark::Interval).transform(Transform::Dodge).encode(Channel::Color, "continent")),
        &data(),
    );
    assert!(
        d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("range")),
        "interval * dodge without a range transform still needs range"
    );

    // interval * range * dodge + color is the whole, legal expression.
    let d = check(
        &cat().layer(Layer::new(Mark::Interval).transform(Transform::Range).transform(Transform::Dodge).encode(Channel::Color, "continent")),
        &data(),
    );
    assert!(d.is_empty(), "interval * range * dodge + color should be legal: {:?}",
        d.iter().map(|x| &x.message).collect::<Vec<_>>());
}

#[test]
fn stack_needs_an_accumulating_mark_and_refuses_the_rest_with_direction() {
    // `stack`'s geometry is accumulation, so it is legal on the marks that can
    // accumulate — bar/area by length, `point` by piling dots — and every other
    // mark is refused with the offset that *does* fit named, the mirror of the
    // dodge division.
    let split = |m: Mark, x: &str, extra: &[Transform]| {
        let mut layer = Layer::new(m);
        for t in extra { layer = layer.transform(t.clone()); }
        layer = layer.transform(Transform::Stack).encode(Channel::Color, "continent");
        PlotSpec::new().data("t").x(x).y("life").layer(layer)
    };
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

    // An unfilled path has nothing to fill and pile — points at area.
    for m in [Mark::Line, Mark::Step] {
        let d = check(&split(m.clone(), "gdp", &[]), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("area")),
            "{m:?} * stack should be refused toward area: {:?}", msgs(&d));
    }

    // A width-bearing mark subdivides its slot instead — points at dodge.
    let d = check(&split(Mark::Box, "continent", &[]), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("dodge")),
        "box * stack should be refused toward dodge: {:?}", msgs(&d));
    let d = check(&split(Mark::Interval, "continent", &[Transform::Range]), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("dodge")),
        "interval * range * stack should be refused toward dodge: {:?}", msgs(&d));

    // A ribbon already spans a low to a high — no baseline height to pile —
    // so it is refused toward transparency, not toward another offset.
    let d = check(&split(Mark::Ribbon, "gdp", &[Transform::Range]), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("opacity")),
        "ribbon * range * stack should be refused toward opacity: {:?}", msgs(&d));
}

#[test]
fn the_dot_plot_needs_a_tally_to_pile_and_needs_no_split() {
    // `point * stack` is the dot plot (spec §5): one dot per observation, piled
    // along the measure axis. A point has no height, so what it accumulates is
    // rows — which makes its second condition a *counting transform* where
    // bar/area want a `color` split, and it needs no split at all.
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
    let dot = |extra: &[Transform]| {
        let mut layer = Layer::new(Mark::Point);
        for t in extra { layer = layer.transform(t.clone()); }
        PlotSpec::new().data("t").x("gdp").layer(layer.transform(Transform::Stack))
    };

    // The two sentences that draw one: a continuous axis cut into intervals, and
    // a categorical one tallied. Neither names a color.
    let d = check(&dot(&[Transform::Bin]), &data());
    assert!(d.is_empty(), "`point * bin * stack` is the dot plot and needs no split: {:?}", msgs(&d));
    let cat = PlotSpec::new().data("t").x("continent")
        .layer(Layer::new(Mark::Point).transform(Transform::Count).transform(Transform::Stack));
    assert!(check(&cat, &data()).is_empty(),
        "`point * count * stack` piles dots per category: {:?}", msgs(&check(&cat, &data())));

    // Nothing counts the rows: refused toward the transform that would, naming
    // both readings rather than guessing which axis the user meant.
    let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Point).transform(Transform::Stack)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("bin") && x.message.contains("count")),
        "`point * stack` alone should name bin and count: {:?}", msgs(&d));

    // A statistic that is *not* a tally is refused for the same reason: a pile of
    // 3.7 dots means nothing, and rounding it would invent data.
    for t in [Transform::Mean, Transform::Proportion, Transform::Density] {
        let d = check(&dot(&[t.clone()]), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("counts")),
            "`point * {t:?} * stack` should be refused toward a counting transform: {:?}", msgs(&d));
    }
}

#[test]
fn stack_needs_a_split_and_retires_the_area_overlap_assumption() {
    let cat = || PlotSpec::new().data("t");
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

    // No color/group: nothing to pile — refused toward adding the split.
    let d = check(&cat().x("continent").y("life")
        .layer(Layer::new(Mark::Bar).transform(Transform::Stack)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("color")),
        "bar * stack with no split should ask for color(): {:?}", msgs(&d));

    // bar * stack + color is the whole, legal expression.
    let d = check(&cat().x("continent").y("life")
        .layer(Layer::new(Mark::Bar).transform(Transform::Stack).encode(Channel::Color, "continent")), &data());
    assert!(d.is_empty(), "bar * stack + color should be legal: {:?}", msgs(&d));

    // A split area overlaps, and un-stacked that is an Assumption...
    let plain = check(&cat().x("gdp").y("life")
        .layer(Layer::new(Mark::Area).encode(Channel::Color, "continent")), &data());
    assert_eq!(kinds(&plain), [DiagnosticKind::Assumption], "a split area still warns when not stacked");

    // ...but `stack` resolves the overlap outright, so the warning is gone —
    // the forcing case this build was written against (spec §5).
    let stacked = check(&cat().x("gdp").y("life")
        .layer(Layer::new(Mark::Area).transform(Transform::Stack).encode(Channel::Color, "continent")), &data());
    assert!(stacked.is_empty(), "area * stack + color resolves the overlap and should be silent: {:?}", msgs(&stacked));
}

#[test]
fn a_pile_whose_members_disagree_in_sign_is_refused_and_one_that_agrees_is_not() {
    // `stack`'s third condition, and the only one that reads the numbers. All four
    // cases are asserted together because the refusal is easy to get *too wide*:
    // "no negatives" would forbid three plots that are perfectly well formed, which
    // is Law 8 (never forbid the ugly-but-legal), and a `sum` that comes out
    // positive from mixed rows is the one a raw-column check gets wrong.
    let pile = |ts: &[Transform], key: Vec<String>, g: Vec<String>, v: Vec<f64>| {
        let mut data = HashMap::new();
        data.insert("p".to_string(), DataFrame::new()
            .with_str("q", key).with_str("kind", g).with_float("v", v));
        let mut layer = Layer::new(Mark::Bar).encode(Channel::Color, "kind");
        for t in ts { layer = layer.transform(t.clone()); }
        check(&PlotSpec::new().data("p").x("q").y("v").layer(layer), &data)
    };
    let s = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
    let st = [Transform::Stack];

    // Mixed at one position: refused, and the message has to carry the position and
    // both numbers, since "some values are negative" is not something a reader can
    // act on without re-reading the whole table.
    let d = pile(&st, s(&["Q1", "Q1"]), s(&["sales", "returns"]), vec![5.0, -3.0]);
    let m = d.iter().find(|x| x.kind == DiagnosticKind::Illegal)
        .map(|x| x.message.clone()).unwrap_or_default();
    assert!(m.contains("disagree in sign"), "a mixed pile is refused: {:?}", msgs(&d));
    assert!(m.contains("Q1") && m.contains("sales") && m.contains("returns")
            && m.contains('5') && m.contains("-3"),
        "the refusal names the position, both groups and both numbers: {m}");
    assert!(m.contains("dodge"), "and points a bar at the offset that keeps a baseline: {m}");

    // All-positive and all-negative are both piles that agree, and both stay legal:
    // one grows up from zero, the other down, and the arithmetic is the same.
    for v in [vec![5.0, 3.0], vec![-5.0, -3.0]] {
        let d = pile(&st, s(&["Q1", "Q1"]), s(&["a", "b"]), v.clone());
        assert!(d.is_empty(), "a pile that agrees is legal ({v:?}): {:?}", msgs(&d));
    }

    // Piles at *different* positions may point different ways — each is read on its
    // own. Refusing the column rather than the pile would lose this one.
    let d = pile(&st, s(&["Q1", "Q1", "Q2", "Q2"]), s(&["a", "b", "a", "b"]),
                 vec![5.0, 3.0, -5.0, -3.0]);
    assert!(d.is_empty(), "two piles may point opposite ways: {:?}", msgs(&d));

    // And the case that decides *where* the question is asked: raw rows of mixed
    // sign whose `sum` comes out positive in every cell. The pile is made of the
    // sums, so this is legal — a check that read the bound column instead of running
    // the transforms would refuse it, which is why `check_stack_signs` runs them.
    let d = pile(&[Transform::Sum, Transform::Stack],
                 s(&["Q1", "Q1", "Q1", "Q1"]), s(&["a", "a", "b", "b"]),
                 vec![10.0, -2.0, 8.0, -1.0]);
    assert!(d.is_empty(), "mixed rows summing to positive cells are legal: {:?}", msgs(&d));
}

#[test]
fn jitter_is_point_only_and_refuses_the_rest_with_direction() {
    // The third offset's mark is `point` alone; every other mark is refused
    // toward the offset its geometry wants — the width marks at `dodge`.
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
    let jit = |m: Mark, x: &str, extra: &[Transform]| {
        let mut layer = Layer::new(m);
        for t in extra { layer = layer.transform(t.clone()); }
        PlotSpec::new().data("t").x(x).y("life").layer(layer.transform(Transform::Jitter))
    };

    // A width-bearing mark subdivides its slot — the refusal names dodge.
    for m in [Mark::Bar, Mark::Box] {
        let d = check(&jit(m.clone(), "continent", &[]), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("dodge")),
            "{m:?} * jitter should be refused toward dodge: {:?}", msgs(&d));
    }

    // A connected path or filled region is one shape, not a cloud of points —
    // refused, not toward an offset (none fits) but with that reason stated.
    for m in [Mark::Line, Mark::Area, Mark::Step, Mark::Ribbon] {
        let d = check(&jit(m.clone(), "gdp", &[]), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("not a")),
            "{m:?} * jitter should be refused as a connected shape: {:?}", msgs(&d));
    }
}

#[test]
fn jitter_needs_a_categorical_band_but_no_split() {
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

    // The documented strip plot: categorical x, continuous y — legal, and note
    // it needs *no* color split, unlike dodge/stack (it spreads coincident
    // points, which needs no grouping).
    let d = check(&PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Point).transform(Transform::Jitter)), &data());
    assert!(d.is_empty(), "point * jitter + x(cat) + y(val) should be legal with no split: {:?}", msgs(&d));

    // The horizontal strip — the category on y — is equally legal.
    let d = check(&PlotSpec::new().data("t").x("life").y("continent")
        .layer(Layer::new(Mark::Point).transform(Transform::Jitter)), &data());
    assert!(d.is_empty(), "point * jitter + x(val) + y(cat) should be legal: {:?}", msgs(&d));

    // Both axes continuous: no band to spread within, and jitter must not move a
    // measured value — refused toward opacity, the honest tool.
    let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Point).transform(Transform::Jitter)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("opacity")),
        "point * jitter on two continuous axes should be refused toward opacity: {:?}", msgs(&d));
}

#[test]
fn repel_is_text_only_and_refuses_the_rest_toward_the_offset_that_fits() {
    // The fourth offset's mark is `text` alone, because ink is what it moves and
    // a label is the only glyph made of ink. Every other mark is refused toward
    // the offset its own geometry wants, which is the same shape as the other
    // three refusals and the reason the four read as one family (spec §5).
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
    let rep = |m: Mark, extra: &[Transform]| {
        let mut layer = Layer::new(m);
        for t in extra { layer = layer.transform(t.clone()); }
        PlotSpec::new().data("t").x("continent").y("life").layer(layer.transform(Transform::Repel))
    };

    // The one that is legal: a label crowd on a scatter.
    let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Text).encode(Channel::Label, "continent")
            .transform(Transform::Repel)), &data());
    assert!(d.is_empty(), "text * repel should be legal: {:?}", msgs(&d));

    // A dot's overlap is its position, so the refusal names jitter.
    let d = check(&rep(Mark::Point, &[]), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("jitter")),
        "point * repel should be refused toward jitter: {:?}", msgs(&d));

    // A width-bearing mark subdivides its slot — the refusal names dodge.
    for m in [Mark::Bar, Mark::Box, Mark::Interval] {
        let d = check(&rep(m.clone(), &[Transform::Range]), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("dodge")),
            "{m:?} * repel should be refused toward dodge: {:?}", msgs(&d));
    }

    // A path or a filled band is one shape rather than a crowd of small ones.
    for m in [Mark::Line, Mark::Step, Mark::Area] {
        let d = check(&rep(m.clone(), &[]), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("one shape")),
            "{m:?} * repel should be refused as one shape: {:?}", msgs(&d));
    }
}

/// The path/region family's position rule, asserted across all four marks at
/// once so it cannot become a per-mark answer: **the domain takes either type,
/// the measure takes only numbers.**
///
/// `x` is the axis the path is read along, and a category is a place to read a
/// value at — the profile plot, and the radar in polar. `y` is the quantity the
/// path traces and the region closes on; a mean of category names is not one.
/// This is why the family's two positions do not share a row where `point`'s and
/// `text`'s do: a glyph's axes are the same kind of thing, a path's are not.
#[test]
fn the_path_family_reads_a_category_on_the_domain_but_never_on_the_measure() {
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
    // `ribbon` cannot be bare — its minimum syllable includes a pair transform
    // — and `area` refuses one (it draws a value per x, not a span), so each
    // mark is asked in its own smallest legal form.
    let layer = |m: Mark| match m {
        Mark::Ribbon => Layer::new(Mark::Ribbon).transform(Transform::Range),
        other => Layer::new(other),
    };

    for m in [Mark::Line, Mark::Step, Mark::Area, Mark::Ribbon] {
        assert_eq!(rule_for(&m, &Channel::X).accepts, VarType::Either,
            "{m:?} must take a category on the domain");
        assert_eq!(rule_for(&m, &Channel::Y).accepts, VarType::Continuous,
            "{m:?} must keep its measure numeric");

        // The domain: legal, and legal for the same reason on every one of them.
        let d = check(&PlotSpec::new().data("t").x("continent").y("life")
            .layer(layer(m.clone())), &data());
        assert!(d.is_empty(), "{m:?} + x(category) should be legal: {:?}", msgs(&d));

        // The measure: refused, naming the binding and what it needs.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("continent")
            .layer(layer(m.clone())), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("y(continent)")
                && x.message.contains("continuous")),
            "{m:?} + y(category) should be refused with direction: {:?}", msgs(&d));
    }

    // And the refusals that make the relaxation safe: a transform that needs a
    // number line still says so on the domain, now that the rule table no
    // longer says it for them (`check_distribution_axis`, spec §12).
    //
    // **`density` is asked with the measure unbound, and that is the point rather
    // than a convenience.** With both positions bound it is the *violin* (spec
    // §5) — the category says which rows each estimate is made from and `y` is
    // what it spreads along — so the sentence this loop used to refuse is now a
    // plot. What is still refused is the category with nothing beside it, which
    // is the case the refusal was always really about: no number line anywhere.
    for (t, atom, measure) in [(Transform::Bin, "count", true),
                               (Transform::Density, "proportion", false),
                               (Transform::Smooth, "mean", true)] {
        let spec = PlotSpec::new().data("t").x("continent");
        let spec = if measure { spec.y("life") } else { spec };
        let d = check(&spec.layer(Layer::new(Mark::Line).transform(t.clone())), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains(atom)),
            "line * {t:?} + x(category) should still be refused toward {atom}: {:?}", msgs(&d));
    }
}

#[test]
fn a_ribbon_needs_a_range_transform_and_otherwise_mirrors_area() {
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

    // Its channel row *is* `area`'s — color splits, opacity is set-only, size
    // cannot be set, shape/label are refused. Asserted against `area` directly
    // so the "mirrors area" claim cannot drift as either row changes.
    for c in [Channel::X, Channel::Y, Channel::Z, Channel::Color, Channel::Size,
              Channel::Shape, Channel::Pattern, Channel::Opacity, Channel::Group, Channel::Label, Channel::Play] {
        assert_eq!(rule_for(&Mark::Ribbon, &c).renders, rule_for(&Mark::Area, &c).renders,
            "ribbon and area must agree on {c:?}.renders");
        assert_eq!(rule_for(&Mark::Ribbon, &c).settable, rule_for(&Mark::Area, &c).settable,
            "ribbon and area must agree on {c:?}.settable");
    }

    // But where `area` stands alone (it closes on the baseline 0), a `ribbon`
    // floats between two extents like `interval`, so it needs a range transform —
    // refused with direction without one (its minimum syllable, §6/§7).
    let d = check(&base().layer(Layer::new(Mark::Ribbon)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("range")),
        "a bare ribbon should be refused toward a range transform: {:?}", msgs(&d));

    // With `range` or `confidence` it is the whole legal expression.
    for t in [Transform::Range, Transform::Confidence, Transform::Deviation] {
        let d = about_the_sentence(
            check(&base().layer(Layer::new(Mark::Ribbon).transform(t.clone())), &data()));
        assert!(d.is_empty(), "ribbon * {t:?} + x + y should be legal: {:?}", msgs(&d));
    }

    // And like `area`, a categorical x is *accepted*: a band across categories
    // is the spread profile, the filled counterpart to one `interval` whisker
    // per category. This used to be refused, on the reasoning that a band has
    // no gap to fill between categories — which was true of `area` in exactly
    // the same way, so once `area` took the domain the refusal was `interval`
    // is the tidier chart, enforced as legality. Law 8 puts that judgment back
    // where it belongs, with the reader.
    let cat_x = PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Ribbon).transform(Transform::Range));
    assert!(check(&cat_x, &data()).is_empty(),
        "a ribbon on a categorical x should be legal, like an area: {:?}",
        msgs(&check(&cat_x, &data())));
}

#[test]
fn bounds_supplies_a_precomputed_pair_for_the_span_marks() {
    let mut data = HashMap::new();
    data.insert("t".to_string(), DataFrame::new()
        .with_float("x",  vec![1.0, 2.0, 3.0])
        .with_float("lo", vec![1.0, 2.0, 3.0])
        .with_float("hi", vec![4.0, 5.0, 6.0]));
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
    let spec = |m: Mark| PlotSpec::new().data("t").x("x").layer(Layer::new(m).bounds("lo", "hi"));

    // `bounds` is legal on all four band marks (ribbon fills the pair, interval
    // whiskers it, line/step trace its two boundaries) with no `y()` — it
    // synthesizes the extents from the two named columns (like `count`).
    for m in [Mark::Ribbon, Mark::Interval, Mark::Line, Mark::Step] {
        let d = check(&spec(m.clone()), &data);
        assert!(d.is_empty(), "{m:?} * bounds + x should be legal: {:?}", msgs(&d));
    }

    // Any other mark draws no low/high pair, so `bounds` is refused there —
    // toward every mark that does, read off the shared table: the message
    // named four of the five until `zone` was found missing from it.
    let d = check(&spec(Mark::Bar), &data);
    let refusal = d.iter().find(|x| x.kind == DiagnosticKind::Illegal)
        .map(|x| x.message.clone()).unwrap_or_default();
    for m in &ALL_MARKS {
        if mark_takes_transform(m, &Transform::Bounds) != TransformLegality::None {
            assert!(refusal.contains(&format!("`{}`", mark_name(m)))
                || refusal.contains(&format!("/`{}`", mark_name(m)))
                || refusal.contains(&format!("`{}`/", mark_name(m))),
                "the refusal leaves out `{}`: {refusal}", mark_name(m));
        }
    }

    // It reshapes, never computes, so a named column that is not in the data is
    // refused with direction rather than silently drawing nothing.
    let bad = PlotSpec::new().data("t").x("x")
        .layer(Layer::new(Mark::Ribbon).bounds("lo", "nope"));
    assert!(check(&bad, &data).iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("nope")),
        "a missing bounds column should be refused: {:?}", msgs(&check(&bad, &data)));
}

/// `bin`/`density`/`smooth` describe a spread *along* an axis, so the axis they
/// read must carry a number — and the refusal must be **fatal**, which is the
/// whole point of this test. The three used to warn from `transform.rs` and then
/// hand the renderer something anyway: `bin`/`density` an empty frame (an empty
/// panel with fabricated 0..1 axes), `smooth` its input unchanged (the raw
/// scatter drawn as if it were the fit). `writing.qmd` documented the `bin` case
/// as a refusal for several sessions while it quietly drew that empty panel.
#[test]
fn a_distributional_transform_needs_a_numeric_axis_and_refuses_fatally() {
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

    // Each of the three is refused on a categorical key axis, fatally, and points
    // at the atom that *does* answer the categorical question.
    for (t, mark, toward) in [
        (Transform::Bin, Mark::Bar, "count"),
        (Transform::Density, Mark::Bar, "proportion"),
        (Transform::Smooth, Mark::Point, "mean"),
    ] {
        let spec = PlotSpec::new().data("t").x("continent").y("life")
            .layer(Layer::new(mark.clone()).transform(t.clone()));
        let d = check(&spec, &data());
        assert!(
            d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("continent")
                && x.message.contains(toward)),
            "{mark:?} * {t:?} on a categorical x must be Illegal and point at `{toward}`: {:?}",
            msgs(&d)
        );
    }

    // The same three on a numeric axis stay legal — the refusal must not have
    // swallowed the histogram, the density curve or the LOESS fit.
    // `bin` and `density` bind no `y`: they draw their own number there, and a
    // column named on it would be refused as read by nothing.
    for (t, mark, y) in [
        (Transform::Bin, Mark::Bar, None),
        (Transform::Density, Mark::Area, None),
        (Transform::Smooth, Mark::Point, Some("life")),
    ] {
        let spec = PlotSpec::new().data("t").x("gdp");
        let spec = match y { Some(y) => spec.y(y), None => spec };
        let d = check(&spec.layer(Layer::new(mark.clone()).transform(t.clone())), &data());
        assert!(d.is_empty(), "{mark:?} * {t:?} on numeric axes must stay legal: {:?}", msgs(&d));
    }

    // Which axis is read is *relational*, not fixed to x: a horizontal bar cuts
    // `y`, so a categorical y is refused there — and a categorical *x* is not,
    // because on a horizontal bar x is the measure the transform writes.
    let horiz = |axis_cat: bool| {
        let s = PlotSpec::new().data("t");
        let s = if axis_cat { s.y("continent") } else { s.y("life") };
        s.layer(Layer::new(Mark::Bar).transform(Transform::Bin))
    };
    let d = check(&horiz(true), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("y(continent)")),
        "a horizontal `bar * bin` must refuse a categorical y, naming it: {:?}", msgs(&d));
    let d = check(&horiz(false), &data());
    assert!(d.is_empty(), "`bar * bin + y(life)` is the histogram on its side: {:?}", msgs(&d));

    // `smooth` fits y against x, so it needs *both* axes numeric — a categorical
    // y is refused even when x is fine. This is what makes it not just "check x".
    let d = check(&PlotSpec::new().data("t").x("gdp").y("continent")
        .layer(Layer::new(Mark::Point).transform(Transform::Smooth)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("y(continent)")),
        "`smooth` must refuse a categorical y as well as a categorical x: {:?}", msgs(&d));
}

// The Mark × Transform grid (spec §5): `mark_takes_transform` is the one source
// the generated grid reads. These pin it to the engine so the grid can never
// promise a combination the engine refuses, nor hide one it allows — the
// transform analog of `settings_grid_agrees_with_the_style_checks`.

#[test]
fn transform_grid_agrees_with_the_dedicated_checks() {
    // The four transforms with a dedicated mark-check now read `mark_takes_transform`
    // for the mark half, so they agree by construction; calling each in isolation —
    // with its *secondary* condition satisfied (a split for dodge/stack, a
    // categorical axis for jitter, existing columns for bounds) — proves the
    // consultation is wired and stays wired.
    let df = DataFrame::new()
        .with_float("gdp", vec![1.0, 2.0, 3.0])
        .with_float("life", vec![4.0, 5.0, 6.0])
        .with_float("lo", vec![1.0, 2.0, 3.0])
        .with_float("hi", vec![4.0, 5.0, 6.0])
        .with_str("continent", vec!["Asia".into(), "Europe".into(), "Africa".into()]);
    let cat_x = PlotSpec::new().data("t").x("continent").y("life");

    for m in &ALL_MARKS {
        if !is_drawable(m) {
            continue; // path/surface are refused before any transform check runs
        }
        let split = |t: Transform| {
            Layer::new(m.clone()).transform(t).encode(Channel::Color, "continent")
        };
        let expect = |t: Transform| mark_takes_transform(m, &t) != TransformLegality::None;

        let mut out = Vec::new();
        check_dodge(&mut out, &split(Transform::Dodge));
        assert_eq!(out.is_empty(), expect(Transform::Dodge), "{m:?} dodge: {out:?}");

        // `stack`'s secondary condition is not the same sentence for every mark,
        // because what a mark piles differs: bar/area pile a split's measured
        // heights, `point` piles the rows a tally counted (spec §5). Each gets
        // its own satisfied form, or the point row would fail here for the
        // secondary condition while the grid is asking about the mark.
        let mut out = Vec::new();
        let stack_layer = match m {
            Mark::Point => split(Transform::Stack).transform(Transform::Count),
            _ => split(Transform::Stack),
        };
        check_stack(&mut out, &stack_layer);
        assert_eq!(out.is_empty(), expect(Transform::Stack), "{m:?} stack: {out:?}");

        let mut out = Vec::new();
        let jl = Layer::new(m.clone()).transform(Transform::Jitter);
        check_jitter(&mut out, &cat_x, &df, &jl);
        assert_eq!(out.is_empty(), expect(Transform::Jitter), "{m:?} jitter: {out:?}");

        let mut out = Vec::new();
        check_bounds(&mut out, &df, &Layer::new(m.clone()).bounds("lo", "hi"));
        assert_eq!(out.is_empty(), expect(Transform::Bounds), "{m:?} bounds: {out:?}");
    }
}

#[test]
fn transform_statistics_and_pairs_match_the_engine_off_their_marks() {
    // The value statistics and `range`/`confidence` are *not* routed through the
    // table (their refusals are emergent — `check_span_needs_range`, `check_box`,
    // the `label` requirement — plus the new gap-closing `check_pair_transform_marks`).
    // So a representative full `check()` validates the grid against the engine
    // non-circularly: a continuous x/y that every locus mark accepts, one value
    // statistic and one pair transform, across every drawable mark.
    let fatal = |d: &[Diagnostic]| d.iter().any(|x| x.is_fatal());
    for m in &ALL_MARKS {
        if !is_drawable(m) {
            continue;
        }
        for t in [Transform::Mean, Transform::Range] {
            // Each mark gets a spec it could actually draw, the way the `stack`
            // row above gives `point` its own satisfied form — otherwise this
            // asks about the *positions* while the grid is answering about the
            // mark. A `zone` measures by color over a pair of categorical
            // slots, so that is what a `zone * mean` has to be handed (spec §5);
            // every other mark reads a continuous x/y.
            let spec = match m {
                Mark::Zone => PlotSpec::new().data("t").x("continent").y("region")
                    .layer(Layer::new(m.clone()).transform(t.clone())
                        .encode(Channel::Color, "life")),
                // A surface has no flat form *and* no single-transform form: it
                // measures with `z`, so a reduction needs cells to reduce into, and
                // only `bin` cuts a surface any (spec §15). So the spec it "could
                // actually draw" is the terraced sheet — the same accommodation the
                // `zone` above gets for needing two categorical slots. `bin` rides
                // along for `range` too, where it changes nothing: that refusal is
                // the pair transform's, and the grid still says `None`.
                Mark::Surface => PlotSpec::new().data("t").x("gdp").y("life").z("value")
                    .layer(Layer::new(m.clone())
                        .transform(Transform::Bin).transform(t.clone())),
                // A text prints its label, and the one a summary's rows hold is
                // the column it reduced: `text * mean + label(life)`.
                Mark::Text => PlotSpec::new().data("t").x("continent").y("life")
                    .layer(Layer::new(m.clone()).transform(t.clone())
                        .encode(Channel::Label, "life")),
                _ => PlotSpec::new().data("t").x("gdp").y("life")
                    .layer(Layer::new(m.clone()).transform(t.clone())),
            };
            let refused = fatal(&check(&spec, &data()));
            let table_none = mark_takes_transform(m, &t) == TransformLegality::None;
            assert_eq!(
                refused, table_none,
                "{m:?} * {}: engine refuses={refused}, grid says none={table_none}: {:?}",
                transform_name(&t),
                check(&spec, &data()).iter().map(|x| x.message.clone()).collect::<Vec<_>>(),
            );
        }
    }
}

#[test]
fn a_pair_transform_is_refused_on_a_locus_mark_toward_the_span_mark() {
    // The Law-1 gap the grid surfaced: `range`/`confidence` draw a span, so on a
    // mark that draws one value per x they are refused — `point`/`bar` toward the
    // whisker/band marks, `area` toward `ribbon` (its filled twin). Previously
    // these rendered nonsense (two bars per group) with no word.
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
    for m in [Mark::Point, Mark::Bar] {
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(m.clone()).transform(Transform::Range)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && (x.message.contains("interval") || x.message.contains("ribbon"))),
            "{m:?} * range should point at the span marks: {:?}", msgs(&d));
    }
    let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Area).transform(Transform::Confidence)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("ribbon")),
        "area * confidence should point at ribbon: {:?}", msgs(&d));
}

/// **The published chain grid says `—` for exactly the pairs the engine refuses.**
///
/// The book generates its Transform × Transform grid from `chain_cells`, and a
/// generated grid is only trustworthy while what it is generated *from* is the
/// same thing the caller meets. It was not, for a few hours on 2026-07-31: the
/// cells came from `job_conflict` alone, so the grid marked `proportion * smooth`
/// legal while `check_share_composition` refused it — the same over-promise a
/// hand-written legend once put on the Mark × Space column, arriving through the
/// generator instead.
///
/// Two rules reach a pair, so the grid has to read both. This walks every ordered
/// pair against `check` itself rather than against either predicate, which is what
/// makes it catch a *third* rule if one is ever added in a third place.
#[test]
fn the_published_chain_grid_matches_what_the_engine_refuses() {
    let mtx = rules_matrix();
    // A mark that takes as much as possible, so a pair is judged on the pair
    // rather than on the mark: `bar` takes 12 of the 17, `line` the pair
    // transforms `bar` does not. Between them they cover every transform that
    // composes at all.
    for (mark, x, y) in [(Mark::Bar, "cat", "life"), (Mark::Line, "gdp", "life")] {
        for cell in &mtx.chain_cells {
            let (Some(a), Some(b)) = (
                USER_TRANSFORMS.iter().find(|t| transform_name(t) == cell.a),
                USER_TRANSFORMS.iter().find(|t| transform_name(t) == cell.b),
            ) else { continue };
            // Only pairs this mark actually takes can say anything about the pair.
            if mark_takes_transform(&mark, a) == TransformLegality::None
                || mark_takes_transform(&mark, b) == TransformLegality::None { continue }
            let layer = Layer::new(mark.clone())
                .transform(a.clone()).transform(b.clone()).bounds("lo", "hi");
            let d = check(&PlotSpec::new().data("t").x(x).y(y).layer(layer), &data());
            let refused = d.iter().any(|x| x.kind == DiagnosticKind::Illegal);
            if !cell.legal {
                assert!(refused,
                    "the grid says `{} * {}` is refused, and the engine drew it",
                    cell.a, cell.b);
            }
        }
    }
    // And the reasons the grid can give are the two rules that exist.
    for cell in &mtx.chain_cells {
        if let Some(j) = cell.job {
            assert!(["extent", "measure", "scale", "position", "share"].contains(&j),
                "`{} * {}` reports an unknown reason `{j}`", cell.a, cell.b);
        }
    }
}

#[test]
fn transforms_matrix_covers_every_transform_and_mark() {
    // The `transforms` block of the dump the grid is generated from.
    let mtx = rules_matrix();
    assert_eq!(mtx.transforms.len(), USER_TRANSFORMS.len());
    assert_eq!(mtx.transform_cells.len(), USER_TRANSFORMS.len() * ALL_MARKS.len());
    let tc = |t: &str, m: &str| {
        mtx.transform_cells.iter().find(|c| c.transform == t && c.mark == m).unwrap().state
    };
    let cls = |t: &str| mtx.transforms.iter().find(|i| i.name == t).unwrap().class;
    // The three classes are labeled so the book can group the columns.
    assert_eq!(cls("bin"), "statistic");
    assert_eq!(cls("range"), "pair");
    assert_eq!(cls("dodge"), "collision");
    // Corners: a value statistic on a locus mark vs a span mark; the pair
    // transform required on interval/ribbon, optional on line, and the closed gap
    // on bar; and the collision trio's partition — which divides by geometry *and
    // axis*, so `point` appears under all three: `stack` for the dot plot's
    // measure axis, `jitter` for the strip plot's categorical one, and `dodge`
    // for the beeswarm's, the least distance that clears the others.
    assert_eq!(tc("bin", "bar"), "combines");
    assert_eq!(tc("bin", "interval"), "none");
    assert_eq!(tc("range", "interval"), "required");
    assert_eq!(tc("range", "ribbon"), "required");
    assert_eq!(tc("range", "line"), "combines");
    assert_eq!(tc("range", "bar"), "none");
    assert_eq!(tc("dodge", "bar"), "combines");
    assert_eq!(tc("dodge", "point"), "combines");
    assert_eq!(tc("stack", "area"), "combines");
    assert_eq!(tc("stack", "point"), "combines");
    assert_eq!(tc("stack", "box"), "none");
    assert_eq!(tc("jitter", "point"), "combines");
    assert_eq!(tc("jitter", "bar"), "none");
}

// The Mark × Space grid — the fourth face of the orthogonality matrix. Same
// two guards its three siblings have: the dump covers every pair, and the
// table the grid prints is the one the refusals enforce.

#[test]
fn spaces_matrix_covers_every_space_and_mark() {
    let mtx = rules_matrix();
    assert_eq!(mtx.spaces.len(), ALL_SPACES.len());
    assert_eq!(mtx.space_cells.len(), ALL_SPACES.len() * ALL_MARKS.len());
    let sc = |s: &str, m: &str| {
        mtx.space_cells.iter().find(|c| c.space == s && c.mark == m).unwrap().drawn
    };
    // Corners the book's glyph mapping relies on: every drawable mark in the
    // plane; only the scatter in space; the two designed-but-unbuilt spaces
    // empty throughout.
    assert!(sc("flat", "bar") && sc("flat", "box") && sc("flat", "ribbon"));
    assert!(!sc("flat", "surface"), "a sheet has no reading in the plane");
    assert!(sc("space", "point"));
    assert!(!sc("space", "line"));
    // `path` bends with the rest — its segments are strokes between placed
    // vertices, which is `line`'s geometry, so the spiral needs no arc work.
    assert!(sc("flat", "path") && sc("polar", "path"));
    // **Polar's column has exactly one blank, and it is not a polar gap.**
    // Every mark that draws flat draws bent, since 2026-07-26; `surface` is
    // absent from both for one reason, its minimum syllable including the cube.
    // Written as the whole column rather than as a list of corners, because a
    // list is what went stale twice while `path` and then `rule` learned to
    // bend — the same hand-written-list drift `check_polar`'s message fixed.
    for m in &ALL_MARKS {
        assert_eq!(
            sc("polar", mark_name(m)),
            sc("flat", mark_name(m)),
            "{m:?}: the plane and the circle must agree about which marks draw"
        );
    }
    // **The globe column, stated whole and by name: `map`'s five and one
    // more.** The sphere spends both positions on the place, so the marks
    // that place something stand on both — and the sphere alone has a
    // direction to spare, the radius, so `bar` stands here and not on the
    // flattening. Exactly that one cell may separate the two columns; any
    // other drift is this assertion failing.
    for m in &ALL_MARKS {
        let drawn = matches!(m,
            Mark::Point | Mark::Text | Mark::Path | Mark::Rule | Mark::Zone | Mark::Bar);
        assert_eq!(sc("globe", mark_name(m)), drawn,
            "{m:?}: the globe column and the renderer disagree");
        let expect_match = *m != Mark::Bar;
        assert_eq!(sc("globe", mark_name(m)) == sc("map", mark_name(m)), expect_match,
            "{m:?}: the sphere and its flattening may differ only where the radius is");
    }
    // **The map column, stated whole.** A map is a map of the plane, so it has
    // a coordinate for everything — but longitude and latitude spend *both*
    // axes, so a mark needing one to measure along has none left. What draws is
    // what a position places and nothing else: a place, a route through places,
    // a name at a place, and the rule that spans the axis it does not name,
    // which is how the graticule is a mark rather than furniture.
    //
    // Restating the list here is the point rather than a duplication. This
    // assertion is what makes the column change on purpose — it is the one that
    // failed when `map` gained its first four marks, which is exactly the job a
    // hand-written list should do when the generated grid moves under it.
    //
    // `zone` joined them on 2026-08-01 and is the **choropleth**. It is the one
    // mark here that a position does not place: its sides come from a boundary,
    // which is the fifth extent description this mark reads and the first that
    // arrives as many rows per shape.
    for m in &ALL_MARKS {
        let draws = matches!(
            m,
            Mark::Point | Mark::Path | Mark::Text | Mark::Rule | Mark::Zone
        );
        assert_eq!(sc("map", mark_name(m)), draws,
            "{m:?}: a map draws what a position places, plus the region a \
             boundary bounds — and nothing that measures along an axis it \
             does not have");
    }
}

/// **A blank `z` cell is one of three different things**, and until 2026-07-26
/// every one of them said "not drawn yet" — including the four the spec had
/// argued *against* since M8a. A message that promises a decided refusal is
/// coming is the same defect as a book chunk claiming a refusal that stopped
/// happening, one layer down.
#[test]
fn a_blank_z_cell_says_which_of_three_things_it_is() {
    // Decided: the path/region family reads a domain, and a cube has no left to
    // right. `Illegal`, and the direction is `path` — `line` with the sort removed.
    for m in [Mark::Line, Mark::Step, Mark::Area, Mark::Ribbon] {
        assert_eq!(z_refusal_kind(&m), DiagnosticKind::Illegal, "{m:?} is a ruling, not a gap");
        let msg = z_refusal(&m, "h");
        assert!(msg.contains("no left to right"), "{m:?}: {msg}");
        assert!(msg.contains("`path`"), "{m:?} must point at the mark that does draw: {msg}");
        assert!(!msg.contains("not drawn yet") && !msg.contains("does not draw it yet"),
            "{m:?} is refused by the grammar, so it must not promise a renderer: {msg}");
    }
    // Blocked on occlusion: both are marks whose 3-D form has no footprint, so
    // painter's order cannot place them. `Unsupported` — owed, not refused.
    for m in [Mark::Rule, Mark::Zone] {
        assert_eq!(z_refusal_kind(&m), DiagnosticKind::Unsupported);
        let msg = z_refusal(&m, "h");
        assert!(msg.contains("footprint"), "{m:?} must name why it cannot be sorted: {msg}");
        // The blocker is named in plain words rather than as "occlusion", which is
        // rendering jargon this audience does not have (book law 8), and it no longer
        // cites a milestone id a reader cannot look up. Assert the meaning, not the term.
        assert!(msg.contains("what hides what"), "{m:?} must name the blocker: {msg}");
        assert!(!msg.contains("occlusion") && !msg.contains("M8a"),
            "{m:?}: a refusal a user reads must not use rendering jargon or an internal \
             milestone id: {msg}");
    }
    // And the marks that draw take no message at all.
    for m in [Mark::Point, Mark::Bar, Mark::Path, Mark::Surface, Mark::Box, Mark::Interval] {
        assert!(rule_for(&m, &Channel::Z).renders.is_some(), "{m:?} should draw in the cube");
    }
}

/// **The three slot marks stand or fall together**, which is what `is_slot_mark`
/// asserts and what `slot_orient` has said in prose since orientation was
/// decided: a bar's length, a whisker's span and a box's summary are the same
/// question asked of the same pair of axes. A cube that took one and not the
/// others would be the Law-1 gap the family exists to prevent.
#[test]
fn the_three_slot_marks_all_stand_in_the_cube() {
    for m in ALL_MARKS.iter().filter(|m| is_slot_mark(m)) {
        assert!(mark_draws_in_space(m, SpaceKind::Space),
            "{m:?} is a slot mark and must stand on the cube's floor");
        // And the dimensionality rule must agree, or `check_slot_shape` would
        // still demand a measured axis on a floor of two categories.
        assert!(cuts_both_positions(m, SpaceKind::Space),
            "{m:?} must cut the floor in the cube");
        assert_eq!(measure_channel(m, SpaceKind::Space), Some(Channel::Z),
            "{m:?} measures along `z` in the cube");
    }
}

/// **A pair is a reduction too.** `reads_two_dimensions` asked only about the
/// five single-value statistics, so a whisker in the cube fell through to the
/// one-key branch and drew one per *row* rather than one per cell.
#[test]
fn a_pair_transform_is_a_two_dimensional_reading_in_the_cube() {
    for t in [Transform::Range, Transform::Confidence, Transform::Deviation, Transform::Box] {
        assert!(crate::transform::pairs_a_column(&[t.clone()]), "{t:?} pairs a column");
        assert!(reads_two_dimensions(&Mark::Interval, &[t.clone()], SpaceKind::Space),
            "{t:?} on a whisker in the cube must group by the floor");
        // Flat, the same sequence is a *one*-dimensional reading — the whole
        // point of the subtraction is that the space decides, not the transform.
        assert!(!reads_two_dimensions(&Mark::Interval, &[t.clone()], SpaceKind::Flat),
            "{t:?} on a flat whisker groups by its slot, not by a pair");
    }
    // `bounds` names columns that already hold the pair, so there is nothing for
    // a cell to reduce and it is not one of them.
    assert!(!crate::transform::pairs_a_column(&[Transform::Bounds]));
}

/// **A hexagonal mesh has no polar reading** — `bin(tiling = )`'s third
/// refusal, and the one that arrived with the space rather than with the
/// tiling. The first two are a one-dimensional bin and a categorical axis;
/// this is the third way a plane can fail to be there. `rect` must survive,
/// because a rectangle bent is the sector the mark draws.
#[test]
fn a_hexagonal_mesh_has_no_polar_reading_and_a_rectangular_one_does() {
    let polar_bin = |tiling: &str| {
        let mut out = Vec::new();
        let mut layer = Layer::new(Mark::Zone).transform(Transform::Bin);
        layer.bin = Some(crate::ir::BinSpec { tiling: Some(tiling.into()), ..Default::default() });
        let spec = PlotSpec::new().data("t").x("a").y("b")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(layer);
        check_polar(&mut out, &spec);
        out
    };
    let hex = polar_bin("hex");
    assert_eq!(hex.len(), 1, "a hex mesh in polar must be refused exactly once");
    assert_eq!(hex[0].kind, DiagnosticKind::Illegal);
    // Direction, never a bare refusal (§12): it must name the tiling that does
    // bend and the other way out.
    assert!(hex[0].message.contains("rect"), "no direction: {}", hex[0].message);
    assert!(hex[0].message.contains("drop `polar()`"), "no second way out: {}", hex[0].message);
    assert!(polar_bin("rect").is_empty(), "a rectangle bent is a sector, which draws");
}

// -- the violin: the slot reading of `density` (spec §5) -------------------

/// `x(continent) + y(life)` — a category and a number, which is the whole
/// condition. Both marks that draw the reading, upright and lying down.
#[test]
fn a_violin_is_legal_on_both_marks_and_both_ways_round() {
    for mark in [Mark::Ribbon, Mark::Area] {
        let upright = PlotSpec::new().data("t").x("continent").y("life")
            .layer(Layer::new(mark.clone()).transform(Transform::Density));
        let d = check(&upright, &data());
        assert!(d.is_empty(), "{mark:?} * density + x(cat) + y(num): {:?}", msgs(&d));

        // Sideways, with the category on `y` — the form with room for long
        // names. The static rule table says `y` on these marks is the measure,
        // and it is right in every other reading; the violin reads its
        // orientation off the bindings the way `bar`/`box`/`interval` do (§6),
        // which is why there is still no `flip` atom.
        let sideways = PlotSpec::new().data("t").x("life").y("continent")
            .layer(Layer::new(mark.clone()).transform(Transform::Density));
        let d = check(&sideways, &data());
        assert!(d.is_empty(), "{mark:?} * density + x(num) + y(cat): {:?}", msgs(&d));
    }
}

/// The reading is selected by the *types*, so the curve must still be refused on
/// a `ribbon` — with direction toward the binding that would make it a violin,
/// not toward `range`.
#[test]
fn a_ribbon_density_curve_is_refused_toward_the_violin() {
    let curve = PlotSpec::new().data("t").x("life")
        .layer(Layer::new(Mark::Ribbon).transform(Transform::Density));
    let d = check(&curve, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("violin")),
        "a ribbon density curve should point at the violin: {:?}", msgs(&d));
}

/// A category with **no measure** is not the slot reading, and must keep the
/// refusal it has always had — the estimate has nothing to spread along.
#[test]
fn a_density_over_a_bare_category_is_still_refused() {
    let d = check(&PlotSpec::new().data("t").x("continent")
        .layer(Layer::new(Mark::Area).transform(Transform::Density)), &data());
    assert!(!d.is_empty(), "`area * density + x(cat)` must still be refused");
}

/// `compare` belongs to exactly one reading, as `bandwidth` and `levels` do —
/// refused on the curve and the field, and its value checked by name so a typo
/// cannot fall through to the default and answer the other question.
#[test]
fn compare_is_refused_in_the_readings_it_cannot_mean() {
    let with = |spec: PlotSpec, how: &str| {
        let mut s = spec;
        for l in &mut s.layers {
            l.density = Some(crate::ir::DensitySpec {
                adjust: None, bandwidth: None, levels: None, compare: Some(how.into()), reach: None, support: (None, None),
            });
        }
        check(&s, &data())
    };
    // The curve — one estimate with a whole axis to itself, and no slots.
    let d = with(base().layer(Layer::new(Mark::Line).transform(Transform::Density)), "count");
    assert!(d.iter().any(|x| x.message.contains("no slots")),
        "compare on a curve should be refused: {:?}", msgs(&d));
    // The field — measured by color, with no width in it at all.
    let d = with(base().layer(Layer::new(Mark::Path).transform(Transform::Density)), "count");
    assert!(d.iter().any(|x| x.message.contains("no slots")),
        "compare on a field should be refused: {:?}", msgs(&d));
    // On the violin, both words are legal and a third is not.
    let violin = || PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Ribbon).transform(Transform::Density));
    for how in [crate::ir::COMPARE_SHAPE, crate::ir::COMPARE_COUNT] {
        let d = with(violin(), how);
        assert!(d.is_empty(), "compare = {how:?} should be legal: {:?}", msgs(&d));
    }
    let d = with(violin(), "area");
    assert!(d.iter().any(|x| x.message.contains("not a reading this engine has")),
        "an unknown compare should be refused by name: {:?}", msgs(&d));
}

/// The two stroke marks joined the slot reading to close a **silent misdraw**,
/// not only to add the ridgeline's edge: `line * density + x(<number>) +
/// y(<category>)` was legal before and drew the *pooled* curve, because a bound
/// `y` on a synthesizing transform names the output column — so the category was
/// swallowed as a name and the axis came out reading "Continent" over density
/// values. Pinned as a *plot* now rather than as the absence of a diagnostic,
/// since the old behavior raised none either.
#[test]
fn the_stroke_marks_trace_the_slot_reading_rather_than_pooling_it() {
    for mark in [Mark::Line, Mark::Step] {
        let spec = PlotSpec::new().data("t").x("life").y("continent")
            .layer(Layer::new(mark.clone()).transform(Transform::Density));
        let d = check(&spec, &data());
        assert!(d.is_empty(), "{mark:?} * density + x(num) + y(cat): {:?}", msgs(&d));
        assert_eq!(slot_density(&spec, &spec.layers[0], data().get("t")),
                   Some(Orient::Horizontal),
                   "{mark:?} must read as the slot reading, not the pooled curve");
    }
    // The curve is still the curve when there is no category to group by.
    let curve = base().layer(Layer::new(Mark::Line).transform(Transform::Density));
    assert!(slot_density(&curve, &curve.layers[0], data().get("t")).is_none());
}

/// `reach` is the fourth knob to belong to one reading, and the slot's second.
#[test]
fn reach_is_refused_where_there_are_no_slots_to_measure_it_in() {
    let with = |spec: PlotSpec, r: f64| {
        let mut s = spec;
        for l in &mut s.layers {
            l.density = Some(crate::ir::DensitySpec {
                adjust: None, bandwidth: None, levels: None, compare: None, reach: Some(r), support: (None, None),
            });
        }
        check(&s, &data())
    };
    let d = with(base().layer(Layer::new(Mark::Line).transform(Transform::Density)), 2.5);
    assert!(d.iter().any(|x| x.message.contains("no slots")),
        "reach on a curve should be refused: {:?}", msgs(&d));
    let violin = || PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Area).transform(Transform::Density));
    assert!(with(violin(), 2.5).is_empty(), "an overlapping ridge is a plot, not a mistake");
    let d = with(violin(), -1.0);
    assert!(d.iter().any(|x| x.message.contains("positive number of slots")),
        "a negative reach should be refused: {:?}", msgs(&d));
}

/// A split violin's regions stand in **separate slots**, so the warning that a
/// split `area` hides itself is false there — it fired on every colored
/// ridgeline until this was scoped.
#[test]
fn a_split_violin_raises_no_overlap_warning() {
    let spec = PlotSpec::new().data("t").x("life").y("continent")
        .layer(Layer::new(Mark::Area).transform(Transform::Density)
            .encode(Channel::Color, "continent"));
    let d = check(&spec, &data());
    assert!(d.is_empty(), "a colored ridgeline warns about nothing: {:?}", msgs(&d));
    // …while an ordinary split area, whose regions do share a domain, still does.
    let plain = base().layer(Layer::new(Mark::Area).encode(Channel::Color, "continent"));
    assert!(!check(&plain, &data()).is_empty(),
        "the overlap warning must survive for the case it was written for");
}

/// The grid the book generates must agree that a ribbon takes `density`.
/// `book/combinations.qmd` is built from this dump on every render, so a cell
/// left saying "refused" would be the book documenting a refusal that stopped
/// happening — the failure `check_refusals.R` exists to catch one level up.
#[test]
fn the_grid_records_that_a_ribbon_takes_density() {
    assert_eq!(mark_takes_transform(&Mark::Ribbon, &Transform::Density),
               TransformLegality::Required,
               "the violin is a fourth way to satisfy a ribbon's minimum syllable");
    assert_eq!(mark_takes_transform(&Mark::Area, &Transform::Density),
               TransformLegality::Combines);
    // And no other span mark gained it: a whisker has no width to spread across.
    assert_eq!(mark_takes_transform(&Mark::Interval, &Transform::Density),
               TransformLegality::None);
}

/// `surface` is refused in polar by `check_surface` and **not** a second time
/// by `check_polar`. Pins the one exclusion in the agreement below, so that
/// removing it shows up here rather than as a doubled diagnostic in a plot.
#[test]
fn a_surface_in_polar_is_refused_once_and_names_the_cube() {
    let mut out = Vec::new();
    let spec = PlotSpec::new().data("t").x("a").y("b")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(Layer::new(Mark::Surface));
    check_polar(&mut out, &spec);
    assert!(out.is_empty(), "check_polar must not speak for a surface: {out:?}");
}

#[test]
fn space_grid_agrees_with_the_polar_refusals() {
    // The grid says which marks draw in polar; `check_polar` refuses the rest.
    // If the two ever disagree the book would promise a plot the engine will
    // not draw — the drift the generated grids exist to make impossible.
    for m in &ALL_MARKS {
        if !is_drawable(m) {
            continue; // refused earlier, by `check_mark`, with its own direction
        }
        // `surface` is the one mark that does not bend and is deliberately
        // *not* refused here: a sheet needs the cube, which is the sentence
        // `check_surface` already gives with both routes into it named, and
        // saying it twice helps nobody. So it is excluded from the agreement
        // rather than silently making it fail.
        if matches!(m, Mark::Surface) {
            continue;
        }
        // `edge` likewise: its own refusal names `network()`, and a polar one
        // beside it sent the reader to a flat `edge`, refused too.
        if *m == Mark::Edge {
            let spec = PlotSpec::new().data("t")
                .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
                .layer(Layer::new(Mark::Edge));
            let out = check(&spec, &data());
            assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
                && d.message.contains("network()")), "edge in polar must still be refused");
            assert!(out.iter().all(|d| !d.message.contains("Drop `polar()`")), "{out:?}");
            continue;
        }
        let mut out = Vec::new();
        let spec = PlotSpec::new().data("t").x("gdp").y("life")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(Layer::new(m.clone()));
        check_polar(&mut out, &spec);
        assert_eq!(
            out.is_empty(),
            mark_draws_in_space(m, SpaceKind::Polar),
            "{m:?} in polar: refusals={:?}",
            out.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
        );
        // A refusal must give direction, never a bare "not supported" (§12).
        for d in &out {
            assert_eq!(d.kind, DiagnosticKind::Unsupported);
            assert!(d.message.contains("Drop `polar()`"), "no direction: {}", d.message);
        }
    }
}

#[test]
fn a_bound_z_projects_without_space_and_a_synthesized_one_needs_it() {
    // The two ways to have a third dimension, and the asymmetry between them
    // (§15). Binding `z` is the original trigger and needs no `space()` — that
    // only sets the angle — while a *synthesized* `z` needs the coordinate to say
    // where the invented measure goes, or every flat histogram would project.
    //
    // This is a regression pin, not a restatement. `space_of` used to require
    // `CoordSpace::Space` for both, so it disagreed with the renderer's own copy
    // of the test about every `z`-bound plot drawn since M8a; making the renderer
    // read `space_of` then silently flattened the glider route — a 50 KB plot
    // came out 7 KB, and it was byte-comparison against the previous build that
    // caught it rather than any test here.
    let bare_z = PlotSpec::new().data("t").x("gdp").y("life").z("pop")
        .layer(Layer::new(Mark::Point));
    assert_eq!(space_of(&bare_z), SpaceKind::Space,
        "a bound `z` projects on its own — `space()` sets the angle, not the dimension");

    let flat_hist = PlotSpec::new().data("t").x("life")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    assert_eq!(space_of(&flat_hist), SpaceKind::Flat,
        "a flat histogram invents a measure too — it must not project");

    let hist_3d = PlotSpec::new().data("t").x("gdp").y("life")
        .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
    assert_eq!(space_of(&hist_3d), SpaceKind::Space,
        "`space()` plus a transform that invents a measure is the 3-D histogram");
}

/// **A facet crossed with the cube is clean, and both routes into the cube
/// agree** — which is the part that was broken rather than merely missing.
///
/// `check_space` refused a *bound* `z` beside a facet as "not drawn yet", and
/// never reached that line for a *synthesized* `z`, because it early-returns
/// on `axis_def(Z)` first. So the 3-D histogram faceted drew five cubes under
/// `GOG_STRICT=1` while the scatter beside it was refused: one partition, two
/// answers, and a Law 2 break that shipped. Deleting the refusal is what makes
/// these two lines assert the same thing.
#[test]
fn a_facet_crossed_with_the_cube_is_clean() {
    let d = data();
    let bound = PlotSpec::new().data("t").x("gdp").y("life").z("value")
        .layer(Layer::new(Mark::Point))
        .facet_col("continent");
    assert!(msgs(&check(&bound, &d)).is_empty(),
        "a faceted cube is drawn: {:?}", msgs(&check(&bound, &d)));

    let synthesized = PlotSpec::new().data("t").x("continent").y("region")
        .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        .layer(Layer::new(Mark::Bar).transform(Transform::Count))
        .facet_col("continent");
    assert!(msgs(&check(&synthesized, &d)).is_empty(),
        "and so is the faceted 3-D histogram, which always was: {:?}",
        msgs(&check(&synthesized, &d)));
}

/// `free` on `z` needed no rule of its own, and this is the assertion that
/// says so. `check_free` reads the *positions* as one family of three (Law 1),
/// so the third one has always been accepted with a facet and refused without
/// one. It was simply unreachable while the cube could not be faceted — spec
/// §11 recorded it as "accepts it and cannot yet reach it", predicting it
/// would start working with nothing added. It did.
#[test]
fn free_on_z_needs_no_second_rule() {
    let d = data();
    let free_z = |spec: PlotSpec| {
        let mut s = spec;
        s.z = s.z.map(|def| def.with_free());
        s
    };
    let with_facet = free_z(PlotSpec::new().data("t").x("gdp").y("life").z("value")
        .layer(Layer::new(Mark::Point))
        .facet_col("continent"));
    assert!(msgs(&check(&with_facet, &d)).is_empty(),
        "a freed z is legal beside a facet: {:?}", msgs(&check(&with_facet, &d)));

    let no_facet = free_z(PlotSpec::new().data("t").x("gdp").y("life").z("value")
        .layer(Layer::new(Mark::Point)));
    assert!(msgs(&check(&no_facet, &d)).iter().any(|m| m.contains("free")),
        "a freed axis with no panels to free it across is refused: {:?}",
        msgs(&check(&no_facet, &d)));
}

#[test]
fn a_reduction_groups_by_the_positions_the_mark_does_not_measure_with() {
    // Spec §5's subtraction, read as the **two-dimensional group-by**: a value
    // statistic groups by every position the mark does not measure with, and
    // reduces the column named on the one it does. A flat `bar` measures with `y`,
    // so it groups by `x`; a `bar` in `space` measures with `z`, so it groups by
    // the pair; a `zone` measures with `color`, so it groups by the pair too.
    //
    // This replaces the refusal that stood here. That refusal was right about the
    // bug — left to run, `bar * mean + x + y + z + space()` grouped by `x` and
    // wrote to `y`, one column per *row* piled at each slot with a height that was
    // whichever row painted last, every part legal and nothing said — and wrong
    // about the reason, which it gave as "there is no channel left to name the
    // column". `z` names it, exactly as `y` does flat.
    let cube = |t: Transform| PlotSpec::new().data("t").x("continent").y("region").z("life")
        .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        .layer(Layer::new(Mark::Bar).transform(t));
    let tile = |t: Transform| PlotSpec::new().data("t").x("continent").y("region")
        .layer(Layer::new(Mark::Zone).transform(t).encode(Channel::Color, "life"));
    for t in [Transform::Mean, Transform::Sum, Transform::Median,
              Transform::Max, Transform::Min] {
        for (what, spec) in [("in the cube", cube(t.clone())), ("on a zone", tile(t.clone()))] {
            let d = check(&spec, &data());
            assert!(!d.iter().any(|x| x.is_fatal()),
                "`{} * {}` {what} must draw: {:?}",
                if what.contains("zone") { "zone" } else { "bar" }, transform_name(&t),
                d.iter().map(|x| x.message.clone()).collect::<Vec<_>>());
        }
    }

    // `smooth` is the one that stays refused, and for its own reason rather than
    // theirs: it *fits* along a domain instead of reducing, and a floor has no
    // left to right — the same thing that keeps `line` and `area` out of the cube.
    let d = check(&cube(Transform::Smooth), &data());
    let hit = d.iter().find(|x| x.message.contains("no left to right"));
    let Some(hit) = hit else {
        panic!("`bar * smooth` in space was not refused: {:?}",
               d.iter().map(|x| x.message.clone()).collect::<Vec<_>>());
    };
    assert_eq!(hit.kind, DiagnosticKind::Illegal);
    // §12: a refusal names what to do instead. Both routes, since which one the
    // reader wants depends on whether they meant to fit or to summarize.
    assert!(hit.message.contains("line * smooth"), "no flat-fit direction: {}", hit.message);
    assert!(hit.message.contains("mean"), "no reduction direction: {}", hit.message);

    // And the flat reading is untouched — the pair reading is a *widening*, so
    // one key must keep meaning what it always did.
    let flat = PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Bar).transform(Transform::Mean));
    assert!(!check(&flat, &data()).iter().any(|x| x.is_fatal()),
        "the flat `bar * mean` must keep drawing");
}

#[test]
fn a_pair_reduction_needs_its_measure_channel_named_and_its_cells_slotted() {
    // What the pair reading has to ask that the one-key reading got for free:
    // flat, `y` is required by the mark, so a value statistic never had to check
    // that its column was named. Here the measurement rides an **optional**
    // channel — `color` on a zone, `z` on a bar — so all three questions are live,
    // and each is fatal rather than warn-then-draw (§12).
    let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

    // 1. No column to reduce, on the mark where that is reachable. A zone's
    //    `color` is optional, so `zone * mean` with nothing bound is a plot
    //    someone can write; a bar's third axis is what *puts it in the cube* at
    //    all, so `bar * mean + space()` with no `z` is a flat plot with two
    //    categorical axes and `check_slot_shape` owns it — asserted below.
    let d = check(&PlotSpec::new().data("t").x("continent").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Mean)), &data());
    let hit = d.iter().find(|x| x.message.contains("nothing says which column"));
    let Some(hit) = hit else { panic!("an unnamed column was accepted: {:?}", msgs(&d)) };
    assert_eq!(hit.kind, DiagnosticKind::Illegal);
    assert!(hit.message.contains("measures by `color`"),
        "the wrong channel was named: {}", hit.message);
    assert!(hit.message.contains("count"), "no count direction: {}", hit.message);

    // The bar's version of the same mistake, refused by the checks that already
    // owned it — and between them they still say to bind `z`, which is §12's
    // whole requirement of a refusal.
    let d = check(&PlotSpec::new().data("t").x("continent").y("region")
        .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        .layer(Layer::new(Mark::Bar).transform(Transform::Mean)), &data());
    assert!(d.iter().any(|x| x.is_fatal()), "a bar measuring nothing was accepted: {:?}", msgs(&d));
    assert!(d.iter().any(|x| x.message.contains("z(<column>)")),
        "nothing pointed at the third axis: {:?}", msgs(&d));

    // 1b. Two transforms cannot both be the cell's measurement. `bin` invents one
    //     and `mean` reduces a named one, and a cell holds a single number — so
    //     composing them is refused rather than one of the two quietly winning.
    let d = check(&PlotSpec::new().data("t").x("continent").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Count)
            .transform(Transform::Mean).encode(Channel::Color, "life")), &data());
    let hit = d.iter().find(|x| x.message.contains("measures each cell twice"));
    let Some(hit) = hit else { panic!("a doubly-measured cell was accepted: {:?}", msgs(&d)) };
    assert_eq!(hit.kind, DiagnosticKind::Illegal);
    assert!(hit.message.contains("zone * count") && hit.message.contains("color(<column>)"),
        "both ways out must be named: {}", hit.message);

    // 1c. `bin` is the exception, and it is the whole composition ruling: it
    //     supplies an *extent* rather than a measurement, so it composes. The
    //     two questions above it still apply — the column must be named and must
    //     be a number — and only *both positions categorical* is answered
    //     differently, because a cut axis owns cells too.
    let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Zone).transform(Transform::Bin)
            .transform(Transform::Mean).encode(Channel::Color, "life")), &data());
    assert!(!d.iter().any(Diagnostic::is_fatal),
        "a cut cell reduced by a named statistic is the summary heatmap: {:?}", msgs(&d));

    // 2. A column that is not a number. There is no mean of a category, and
    //    answering it in the transform would be the warn-then-draw §12 forbids.
    let d = check(&PlotSpec::new().data("t").x("continent").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Mean)
            .encode(Channel::Color, "region")), &data());
    assert!(d.iter().any(|x| x.is_fatal() && x.message.contains("not a numeric column")),
        "a categorical column was reduced: {:?}", msgs(&d));

    // 3. A continuous position. These five **measure without cutting**, so their
    //    cells can only be the slots categories own — a number is a point. The
    //    refusal says so per axis and points at binning where the data lives.
    let d = check(&PlotSpec::new().data("t").x("gdp").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Mean)
            .encode(Channel::Color, "life")), &data());
    let hit = d.iter().find(|x| x.message.contains("owns no cell to summarize into"));
    let Some(hit) = hit else { panic!("a continuous key was accepted: {:?}", msgs(&d)) };
    assert!(hit.message.contains("x(gdp)") && !hit.message.contains("y(region)"),
        "the refusal must name the axis that is loose, and only it: {}", hit.message);
    // And exactly one refusal for the one mistake: `check_zone_extent` defers to
    // this one, the way it already does for a tally.
    assert_eq!(d.iter().filter(|x| x.is_fatal()).count(), 1,
        "one mistake, one refusal: {:?}", msgs(&d));
}

#[test]
fn only_bin_gives_its_measurement_up_when_two_transforms_compose() {
    // **Which transform owns the measurement when two are composed** (spec §5).
    // The answer is *the one that was handed a column*, so a value statistic
    // always keeps it and the synthesizing transform keeps whatever else it
    // supplies — an extent for `bin`, nothing for the other three.
    //
    // Asked on a one-key mark deliberately. Until 2026-07-26 this refusal lived
    // inside the *two-dimensional* group-by, so the identical mistake was fatal
    // on a `zone` and silent on a `bar`: every one-key composition ran, dropped
    // the statistic, and relabeled the axis to the column nobody had read.
    let compose = |t: Transform| {
        check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Bar).transform(t.clone()).transform(Transform::Mean)), &data())
    };

    // `bin` composes: it cuts, and a cut is an extent description.
    let d = compose(Transform::Bin);
    assert!(!d.iter().any(Diagnostic::is_fatal),
        "`bar * bin * mean` is the binned mean profile: {:?}", msgs(&d));

    // `proportion` composes too, and for a different reason from `bin`'s: it
    // does not give a measurement up, it **rescales the one it finds**. This
    // was refused for a day, which made `bar * sum * proportion` — each slot's
    // summed column as a share of the total — a sentence gog could not say.
    let d = compose(Transform::Proportion);
    assert!(!d.iter().any(Diagnostic::is_fatal),
        "`bar * proportion * mean` reads the mean as a share: {:?}", msgs(&d));

    // The other two do not, and each is refused for **its own** reason rather
    // than a shared one — the defect the 2026-07-26 refusal audit went looking
    // for is a message restating a rule that belongs somewhere else.
    for (t, reason) in [
        (Transform::Count,      "supplies only a measurement"),
        (Transform::Density,    "not a bucket holding rows"),
    ] {
        let d = compose(t.clone());
        let hit = d.iter().find(|x| x.message.contains("measures each cell twice"));
        let Some(hit) = hit else {
            panic!("`bar * {} * mean` was accepted: {:?}", transform_name(&t), msgs(&d))
        };
        assert_eq!(hit.kind, DiagnosticKind::Illegal);
        assert!(hit.message.contains(reason),
            "`{}` must be refused for its own reason: {}", transform_name(&t), hit.message);
        // Every refusal names the way *through* as well as the two ways out —
        // §12's whole requirement, and here it is the feature this ruling built.
        assert!(hit.message.contains("bin"),
            "the refusal must point at the transform that does cut: {}", hit.message);
    }

    // Two **synthesizing** transforms are the same contradiction with neither side
    // handed a column, so there is nothing to give way.
    //
    // `proportion` was on this list for one day, and the pair that put it there —
    // `bin * proportion` — is the relative-frequency histogram, now legal and
    // asserted below. The plot that condemned it (twelve equal bars at 1/12, under
    // an axis reading `Count`) was a *sequencing* defect: run in order, `bin`
    // tallied and `proportion` then read the binned frame as its population, where
    // every cell appears exactly once. Refusing the sentence for what the
    // implementation did to it is the mistake this line records.
    for (a, b) in [
        (Transform::Bin,   Transform::Density),
        (Transform::Bin,   Transform::Count),
        (Transform::Count, Transform::Density),
    ] {
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Bar).transform(a.clone()).transform(b.clone())), &data());
        let hit = d.iter().find(|x| x.message.contains("measures each cell twice"));
        let Some(hit) = hit else {
            panic!("`bar * {} * {}` was accepted: {:?}",
                transform_name(&a), transform_name(&b), msgs(&d))
        };
        assert_eq!(hit.kind, DiagnosticKind::Illegal);
        assert!(hit.message.contains("neither was handed a column"),
            "two invented measurements are refused for *that* reason: {}", hit.message);
    }

    // `smooth` is refused against all four, `bin` included, and for a reason none
    // of them share: it fits a curve and already averages locally as it goes, so
    // cutting the domain first buys it nothing it was not doing.
    for t in [Transform::Bin, Transform::Count, Transform::Density] {
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Line).transform(t.clone()).transform(Transform::Smooth)), &data());
        let hit = d.iter().find(|x| x.message.contains("asks one question twice"));
        let Some(hit) = hit else {
            panic!("`line * {} * smooth` was accepted: {:?}", transform_name(&t), msgs(&d))
        };
        assert_eq!(hit.kind, DiagnosticKind::Illegal);
        assert!(hit.message.contains("bin * mean"),
            "…and points at the statistic that does summarize a cell: {}", hit.message);
    }

    // **A normalizer composes with everything that leaves one number per cell.**
    // The three sentences the day-old refusal had made unsayable, and the reason
    // `proportion` had to leave the synthesizing class rather than gain an
    // exception inside it (Law 2).
    // Only the summary reads `y`; a tally draws its own number there, and a column
    // named on it would be refused as read by nothing.
    for (ts, y) in [
        (vec![Transform::Bin, Transform::Proportion], None),        // relative-frequency histogram
        (vec![Transform::Count, Transform::Proportion], None),      // the tally, spelled out
        (vec![Transform::Sum, Transform::Proportion], Some("life")), // each slot's share of a total
    ] {
        let mut layer = Layer::new(Mark::Bar);
        for t in &ts { layer = layer.transform(t.clone()); }
        let spec = PlotSpec::new().data("t").x("gdp");
        let spec = match y { Some(y) => spec.y(y), None => spec };
        let d = check(&spec.layer(layer), &data());
        assert!(!d.iter().any(Diagnostic::is_fatal),
            "`bar * {}` must draw: {:?}",
            ts.iter().map(transform_name).collect::<Vec<_>>().join(" * "), msgs(&d));
    }

    // …and refuses exactly what it cannot rescale: no total to divide by
    // (`density`, `smooth`) or two numbers per cell instead of one (the pairs).
    // Each says so in its own words rather than sharing a message.
    for (t, reason) in [
        (Transform::Density,    "already integrates to 1"),
        (Transform::Smooth,     "no total to divide by"),
        (Transform::Range,      "share of a span"),
        (Transform::Confidence, "share of a span"),
    ] {
        let mark = if matches!(t, Transform::Range | Transform::Confidence | Transform::Deviation) {
            Mark::Interval
        } else {
            Mark::Line
        };
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(mark).transform(t.clone()).transform(Transform::Proportion)),
            &data());
        let hit = d.iter().find(|x| x.message.contains(reason));
        assert!(hit.is_some_and(|h| h.kind == DiagnosticKind::Illegal),
            "`{} * proportion` must be refused for its own reason: {:?}",
            transform_name(&t), msgs(&d));
    }
}

#[test]
fn a_plot_is_drawn_in_one_space_not_two() {
    // `polar()` bends the plane the plot already has; `z` adds a dimension to
    // it. A plot is drawn in one coordinate space, so asking for both is
    // refused as a rule (Illegal since 2026-09-29, as the spec records it)
    // rather than one of the two quietly winning.
    let spec = PlotSpec::new().data("t").x("gdp").y("life").z("pop")
        .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
        .layer(Layer::new(Mark::Point));
    let d = check(&spec, &data());
    assert!(
        d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("one coordinate space")),
        "polar + z was not refused: {:?}",
        d.iter().map(|x| x.message.clone()).collect::<Vec<_>>()
    );
}

/// **An Unsupported refusal says, in those words, that the grammar allows the
/// sentence and this engine does not draw it, and no refusal promises a
/// future** (ruled 2026-09-28). The kind is never printed, so the wording
/// is how a reader tells the two kinds apart; "yet", "for now" and "wait for
/// the feature" promised a schedule. The three refusals that say a plot is
/// drawn in one coordinate space state a rule, and are Illegal.
#[test]
fn an_unsupported_refusal_says_valid_grammar_and_none_promises_a_future() {
    let mut log_z = base().z("value").layer(Layer::new(Mark::Point));
    log_z.z = log_z.z.map(|c| ChannelDef { scale: Some(ScaleType::Log), ..c });
    let unsupported = [
        base().z("value").layer(Layer::new(Mark::Zone)),
        log_z,
        base().layer(Layer::new(Mark::Text).encode(Channel::Label, "continent")
            .transform(Transform::Repel)).brush(crate::ir::BrushDef::new("gdp").at(1.0, 2.0)),
        PlotSpec::new().data("t").layer(Layer::new(Mark::Rule).encode(Channel::Z, "gdp")),
    ];
    let one_space = [
        CoordSpace::Polar(crate::ir::PolarView::default()),
        CoordSpace::Map(crate::ir::MapView::default()),
        CoordSpace::Nest,
    ].map(|coord| base().z("value").coord(coord).layer(Layer::new(Mark::Point)));
    // The space rules: a map or a globe spends both positions on the place, a
    // globe's `z` is its radius, and a packing has no positions at all. Rules,
    // so Illegal, and never in the Unsupported words (2026-09-29).
    let space_rules = [
        base().coord(CoordSpace::Map(crate::ir::MapView::default()))
            .layer(Layer::new(Mark::Bar)),
        base().coord(CoordSpace::Nest).layer(Layer::new(Mark::Line)),
        base().z("value").coord(CoordSpace::Globe(crate::ir::GlobeView::default()))
            .layer(Layer::new(Mark::Point)),
        base().coord(CoordSpace::Globe(crate::ir::GlobeView::default()))
            .layer(Layer::new(Mark::Bar)),
    ];
    for spec in &space_rules {
        let d = check(spec, &data());
        assert!(!d.is_empty(), "a space rule must refuse: {:?}", spec.coord);
        for x in &d {
            assert!(!x.message.contains("valid grammar"),
                "a rule must not call itself valid grammar: {}", x.message);
        }
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal), "{:?}", msgs(&d));
    }
    let future = ["yet", "for now", "so far", "today", "wait for", "or wait"];
    for spec in &unsupported {
        let d = check(spec, &data());
        let u: Vec<&Diagnostic> =
            d.iter().filter(|x| x.kind == DiagnosticKind::Unsupported).collect();
        assert!(!u.is_empty(), "expected an Unsupported refusal: {:?}", msgs(&d));
        for x in u {
            assert!(x.message.contains("valid grammar") && x.message.contains("does not draw"),
                "an Unsupported refusal must say it in the shared words: {}", x.message);
        }
    }
    for spec in &one_space {
        let d = check(spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("one coordinate space")), "{:?}", msgs(&d));
    }
    for spec in unsupported.iter().chain(one_space.iter()) {
        for x in check(spec, &data()) {
            let words: Vec<&str> = x.message
                .split(|ch: char| !ch.is_alphanumeric() && ch != ' ')
                .collect();
            let text = words.join(" ");
            for f in future {
                assert!(!format!(" {text} ").contains(&format!(" {f} ")),
                    "a refusal promises a future ({f}): {}", x.message);
            }
        }
    }
}

// The one relaxation of Law 7's minimum syllable (spec §15): a `bar` whose
// split is its segmentation has one slot and needs no position axis. These pin
// both halves — that it is allowed where it means something, and refused where
// it does not, which is what keeps it a rule rather than an exception.

#[test]
fn a_bar_may_drop_its_position_when_a_split_divides_the_one_slot() {
    let fatal = |d: &[Diagnostic]| d.iter().any(|x| x.is_fatal());
    let bar = |split: bool| {
        let mut l = Layer::new(Mark::Bar).transform(Transform::Count).transform(Transform::Stack);
        if split { l = l.encode(Channel::Color, "continent"); }
        PlotSpec::new().data("t").layer(l)
    };
    assert!(!fatal(&check(&bar(true), &data())), "a split bar with no x must be legal: {:?}",
        check(&bar(true), &data()).iter().map(|d| d.message.clone()).collect::<Vec<_>>());

    // Without the split there is nothing to divide the slot, so every row would
    // pile into one place with nothing to tell them apart. Still refused, and
    // the refusal still names the missing position.
    let d = check(&bar(false), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("needs `x()`")),
        "an unsplit bar with no x must still be refused: {:?}",
        d.iter().map(|x| x.message.clone()).collect::<Vec<_>>());
}

#[test]
fn exactly_three_marks_may_drop_a_position() {
    // Law 7's floor has not moved; the point of this test is that the list of
    // marks standing off it is closed and each member has a reason stated once
    // in the engine. `bar` drops `x` when a split divides its one slot
    // (`bar_divides_one_slot`). `rule` is *placed* by one position and spans
    // the other (`rule_axis`). `zone` is *bounded* on one axis and spans the
    // other (`check_bounds`) — the same relaxation as `rule`'s, one dimension
    // up, which is why the two arrived together rather than the second being
    // an exception the first made room for. Every other drawable mark keeps
    // both positions.
    let fatal = |d: &[Diagnostic]| d.iter().any(|x| x.is_fatal());
    for m in &ALL_MARKS {
        if matches!(m, Mark::Bar | Mark::Rule | Mark::Zone) || !is_drawable(m) {
            continue;
        }
        let spec = PlotSpec::new().data("t").y("life")
            .layer(Layer::new(m.clone()).encode(Channel::Color, "continent"));
        assert!(fatal(&check(&spec, &data())), "{m:?} drew with no x");
    }
    // The relaxed three really are relaxed — asserted here rather than only in
    // their own tests, so this stays a statement about the whole set.
    let mut d2 = data();
    d2.insert("z".to_string(),
        DataFrame::new().with_float("lo", vec![1.0]).with_float("hi", vec![2.0]));
    for spec in [
        PlotSpec::new().data("t").y("life").layer(
            Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "continent")),
        PlotSpec::new().data("t").y("life").layer(Layer::new(Mark::Rule)),
        PlotSpec::new().data("t").y("life").layer(
            Layer::new(Mark::Zone).data("z").bounds("lo", "hi")),
    ] {
        assert!(!fatal(&check(&spec, &d2)), "a relaxed mark was refused: {:?}",
            msgs(&check(&spec, &d2)));
    }
}

// -----------------------------------------------------------------------
// Per-layer positions — one axis, its own column (spec §8)
// -----------------------------------------------------------------------

/// A second table naming its own position columns. This is the sentence spec
/// §8 asserted for several sessions while it could not run: `x(at)` resolved
/// against the *plot's* table and was refused as a missing column, so the
/// section's own worked example had to reuse the base table's names.
#[test]
fn a_second_table_may_name_its_own_position_columns() {
    let mut d = data();
    d.insert(
        "notes".to_string(),
        DataFrame::new()
            .with_float("at", vec![2.0])
            .with_float("val", vec![5.0])
            .with_str("what", vec!["here".into()]),
    );
    let spec = base().layer(Layer::new(Mark::Point)).layer(
        Layer::new(Mark::Text)
            .data("notes")
            .encode(Channel::X, "at")
            .encode(Channel::Y, "val")
            .encode(Channel::Label, "what"),
    );
    let diags = check(&spec, &d);
    assert!(
        diags.iter().all(|x| !x.is_fatal()),
        "the note table's own columns were refused: {:?}",
        msgs(&diags)
    );
}

/// The plot's column is *not* looked for in a layer that named its own —
/// which is the precise failure the defect was. A note table holds `at`, not
/// `gdp`, and asking it for `gdp` is what produced "column not in the data".
#[test]
fn the_plots_column_is_not_demanded_of_a_layer_that_named_its_own() {
    let mut d = data();
    d.insert(
        "notes".to_string(),
        DataFrame::new().with_float("at", vec![2.0]).with_float("val", vec![5.0]),
    );
    let spec = base().layer(Layer::new(Mark::Point)).layer(
        Layer::new(Mark::Point)
            .data("notes")
            .encode(Channel::X, "at")
            .encode(Channel::Y, "val"),
    );
    let diags = check(&spec, &d);
    assert!(
        !msgs(&diags).iter().any(|m| m.contains("gdp")),
        "the plot's column was demanded of the note table: {:?}",
        msgs(&diags)
    );
}

/// A layer's own position is still checked — against *its own* table. The
/// feature moves which table answers the question, never whether it is asked.
#[test]
fn a_layer_position_naming_a_missing_column_is_still_refused() {
    let mut d = data();
    d.insert("notes".to_string(), DataFrame::new().with_float("at", vec![2.0]));
    let spec = base().layer(
        Layer::new(Mark::Point).data("notes").encode(Channel::X, "nope"),
    );
    let diags = check(&spec, &d);
    assert!(
        diags.iter().any(|x| x.is_fatal()),
        "a missing column on a layer position must still be fatal"
    );
    assert!(msgs(&diags).iter().any(|m| m.contains("nope")));
}

/// One axis, its own column — **never its own scale**. The line between the
/// thing §8 owes and the thing §18 refuses is exactly one field wide, so it
/// gets its own test.
#[test]
fn a_layer_may_not_bring_its_own_scale() {
    let mut d = data();
    d.insert("notes".to_string(), DataFrame::new().with_float("at", vec![2.0]));
    let spec = base().layer(
        Layer::new(Mark::Point)
            .data("notes")
            .encode_scaled(Channel::X, "at", ScaleType::Log),
    );
    let diags = check(&spec, &d);
    assert!(
        diags.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("share one x axis")),
        "a layer's own scale is a second axis and must be refused: {:?}",
        msgs(&diags)
    );
}

/// The same column with the same scale as the axis is not a second scale, so
/// it is not refused. Guards the check against firing on a plot that merely
/// repeats itself — Law 8, never forbid the ugly-but-legal.
#[test]
fn a_layer_repeating_the_axis_scale_is_legal() {
    let mut spec = PlotSpec::new().data("t").y("life").layer(
        Layer::new(Mark::Point).encode_scaled(Channel::X, "gdp", ScaleType::Log),
    );
    spec.x = Some(ChannelDef::field("gdp").with_scale(ScaleType::Log));
    let diags = check(&spec, &data());
    assert!(
        !msgs(&diags).iter().any(|m| m.contains("share one x axis")),
        "repeating the axis's own scale is not a second scale: {:?}",
        msgs(&diags)
    );
}

/// A layer that states **nothing** inherits the axis, and inheriting is not
/// disagreeing. Until 2026-07-27 the check compared `Option` against `Option`,
/// so any plot-scoped `limits`/`scale`/`tick_count` refused *every* layer that
/// named its own position column — the sentence the diagnostic itself calls
/// fine ("`y(v)` on its own"). Found while drawing a sunburst, where the
/// radial domain is what puts the hole in the middle and a second layer is
/// what labels the rings, so the two could not be said together.
#[test]
fn a_layer_naming_its_column_inherits_the_axis_scale_properties() {
    for axis in [
        ChannelDef::field("gdp").with_limits(Some(0.0), Some(10.0)),
        ChannelDef::field("gdp").with_scale(ScaleType::Log),
        ChannelDef::field("gdp").with_tick_count(8),
    ] {
        let mut spec = PlotSpec::new().data("t").y("life").layer(
            // The layer names the column and nothing else — no scale, no
            // limits, no tick count.
            Layer::new(Mark::Point).encode(Channel::X, "gdp"),
        );
        spec.x = Some(axis.clone());
        let diags = check(&spec, &data());
        assert!(
            !msgs(&diags).iter().any(|m| m.contains("share one x axis")),
            "a layer stating no scale property of its own inherits the axis's, \
             and must not be refused for one it never asked for: axis {axis:?} \
             gave {:?}",
            msgs(&diags)
        );
    }
}

// -----------------------------------------------------------------------
// `rule` — one position from the data, the other extent from the panel
// -----------------------------------------------------------------------

/// The payoff §18 predicted when it filed the ambiguous case as "the sharpest
/// live cost of the parked per-layer position bindings": a thresholds table
/// holding *both* of the plot's columns was refused for being unreadable, and
/// a layer that says which axis it means now answers it outright.
#[test]
fn a_rule_can_say_which_axis_it_means_instead_of_being_refused() {
    let mut d = data();
    d.insert(
        "both".to_string(),
        DataFrame::new().with_float("gdp", vec![2.5]).with_float("life", vec![5.5]),
    );
    let ambiguous = base().layer(Layer::new(Mark::Point))
        .layer(Layer::new(Mark::Rule).data("both"));
    assert!(
        check(&ambiguous, &d).iter().any(|x| x.is_fatal()),
        "a table answering both axes still has nothing to say which is meant"
    );

    let said = base().layer(Layer::new(Mark::Point)).layer(
        Layer::new(Mark::Rule).data("both").encode(Channel::X, "gdp"),
    );
    let diags = check(&said, &d);
    assert!(
        diags.iter().all(|x| !x.is_fatal()),
        "`rule + x(gdp)` names its axis and must be accepted: {:?}",
        msgs(&diags)
    );
    assert_eq!(rule_axis(&said, &d["both"], &said.layers[1]), Some(Channel::X));
}

#[test]
fn a_rule_is_placed_by_whichever_axis_its_own_table_answers() {
    // The mark's whole claim, as one differential assertion over both axes:
    // the *same* layer, against the *same* plot, lands on x or on y purely
    // according to which position column the rule's table holds. Written as
    // one test over both so it cannot pass by describing each separately.
    let cuts = |col: &str| {
        let mut m = data();
        m.insert("cuts".to_string(), DataFrame::new().with_float(col, vec![2.5]));
        m
    };
    let spec = base().layer(Layer::new(Mark::Point))
        .layer(Layer::new(Mark::Rule).data("cuts"));

    for (col, want) in [("gdp", Channel::X), ("life", Channel::Y)] {
        let d = check(&spec, &cuts(col));
        assert!(d.iter().all(|x| !x.is_fatal()), "a rule over `{col}` was refused: {:?}", msgs(&d));
        let df = &cuts(col)["cuts"];
        let rule_layer = &spec.layers[1];
        assert_eq!(rule_axis(&spec, df, rule_layer), Some(want.clone()),
            "a table holding only `{col}` must place the rule on {want:?}");
    }
}

#[test]
fn a_rule_refuses_the_two_shapes_that_do_not_say_which_axis() {
    // Neither column, and both columns. The second is the one that matters:
    // it is the rug written straight over the plot's own table, it is
    // genuinely ambiguous, and guessing an axis there would teach a rule that
    // is not real — `check_slot_shape`'s refusal of a two-categorical bar,
    // one mark over.
    let spec = base().layer(Layer::new(Mark::Point)).layer(Layer::new(Mark::Rule).data("r"));
    let with = |df: DataFrame| {
        let mut m = data();
        m.insert("r".to_string(), df);
        check(&spec, &m)
    };

    let neither = with(DataFrame::new().with_float("other", vec![1.0]));
    assert!(neither.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("needs a position")),
        "a table answering neither axis must be refused: {:?}", msgs(&neither));

    let both = with(DataFrame::new().with_float("gdp", vec![1.0]).with_float("life", vec![2.0]));
    assert!(both.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("both")),
        "a table answering both axes must be refused: {:?}", msgs(&both));
    // And the refusal must direct, not just decline: the fix is a smaller
    // table, so the message has to name it.
    assert!(both.iter().any(|x| x.message.contains("its own table")),
        "the ambiguity refusal must say how to break the tie: {:?}", msgs(&both));
}

#[test]
fn the_axis_a_rule_spans_is_not_reported_as_a_missing_column() {
    // A thresholds table holding *only* the threshold is the designed form
    // (§18), so the plot's other position column is *expected* to be absent
    // from it. Reporting that as a missing column would make the ordinary
    // sentence fail — which is the bug this suppression exists to prevent.
    let mut m = data();
    m.insert("cuts".to_string(), DataFrame::new().with_float("life", vec![5.0]));
    let d = check(
        &base().layer(Layer::new(Mark::Point)).layer(Layer::new(Mark::Rule).data("cuts")),
        &m,
    );
    assert!(!d.iter().any(|x| x.message.contains("not in the data")),
        "the spanned axis must not be reported missing: {:?}", msgs(&d));
}

#[test]
fn reach_is_rule_only_and_takes_one_of_two_values() {
    let styled = |m: Mark, v: &str| {
        let mut layer = Layer::new(m);
        layer.style.reach = Some(v.to_string());
        base().layer(layer)
    };
    // The mark half, read off `mark_takes_setting` so the grid agrees.
    let d = check(&styled(Mark::Point, "panel"), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("has no such axis")),
        "reach on a two-position mark must be refused: {:?}", msgs(&d));

    // The value half, read off `REACHES`.
    for v in REACHES {
        let d = check(&styled(Mark::Rule, v), &data());
        assert!(!d.iter().any(|x| x.message.contains("is not a reach")), "`{v}` must be legal");
    }
    let d = check(&styled(Mark::Rule, "middle"), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("is not a reach")),
        "an invented reach must be refused: {:?}", msgs(&d));
}

#[test]
fn every_none_cell_of_the_transform_grid_actually_refuses() {
    // The grid is generated from `mark_takes_transform`, so a `—` cell is a
    // promise the book prints: this combination does not exist. Nothing made
    // the engine keep it. `rule * mean` was a `—` that *rendered* — two
    // warnings about a missing `x` and an empty panel — which is warn-then-
    // draw, the silent drop §12 forbids, and the same blind spot as an
    // `#| error: true` chunk that stops erroring.
    //
    // What is required is that the layer is **refused**, not that the message
    // names the transform. `interval * bin` is caught by the missing-range
    // refusal instead, and that message is the more useful one — it names
    // what the mark actually needs. The wording of each refusal is pinned by
    // the mark's own test; this one pins that a `—` cell never draws.
    let mut gaps: Vec<String> = Vec::new();
    for m in &ALL_MARKS {
        if !is_drawable(m) {
            continue;
        }
        for t in USER_TRANSFORMS {
            if mark_takes_transform(m, &t) != TransformLegality::None {
                continue;
            }
            let mut layer = Layer::new(m.clone()).transform(t.clone());
            // Give each mark the rest of its minimum syllable, so what is
            // left to complain about is the transform alone.
            if *m == Mark::Text { layer = layer.encode(Channel::Label, "continent"); }
            if is_collision_modifier(&t) { layer = layer.encode(Channel::Color, "continent"); }
            let d = check(&base().layer(layer), &data());
            if !d.iter().any(|x| x.is_fatal()) {
                gaps.push(format!("{} * {}", mark_name(m), transform_name(&t)));
            }
        }
    }
    assert!(gaps.is_empty(), "the grid says these do not exist, but nothing refuses them: {gaps:?}");
}

#[test]
fn a_zone_takes_the_seven_transforms_that_give_it_a_cell() {
    // The mark's Mark × Transform row, pinned from both sides, and the line it
    // draws is *what each transform supplies*. `bounds` names the sides;
    // `bin`/`density` cut them and measure inside; `count`/`proportion` measure
    // into the cells the categories already are. Those five read `Required`
    // because any one satisfies the mark — the same "● means one of these" the
    // three pair transforms on `interval` already mean.
    //
    // **The five reductions `Combine` rather than bound**, which is the
    // two-dimensional group-by (spec §5). They measure without cutting, so they
    // answer *what is in this cell* and never *where are my cells* — the
    // categorical positions answer that. Their column is named by the channel the
    // mark measures with, which for a zone is `color`: the claim they were once
    // refused under (no channel left to name it) was false on its own terms.
    //
    // **`partition` is the sixth** (2026-07-27), and it belongs to the first
    // group without qualification: it names the sides the way `bounds` does,
    // and it measures inside them the way `bin` does, being the only one of the
    // six that does both. What it adds to the row is not a new kind of cell but
    // a new *source* of the rectangular one — cut from a tree instead of from a
    // mesh or handed over column by column.
    //
    // **`flow` is the seventh** (2026-08-17), `partition`'s pattern one family
    // over: it lays each stage's slots itself — sides on the slotted axis,
    // stacked intervals on the measured one — so like the others any one of
    // the seven satisfies the mark on its own.
    //
    // **`cluster` `Combines` and supplies neither** (2026-08-17): a tile
    // plot's cells come from its categorical axes and its measurement from
    // `color`, and what the cluster adds is the *order* of the leaf axis's
    // slots — the seriated heatmap. First of a third kind on this row, so
    // it is pinned beside the reductions rather than folded into a group.
    for t in USER_TRANSFORMS {
        let expect = if matches!(t, Transform::Bounds | Transform::Bin | Transform::Density
                                    | Transform::Count | Transform::Proportion
                                    | Transform::Partition | Transform::Flow) {
            TransformLegality::Required
        } else if t == Transform::Cluster {
            TransformLegality::Combines
        } else if crate::transform::has_reduction(std::slice::from_ref(&t)) {
            TransformLegality::Combines
        } else {
            TransformLegality::None
        };
        assert_eq!(mark_takes_transform(&Mark::Zone, &t), expect,
            "`zone * {}` is wrong in the grid", transform_name(&t));
    }
    // The flow family's refusals, each with its direction (spec §15, the flow
    // entry). The grid row itself is pinned in the loop above; these pin the
    // sentence-level rules a reader will actually hit.
    {
        // A mark with no reading is sent to the three that have one.
        let d = check(&PlotSpec::new().data("t")
            .layer(Layer::new(Mark::Bar).flow(&["continent", "region"])), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("ribbon * flow") && x.message.contains("zone * flow")
            && x.message.contains("label(name)")),
            "a bar under flow is directed to the three readers: {:?}", msgs(&d));

        // One column has no between.
        let d = check(&PlotSpec::new().data("t")
            .layer(Layer::new(Mark::Ribbon).flow(&["continent"])), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("at least two stage columns")),
            "one stage is refused: {:?}", msgs(&d));

        // A categorical `x` has nothing left to say under a transform that
        // draws its own domain, and the refusal names both directions.
        let d = check(&PlotSpec::new().data("t").x("continent")
            .layer(Layer::new(Mark::Ribbon).flow(&["continent", "region"])), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("reorder `flow(...)`'s arguments")
            && x.message.contains("top to bottom")),
            "a category on x under flow is refused with direction: {:?}", msgs(&d));

        // A numeric `x` with no `y` is the count, and the flow runs top to
        // bottom (2026-10-02): orientation read off the bindings.
        let d = check(&PlotSpec::new().data("t").x("gdp")
            .layer(Layer::new(Mark::Ribbon).flow(&["continent", "region"])), &data());
        assert!(!d.iter().any(|x| x.is_fatal()), "x(gdp) runs the flow down: {:?}", msgs(&d));

        // The count on both axes is two weights for one path.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Ribbon).flow(&["continent", "region"])), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("both `x(gdp)` and `y(life)` are bound")),
            "{:?}", msgs(&d));

        // A band's color from a number is refused: a band carries many rows,
        // and a number differs among them. A column of categories that is not
        // a stage splits the band instead (2026-10-03).
        let mut l = Layer::new(Mark::Ribbon).flow(&["continent", "region"]);
        l.encodings.insert(Channel::Color, ChannelDef::field("gdp"));
        let d = check(&PlotSpec::new().data("t").layer(l), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("needs a categorical")),
            "a numeric color on the bands is refused: {:?}", msgs(&d));
        assert_eq!(d.iter().filter(|x| x.is_fatal()).count(), 1, "said once: {:?}", msgs(&d));

        // A column of categories that is not a stage splits each band, so it
        // is accepted on the bands (2026-10-03).
        let three = HashMap::from([("t".to_string(), DataFrame::new()
            .with_str("a", vec!["p".into(), "p".into(), "q".into()])
            .with_str("b", vec!["u".into(), "v".into(), "u".into()])
            .with_str("c", vec!["x".into(), "y".into(), "x".into()])
            .with_float("n", vec![1.0, 2.0, 3.0]))]);
        let mut l = Layer::new(Mark::Ribbon).flow(&["a", "b"]);
        l.encodings.insert(Channel::Color, ChannelDef::field("c"));
        let d = check(&PlotSpec::new().data("t").y("n").layer(l), &three);
        assert!(!d.iter().any(|x| x.is_fatal()),
            "a color of categories splits the bands: {:?}", msgs(&d));

        // The slots take their paint from `style()`, never from a mapping.
        let mut l = Layer::new(Mark::Zone).flow(&["continent", "region"]);
        l.encodings.insert(Channel::Color, ChannelDef::field("continent"));
        let d = check(&PlotSpec::new().data("t").layer(l), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("does not combine")),
            "a mapped color on the slots is refused: {:?}", msgs(&d));

        // Polar is the chord diagram and refuses as unbuilt, not as wrong.
        let d = check(&PlotSpec::new().data("t")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(Layer::new(Mark::Ribbon).flow(&["continent", "region"])), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Unsupported
            && x.message.contains("chord")),
            "flow in polar is unsupported toward the chord: {:?}", msgs(&d));

        // And the well-formed sentence passes with nothing bound at all:
        // the transform supplies both positions (Law 7 stands down).
        let d = check(&PlotSpec::new().data("t")
            .layer(Layer::new(Mark::Ribbon).flow(&["continent", "region"])), &data());
        assert!(d.iter().all(|x| x.kind != DiagnosticKind::Illegal
            && x.kind != DiagnosticKind::Unsupported),
            "a bare ribbon * flow is a complete syllable: {:?}", msgs(&d));

        // A stage named twice fell on the first, adjacent or not, in silence.
        for stages in [["continent", "continent", "region"], ["continent", "region", "continent"]] {
            for mark in [Mark::Zone, Mark::Ribbon, Mark::Text] {
                let d = check(&PlotSpec::new().data("t")
                    .layer(Layer::new(mark.clone()).flow(&stages)), &data());
                assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                    && x.message.contains("`continent` is named twice in `flow()`")),
                    "{mark:?} * flow{stages:?} was not refused: {:?}", msgs(&d));
            }
        }
    }

    // The network family's refusals, each with its direction (spec §15, the
    // network entry). The grid rows are pinned by the generated matrices;
    // these pin the sentence-level rules a reader will actually hit.
    {
        let net = || CoordSpace::Network(crate::ir::NetworkView::default());

        // A mark with no reading is sent to the three that have one.
        let d = check(&PlotSpec::new().data("t").coord(net())
            .layer(Layer::new(Mark::Bar).layout("continent", "region")), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("edge * layout") && x.message.contains("label(name)")),
            "a bar under layout is directed to the three readers: {:?}", msgs(&d));

        // A layout outside the network is refused toward it.
        let d = check(&PlotSpec::new().data("t")
            .layer(Layer::new(Mark::Edge).layout("continent", "region")), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("+ network()")),
            "a layout outside the space is sent to network(): {:?}", msgs(&d));

        // An edge with no layout has nothing to draw, in every space. It drew
        // an empty panel with made-up axes and said nothing.
        let bare = "an `edge` with no `layout` has nothing to draw";
        let cube = CoordSpace::Space(crate::ir::SpaceView::default());
        for coord in [CoordSpace::default(), cube] {
            let d = check(&PlotSpec::new().data("t").coord(coord.clone())
                .channel(Channel::Color, "continent")
                .layer(Layer::new(Mark::Edge)), &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains(bare)
                && x.message.contains("edge * layout(<from>, <to>) + network()")),
                "a bare edge in {coord:?} was not refused: {:?}", msgs(&d));
        }
        // In a network no layer feeds, the network's own refusal says it, once.
        let d = check(&PlotSpec::new().data("t").coord(net())
            .layer(Layer::new(Mark::Edge)), &data());
        assert!(d.iter().any(|x| x.message.contains("nothing here carries one"))
                && !d.iter().any(|x| x.message.contains(bare)),
                "a bare edge in an unfed network is refused once: {:?}", msgs(&d));
        // In a network another layer feeds, the bare edge is still refused.
        let d = check(&PlotSpec::new().data("t").coord(net())
            .layer(Layer::new(Mark::Point).layout("continent", "region"))
            .layer(Layer::new(Mark::Edge)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains(bare)),
            "a bare edge beside a fed network was not refused: {:?}", msgs(&d));

        // A bound position has nothing left to say; the cube is the space's.
        let d = check(&PlotSpec::new().data("t").coord(net()).x("gdp")
            .layer(Layer::new(Mark::Edge).layout("continent", "region")), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("network(turn = 30, tilt = 25)")),
            "a bound x under layout is refused toward the view: {:?}", msgs(&d));

        // The space with no layout in it spells the syllable.
        let d = check(&PlotSpec::new().data("t").coord(net())
            .layer(Layer::new(Mark::Point)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("edge * layout(from, to) + network()")),
            "an empty network names the syllable: {:?}", msgs(&d));

        // A node aesthetic must name what a node has.
        let mut l = Layer::new(Mark::Point).layout("continent", "region");
        l.encodings.insert(Channel::Color, ChannelDef::field("gdp"));
        let d = check(&PlotSpec::new().data("t").coord(net()).layer(l), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("`name`") && x.message.contains("`degree`")),
            "a node maps only what a node has: {:?}", msgs(&d));

        // And so must its text: an edge column drew an empty `text` layer,
        // silently, because `label` was left out of the question.
        let named = |field: &str| {
            let mut l = Layer::new(Mark::Text).layout("continent", "region");
            l.encodings.insert(Channel::Label, ChannelDef::field(field));
            check(&PlotSpec::new().data("t").coord(net()).layer(l), &data())
        };
        let d = named("continent");
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("`label(continent)`")
            && x.message.contains("`label(name)`")),
            "a label naming an edge column is refused toward `name`: {:?}", msgs(&d));
        for ok in [crate::transform::NODE_NAME, crate::transform::NODE_DEGREE] {
            let d = named(ok);
            assert!(d.iter().all(|x| x.kind != DiagnosticKind::Illegal),
                "`label({ok})` names what a node has: {:?}", msgs(&d));
        }

        // A self-loop is refused with its count and its direction.
        let looped = HashMap::from([("t".to_string(), DataFrame::new()
            .with_str("continent", vec!["Asia".into(), "Asia".into()])
            .with_str("region", vec!["Asia".into(), "North".into()]))]);
        let d = check(&PlotSpec::new().data("t").coord(net())
            .layer(Layer::new(Mark::Edge).layout("continent", "region")), &looped);
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("connect a node to itself")),
            "a self-loop is refused: {:?}", msgs(&d));

        // And the well-formed syllable passes with nothing bound at all.
        let d = check(&PlotSpec::new().data("t").coord(net())
            .layer(Layer::new(Mark::Edge).layout("continent", "region"))
            .layer(Layer::new(Mark::Point).layout("continent", "region")), &data());
        assert!(d.iter().all(|x| x.kind != DiagnosticKind::Illegal
            && x.kind != DiagnosticKind::Unsupported),
            "the network's minimum syllable is complete: {:?}", msgs(&d));

        // Names in the cube are drawn, by the text writer every cube uses.
        let d = check(&PlotSpec::new().data("t")
            .coord(CoordSpace::Network(crate::ir::NetworkView {
                turn: Some(30.0), tilt: Some(25.0) }))
            .layer(Layer::new(Mark::Edge).layout("continent", "region"))
            .layer(Layer::new(Mark::Text).layout("continent", "region")), &data());
        assert!(d.iter().all(|x| !x.is_fatal()),
            "text in the network's cube is drawn: {:?}", msgs(&d));
    }

    // A zone on two *continuous* positions with no transform has nothing to
    // bound it, and the refusal offers every route out of that.
    let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Zone)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("bounds") && x.message.contains("bin")
        && x.message.contains("categorical position")),
        "a bare zone is told every way to give it sides: {:?}", msgs(&d));
}

/// **A flat `zone` with `group` is told every way it gets its sides**, read off
/// the grid: typed out, the list named `bounds`, `bin` and `density` and left out
/// `count`, `proportion`, `partition` and `flow`, four of the seven the grid
/// requires of a zone.
#[test]
fn a_flat_zone_with_group_names_every_transform_that_gives_it_sides() {
    let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Zone).transform(Transform::Bin)
            .encode(Channel::Group, "continent")), &data());
    let said = d.iter().find(|x| x.message.contains("`group(continent)` gathers many rows"))
        .map(|x| x.message.clone()).unwrap_or_else(|| panic!("not refused: {:?}", msgs(&d)));
    let required: Vec<_> = USER_TRANSFORMS.iter()
        .filter(|t| mark_takes_transform(&Mark::Zone, t) == TransformLegality::Required)
        .collect();
    assert_eq!(required.len(), 7, "the zone's required transforms moved: {required:?}");
    for t in required {
        assert!(said.contains(&format!("`{}`", transform_name(t))), "{} missing: {said}",
            transform_name(t));
    }
    assert!(said.contains("a category's slot"), "{said}");
}

// -- partition ---------------------------------------------------------

/// A hierarchy table, laid out the way the book's is: two branches, one of
/// which reaches a third level and one of which does not.
fn tree_data() -> HashMap<String, DataFrame> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("group", s(&["A", "A", "B"]))
            .with_str("item", s(&["p", "q", "r"]))
            .with_str("detail", s(&["deep", "", ""]))
            .with_float("amount", vec![1.0, 2.0, 3.0])
            .with_float("gdp", vec![1.0, 2.0, 3.0]),
    )])
}
fn part(mark: Mark) -> Layer {
    Layer::new(mark).partition(&["group", "item", "detail"])
}

/// **Two readers, which is what makes it a transform.** A transform legal on
/// exactly one mark is that mark's business wearing a transform's name (§5's
/// growth test read from the other end), so the row is pinned from both sides:
/// `zone` takes the four edges and `text` the center, and every other mark is
/// refused toward them.
#[test]
fn a_partition_is_read_by_two_marks_and_refused_by_the_rest() {
    for m in &ALL_MARKS {
        let expect = match m {
            Mark::Zone => TransformLegality::Required,
            Mark::Text => TransformLegality::Combines,
            _ => TransformLegality::None,
        };
        assert_eq!(mark_takes_transform(m, &Transform::Partition), expect,
            "`{} * partition` is wrong in the grid", mark_name(m));
    }
    let d = check(&PlotSpec::new().data("t").x("amount")
        .layer(part(Mark::Bar)), &tree_data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("region")
        && x.message.contains("zone") && x.message.contains("text")),
        "a mark with no region reading is sent to the two that have one: {:?}", msgs(&d));
}

/// A negative weight has no share, region or thickness, so `partition` and
/// `flow` refuse it as `nest()` does. Both transforms clamped it to 0 and drew
/// a plot whose parts no longer summed to the table, with nothing said.
#[test]
fn a_negative_weight_is_refused_by_a_partition_and_a_flow() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let tables: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("a", s(&["p", "p", "q"]))
            .with_str("b", s(&["u", "v", "u"]))
            .with_float("w", vec![3.0, -2.0, 4.0]),
    )]);
    let refused = |spec: PlotSpec, wanted: &str| {
        let out = check(&spec, &tables);
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains(wanted) && d.message.contains("has negative values")),
            "wanted {wanted:?}: {:?}", msgs(&out));
    };
    refused(PlotSpec::new().data("t").x("w").layer(Layer::new(Mark::Zone).partition(&["a", "b"])),
            "`x(w)` weighs each branch");
    refused(PlotSpec::new().data("t").y("w").layer(Layer::new(Mark::Ribbon).flow(&["a", "b"])),
            "`y(w)` weighs each path");
}

/// Rows that share an `x` are joined in x order and zigzag inside it; `area`
/// and `step` say so as `line` does, and only when it happens.
#[test]
fn an_area_or_step_through_rows_at_one_x_says_it_zigzags() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let tables: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("c", s(&["a", "a", "a", "b", "b"]))
            .with_float("v", vec![1.0, 3.0, 2.0, 4.0, 1.0])
            .with_str("k", s(&["p", "q", "r", "p", "q"])),
    )]);
    let said = |layer: Layer| check(&PlotSpec::new().data("t").x("c").y("v").layer(layer), &tables)
        .into_iter().find(|d| d.message.contains("zigzags inside that value"));
    for mark in [Mark::Line, Mark::Area, Mark::Step] {
        let d = said(Layer::new(mark.clone())).expect("rows at one x, joined");
        assert_eq!(d.kind, DiagnosticKind::Assumption);
        assert!(d.message.contains("up to 3 rows"), "{}", d.message);
        assert!(said(Layer::new(mark.clone()).encode(Channel::Color, "k")).is_none());
        assert!(said(Layer::new(mark).transform(Transform::Mean)).is_none());
    }
}

/// One series says nothing, however many rows it has, and neither does one
/// series per panel: the rows are counted where they are joined. `line` said it
/// on every series of more than five points, and once per panel under
/// `facet(country)`. A color of numbers ramps rather than splits, and is said.
#[test]
fn a_line_is_told_it_zigzags_only_where_rows_share_an_x() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let tables: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("year", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
            .with_float("v", vec![1.0, 3.0, 2.0, 4.0, 3.0, 5.0, 2.0, 1.0, 4.0, 3.0, 5.0, 4.0])
            .with_float("w", (0..12).map(f64::from).collect())
            .with_str("country", s(&["a", "a", "a", "a", "a", "a", "b", "b", "b", "b", "b", "b"])),
    )]);
    let said = |spec: PlotSpec| check(&spec, &tables).into_iter()
        .find(|d| d.message.contains("zigzags inside that value"));
    let line = || PlotSpec::new().data("t").x("year").y("v");
    let one = PlotSpec::new().data("t").x("year").y("v")
        .layer(Layer::new(Mark::Line)).layer(Layer::new(Mark::Point));
    assert!(said(one.clone()).is_some(), "two countries' rows at each year are joined");
    let mut faceted = one;
    faceted.facet = Some(crate::ir::FacetSpec { col: Some("country".into()), ..Default::default() });
    assert!(said(faceted).is_none(), "one country per panel is one series");
    let ramped = said(line().layer(Layer::new(Mark::Line).encode(Channel::Color, "w")))
        .expect("a ramp does not split the rows");
    assert!(ramped.message.contains("`color(w)` holds numbers")
            && ramped.message.contains("add `group(<field>)`"), "{}", ramped.message);
    let single: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new().with_float("year", (0..40).map(f64::from).collect())
            .with_float("v", (0..40).map(|i| f64::from(i % 7)).collect()),
    )]);
    assert!(check(&line().layer(Layer::new(Mark::Line)), &single).iter()
            .all(|d| !d.message.contains("zigzags")), "forty years of one series");
}

/// `bounds` draws its axis from its own two columns, so a category named on it
/// was never read, and the band drew one comb through every group. Refused, with
/// the splits that draw a band per group.
#[test]
fn a_category_on_the_axis_bounds_draws_is_refused() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let tables: HashMap<String, DataFrame> = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_float("at", vec![1.0, 2.0, 1.0, 2.0])
            .with_float("zero", vec![0.0; 4])
            .with_float("height", vec![1.0, 3.0, 2.0, 1.0])
            .with_str("g", s(&["a", "a", "b", "b"])),
    )]);
    let ridge = |y: Option<&str>| {
        let mut spec = PlotSpec::new().data("t").x("at")
            .layer(Layer::new(Mark::Ribbon).bounds("zero", "height"));
        if let Some(y) = y { spec = spec.y(y); }
        check(&spec, &tables)
    };
    let out = ridge(Some("g"));
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("`ribbon * bounds` draws its band")
        && d.message.contains("`group(g)`")), "{:?}", msgs(&out));
    assert!(!ridge(None).iter().any(Diagnostic::is_fatal), "{:?}", msgs(&ridge(None)));
}

/// A partition reads its weight from `x` and places its levels itself, so a
/// bound `y` is refused: it was ignored as data and printed as the axis title.
#[test]
fn a_y_under_a_partition_is_refused_toward_x() {
    let spec = PlotSpec::new().data("t").y("amount").layer(part(Mark::Zone));
    let out = check(&spec, &tree_data());
    assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("`y(amount)` has no reading under `partition`")
        && d.message.contains("Write `x(amount)`")), "{:?}", msgs(&out));
    let weighed = PlotSpec::new().data("t").x("amount").layer(part(Mark::Zone));
    assert!(!check(&weighed, &tree_data()).iter().any(Diagnostic::is_fatal));
    // The depth the partition computes is the one `y` it reads: a sunburst
    // states its radial domain there.
    let depth = PlotSpec::new().data("t").x("amount").y("depth").layer(part(Mark::Zone));
    assert!(!check(&depth, &tree_data()).iter().any(Diagnostic::is_fatal),
            "{:?}", msgs(&check(&depth, &tree_data())));
}

/// A partition places its own nodes, so neither position has to be named —
/// Law 7's relaxation in `rule`'s and `zone`'s *constitutive* family rather
/// than the bar's conditional one.
#[test]
fn a_partition_needs_no_position_bound() {
    let d = check(&PlotSpec::new().data("t").layer(part(Mark::Zone)), &tree_data());
    assert!(!d.iter().any(|x| x.is_fatal()),
        "a partition supplies both positions: {:?}", msgs(&d));
    // And `text` reads it too, naming each node where the same computation
    // put it — the second reader, drawn rather than merely allowed.
    let d = check(&PlotSpec::new().data("t")
        .layer(part(Mark::Text).encode(Channel::Label, "name")), &tree_data());
    assert!(!d.iter().any(|x| x.is_fatal()),
        "`text * partition + label(name)` names an output column: {:?}", msgs(&d));
}

/// `partition * proportion` is legal wherever `--rules` says it is: on both
/// readers, nested and crossed, measured or tallied. It divides the measure axis
/// by its total, so neither mark's rules may read it as a tally of cells, which
/// is what refused it as a tile plot with no `y()` and `text` as a summary.
#[test]
fn a_partition_takes_proportion_on_both_of_its_readers() {
    let levels = ["group", "item", "detail"];
    for cross in [false, true] {
        for measured in [true, false] {
            let with = |mark: Mark| {
                let l = Layer::new(mark);
                let l = if cross { l.partition_crossed(&levels) } else { l.partition(&levels) };
                l.transform(Transform::Proportion)
            };
            let spec = PlotSpec::new().data("t");
            let spec = if measured { spec.x("amount") } else { spec };
            let d = check(&spec
                .layer(with(Mark::Zone).encode(Channel::Color, "group"))
                .layer(with(Mark::Text).encode(Channel::Label, "name")), &tree_data());
            assert!(!d.iter().any(|x| x.is_fatal()),
                "cross = {cross}, measured = {measured}: {:?}", msgs(&d));
        }
    }
}

/// The two transforms that fill the scale job divide by different totals, so
/// the refusal names each by what it divides by, whichever is written first,
/// and spells `stack(share = TRUE)` out, since plain `stack` divides nothing.
/// Each way out is a sentence that draws: with nothing else measuring, the
/// pile's share has to count.
#[test]
fn proportion_beside_a_filled_pile_is_refused_toward_two_sentences_that_draw() {
    let share = |l: Layer| Layer {
        stack: Some(crate::ir::StackSpec { share: Some(true), baseline: None }),
        ..l
    };
    for (ts, head) in [
        (vec![Transform::Proportion, Transform::Stack], "`bar * proportion * stack`, with the `stack` asked for shares"),
        (vec![Transform::Stack, Transform::Proportion], "`bar * stack * proportion`, with the `stack` asked for shares"),
    ] {
        let layer = ts.iter().fold(Layer::new(Mark::Bar), |l, t| l.transform(t.clone()));
        let d = check(&PlotSpec::new().data("t").x("group")
            .layer(share(layer).encode(Channel::Color, "item")), &tree_data());
        let m = msgs(&d).join("\n");
        assert!(m.contains(head), "the head names the filled pile: {m}");
        assert!(m.contains("`proportion` divides it into shares of the whole plot \
            and the `stack` into shares of each pile"), "{m}");
        assert!(m.contains("`bar * proportion * stack` for shares of the whole plot")
            && m.contains("`bar * count * stack` with its `share` set to true for shares within each pile")
            && !m.contains("TRUE"), "the ways out draw: {m}");
    }
    // With a measurement of its own, both ways out keep it.
    let layer = Layer::new(Mark::Bar).transform(Transform::Sum)
        .transform(Transform::Proportion).transform(Transform::Stack);
    let d = check(&PlotSpec::new().data("t").x("group").y("amount")
        .layer(share(layer).encode(Channel::Color, "item")), &tree_data());
    let m = msgs(&d).join("\n");
    assert!(m.contains("`bar * sum * proportion * stack`")
        && m.contains("`bar * sum * stack` with its `share` set to true"), "{m}");
    // A cut and the summary inside it are both kept, in the order written.
    let layer = Layer::new(Mark::Bar).transform(Transform::Bin).transform(Transform::Mean)
        .transform(Transform::Proportion).transform(Transform::Stack);
    let d = check(&PlotSpec::new().data("t").x("amount").y("gdp")
        .layer(share(layer).encode(Channel::Color, "item")), &tree_data());
    let m = msgs(&d).join("\n");
    assert!(m.contains("`bar * bin * mean * proportion * stack`")
        && m.contains("`bar * bin * mean * stack` with its `share` set to true"), "{m}");
}

/// What a check says about the *sentence*, leaving out the one note that reads the
/// rows: the shared test table's `gdp` holds 1, 2 and 3, each once, so any summary
/// grouped by it is truthfully told its groups are single rows. The tests that
/// pin a sentence as legal and unremarked are about the sentence, not that table.
fn about_the_sentence(d: Vec<Diagnostic>) -> Vec<Diagnostic> {
    d.into_iter().filter(|x| !x.message.contains("every group holds a single row")).collect()
}

/// The splitting channels a mark is advised and relaxed by are the ones it takes:
/// a bar's are `color`, `pattern` and, since 2026-10-09, `group`, never `shape`,
/// which it refuses. So the `dodge` refusal names those three, and `pattern`
/// alone divides a one-slot bar as `color` does (Law 2).
#[test]
fn a_bar_is_split_and_advised_by_the_channels_it_takes() {
    assert_eq!(splitting_channels(&Mark::Bar),
               vec![Channel::Color, Channel::Pattern, Channel::Group]);
    let d = check(&PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Bar).transform(Transform::Mean).transform(Transform::Dodge)), &data());
    let m = msgs(&d).join("\n");
    assert!(m.contains("Add `color(<field>)`, `pattern(<field>)`, or `group(<field>)`")
        && !m.contains("shape("), "{m}");
    let one_slot = |ch: Channel| Layer::new(Mark::Bar).transform(Transform::Count)
        .transform(Transform::Stack).encode(ch, "region");
    assert!(bar_divides_one_slot(&one_slot(Channel::Pattern)), "a hatched pile has one slot");
    assert!(bar_divides_one_slot(&one_slot(Channel::Color)));
    let d = check(&PlotSpec::new().data("t").layer(one_slot(Channel::Pattern)), &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "{:?}", msgs(&d));
}

/// A brush draws on a plain or a dodged bar (2026-10-09), and on a stacked
/// bar only where `stack` left the column alone: the stacked measure holds
/// each segment's running top, so a bound on it, or a drag over the
/// positions, would select by where the pile reached while the page counts
/// the table's own values. Refused in the shared Unsupported words.
#[test]
fn a_bar_is_brushed_except_on_the_measure_a_stack_piles() {
    let slots = || PlotSpec::new().data("t").x("continent").y("life");
    let at = |f: &str| crate::ir::BrushDef::new(f).at(4.0, 5.5);
    for layer in [
        Layer::new(Mark::Bar),
        Layer::new(Mark::Bar).transform(Transform::Dodge).encode(Channel::Color, "region"),
    ] {
        let d = check(&slots().layer(layer).brush(at("life")), &data());
        assert!(!d.iter().any(|x| x.is_fatal()), "{:?}", msgs(&d));
    }
    let pile = || Layer::new(Mark::Bar).transform(Transform::Stack).encode(Channel::Color, "region");
    for brush in [at("life"), crate::ir::BrushDef::new("")] {
        let d = check(&slots().layer(pile()).brush(brush), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Unsupported
            && x.message.contains("valid grammar") && x.message.contains("does not draw")
            && x.message.contains("running total") && x.message.contains("`brush(continent")),
            "{:?}", msgs(&d));
    }
    let mut on_slot = crate::ir::BrushDef::new("continent");
    on_slot.levels = Some(vec!["Asia".into()]);
    let d = check(&slots().layer(pile()).brush(on_slot), &data());
    assert!(!d.iter().any(|x| x.message.contains("running total")), "{:?}", msgs(&d));
}

/// `group` on a `bar` or a `point` splits what a statistic gathers, or what
/// `dodge` (and, on a bar, `stack`) arranges (2026-10-09). A bare one would
/// change nothing, so it is refused toward a statistic or `color`, never
/// accepted and dropped (§12). `jitter` reads no split, so it does not count.
#[test]
fn group_on_a_bar_or_a_point_splits_a_statistic_and_a_bare_one_is_refused() {
    let slots = || PlotSpec::new().data("t").x("continent").y("life");
    for layer in [
        Layer::new(Mark::Bar).transform(Transform::Mean).transform(Transform::Dodge),
        Layer::new(Mark::Bar).transform(Transform::Mean),
        Layer::new(Mark::Bar).transform(Transform::Dodge),
        Layer::new(Mark::Point).transform(Transform::Mean),
        Layer::new(Mark::Point).transform(Transform::Dodge),
    ] {
        let d = check(&slots().layer(layer.clone().encode(Channel::Group, "region")), &data());
        assert!(!d.iter().any(|x| x.is_fatal()), "{:?}: {:?}", layer.transforms, msgs(&d));
    }
    let pile = Layer::new(Mark::Bar).transform(Transform::Count).transform(Transform::Stack)
        .encode(Channel::Group, "region");
    let d = check(&PlotSpec::new().data("t").x("continent").layer(pile), &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "{:?}", msgs(&d));

    // `dodge` is offered first: a table with one row per bar is the commonest way
    // here. On a scatterplot a point has no slot to dodge in, so only `color`
    // and `shape` are.
    for (layer, offers) in [
        (Layer::new(Mark::Bar), "Add `dodge` to set the bars of each `region` side by side"),
        (Layer::new(Mark::Point), "Add `dodge` to set the points of each `region`"),
        (Layer::new(Mark::Point).transform(Transform::Jitter), "`point * mean`"),
    ] {
        let d = check(&slots().layer(layer.encode(Channel::Group, "region")), &data());
        let m = msgs(&d).join("\n");
        assert!(d.iter().any(|x| x.is_fatal() && x.message.contains("would change nothing")
            && x.message.contains(offers)
            && x.message.contains("map `region` to `color`")), "{m}");
        assert!(!m.contains("cannot be bound"), "the old refusal is gone: {m}");
    }
    let d = check(&base().layer(Layer::new(Mark::Point).encode(Channel::Group, "region")), &data());
    assert!(d.iter().any(|x| x.is_fatal() && x.message.contains("needs a category axis")
        && x.message.contains("`color` or `shape`") && !x.message.contains("dodge`,")),
        "{:?}", msgs(&d));
}

/// A summary whose every group is one row draws the rows themselves and says
/// so, drawn as asked (§12's standing rule, read off the rows). It stays quiet
/// on a category, on a key whose values repeat, after a `bin`, and on a date.
#[test]
fn a_summary_of_one_row_groups_draws_the_rows_and_says_so() {
    let note = |spec: PlotSpec, d: &HashMap<String, DataFrame>| -> Vec<String> {
        check(&spec, d).into_iter()
            .filter(|x| x.kind == DiagnosticKind::Assumption
                && x.message.contains("every group holds a single row"))
            .map(|x| x.message)
            .collect()
    };
    let d = data(); // `gdp` holds 1, 2 and 3, each once
    let on = |key: &str, t: Transform| PlotSpec::new().data("t").x(key).y("life")
        .layer(Layer::new(Mark::Bar).transform(t));
    let m = note(on("gdp", Transform::Mean), &d);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].contains("`bar * mean` groups the rows by their value of `gdp`")
        && m[0].contains("`bar * bin * mean`") && m[0].contains("Drawn as asked."), "{m:?}");
    let c = note(PlotSpec::new().data("t").x("gdp")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count)), &d);
    assert!(c.len() == 1 && c[0].contains("every count is 1")
        && c[0].contains("`bar * bin + x(gdp)`"), "{c:?}");

    assert!(note(on("continent", Transform::Mean), &d).is_empty(), "a category");
    let repeats = HashMap::from([("t".to_string(), DataFrame::new()
        .with_float("gdp", vec![1.0, 1.0, 2.0]).with_float("life", vec![4.0, 5.0, 6.0]))]);
    assert!(note(on("gdp", Transform::Mean), &repeats).is_empty(), "a key that repeats");
    let cut = PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin).transform(Transform::Mean));
    assert!(note(cut, &d).is_empty(), "a cut");
    let days = HashMap::from([("t".to_string(), DataFrame::new()
        .with_time("gdp", vec![0.0, 86_400.0, 172_800.0], crate::time::TimeUnit::Day)
        .with_float("life", vec![4.0, 5.0, 6.0]))]);
    assert!(note(on("gdp", Transform::Mean), &days).is_empty(), "a date");
    // A layer refused for something else has one message, the refusal.
    let refused = PlotSpec::new().data("t").x("gdp").y("continent")
        .layer(Layer::new(Mark::Line).transform(Transform::Mean));
    let all = check(&refused, &d);
    assert!(all.iter().any(|x| x.is_fatal()), "the premise: {:?}", msgs(&all));
    assert!(note(refused, &d).is_empty(), "a refused layer");
}

/// A whole-picture layout takes no second transform, and it is told so in its
/// own terms rather than in the per-job sentences, which describe cells and
/// piles a layout does not have.
#[test]
fn a_layout_beside_another_transform_is_refused_in_its_own_terms() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let d = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("a", s(&["p", "q"]))
            .with_str("b", s(&["u", "v"]))
            .with_float("n", vec![1.0, 2.0]),
    )]);
    for t in [Transform::Proportion, Transform::Mean] {
        let out = check(&PlotSpec::new().data("t").y("n")
            .layer(Layer::new(Mark::Ribbon).flow(&["a", "b"]).transform(t.clone())), &d);
        let m = msgs(&out).join("\n");
        assert!(m.contains("gives `flow` a second transform, and `flow` takes none")
            && !m.contains("shares of the whole plot") && !m.contains("measures each cell twice"),
            "{t:?}: {m}");
    }
}

/// The one genuine ambiguity, refused rather than defaulted — Law 5, and the
/// ruling `bin(30, width = 5)` set. plotly picks a side here
/// (`branchvalues="remainder"`); gog will not, because the arithmetic is the
/// accounts' and not the grammar's.
#[test]
fn an_interior_node_with_its_own_value_is_refused_with_direction() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let d = HashMap::from([(
        "t".to_string(),
        DataFrame::new()
            .with_str("group", s(&["A", "A"]))
            .with_str("item", s(&["", "p"]))
            .with_float("amount", vec![5.0, 5.0]),
    )]);
    let out = check(&PlotSpec::new().data("t").x("amount")
        .layer(Layer::new(Mark::Zone).partition(&["group", "item"])), &d);
    assert!(out.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("value of its own")
        && x.message.contains("leaf")),
        "an interior value is refused and told where numbers belong: {:?}", msgs(&out));
}

/// A level is a **name** a branch shares; a number is what the branch is
/// weighed by. Refused here rather than by `check_distribution_axis`, whose
/// message would send the reader looking for a bin.
#[test]
fn a_numeric_level_is_refused_toward_the_measure() {
    let out = check(&PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Zone).partition(&["group", "gdp"])), &tree_data());
    assert!(out.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("holds numbers") && x.message.contains("x(gdp)")),
        "a numeric level is refused toward `x()`: {:?}", msgs(&out));
}

#[test]
fn a_categorical_position_bounds_a_zone_and_a_continuous_one_does_not() {
    // Spec §5's fourth extent description, which is the whole of the tile plot:
    // **a category owns a slot, a number is a point.** Nothing else changes —
    // no transform, no channel, no new vocabulary — so the test is that one
    // sentence read off `check`, one axis at a time.
    let bare = |x: &str, y: &str| {
        check(&PlotSpec::new().data("t").x(x).y(y).layer(Layer::new(Mark::Zone)), &data())
            .iter().any(|d| d.kind == DiagnosticKind::Illegal
                && d.message.contains("nothing here says where its sides are"))
    };
    assert!(bare("gdp", "life"), "two numbers bound nothing: a point has no width");
    assert!(!bare("continent", "region"), "two categories are a mesh — the tile plot");
    // One category with the other axis **left unsaid** draws: bounded on the
    // categorical axis, spanning the panel on the other. That is `rule`'s
    // relaxation arriving a third time, the column highlight.
    let highlight = |x: Option<&str>, y: Option<&str>| {
        let mut s = PlotSpec::new().data("t");
        if let Some(x) = x { s = s.x(x) }
        if let Some(y) = y { s = s.y(y) }
        check(&s.layer(Layer::new(Mark::Zone)), &data())
    };
    assert!(highlight(Some("continent"), None).is_empty(), "a category bounds its own axis");
    assert!(highlight(None, Some("region")).is_empty(), "and so does the other one");
    // **A number bound beside the category is refused** (2026-10-02, at the
    // author's word). It was read for the axis and dropped from the zone, which
    // spanned the panel instead: the silent drop §12 forbids, asserted here as
    // "one of each still draws" until then.
    for (x, y, n) in [("continent", "life", "y(life)"), ("gdp", "region", "x(gdp)")] {
        let d = highlight(Some(x), Some(y));
        assert!(d.iter().any(|m| m.kind == DiagnosticKind::Illegal
            && m.message.contains(&format!("`{n}` is a number, and on a `zone` a number is a point"))
            && m.message.contains("zone * bin * mean")),
            "{x} × {y}: {:?}", msgs(&d));
    }
}

/// **A zone's numbered side on a categorical axis is a slot, and one past every
/// slot is refused.** The band's own table holds two numbers and no category,
/// so the axis is known only across layers. Values of the other axis written as
/// `start`/`end` were placed near slot 70 of three and drawn far off the panel in
/// silence; the message names the measure pair they fit. Sides inside the slots
/// (the waterfall's `slot - 0.3`) and a side partly inside still draw.
#[test]
fn a_zone_side_past_every_slot_is_refused() {
    let with_band = |lo: f64, hi: f64| {
        let mut d = data();
        d.insert("b".to_string(),
            DataFrame::new().with_float("lo", vec![lo]).with_float("hi", vec![hi]));
        d
    };
    let sentence = |pair: fn(Layer) -> Layer| PlotSpec::new().data("t").y("continent").x("life")
        .layer(pair(Layer::new(Mark::Zone).data("b")))
        .layer(Layer::new(Mark::Point).data("t"));
    let refused = |d: &[Diagnostic]| d.iter().find(|m| m.kind == DiagnosticKind::Illegal
        && m.message.contains("would be drawn off the panel")).map(|m| m.message.clone());

    let d = check(&sentence(|l| l.span("lo", "hi")), &with_band(4.5, 5.5));
    let m = refused(&d).unwrap_or_else(|| panic!("past every slot: {:?}", msgs(&d)));
    assert!(m.contains("holds the 3 categories of `continent`")
        && m.contains("between -0.5 and 2.5")
        && m.contains("write `bounds(lo, hi)`"), "{m}");

    for (lo, hi, why) in [(0.7, 1.3, "inside the slots"), (2.0, 9.0, "partly inside")] {
        let d = check(&sentence(|l| l.span("lo", "hi")), &with_band(lo, hi));
        assert!(refused(&d).is_none(), "{why}: {:?}", msgs(&d));
    }
    // The measure pair on the continuous axis is the band that was meant.
    let d = check(&sentence(|l| l.bounds("lo", "hi")), &with_band(4.5, 5.5));
    assert!(refused(&d).is_none(), "lower/upper on x: {:?}", msgs(&d));
}

/// A plain zone reads its positions as slots, and the scale stated on one is
/// checked as it is on every mark: `scale = "category"` on a number drew bands
/// across the panel in silence, where `point` and `bar` refuse it. Said once:
/// the zone's own refusal steps aside for the scale's.
#[test]
fn a_category_scale_on_a_plain_zones_number_is_refused_as_on_every_mark() {
    let mut spec = PlotSpec::new().data("t").y("continent")
        .layer(Layer::new(Mark::Zone).encode(Channel::Color, "life"));
    spec.x = Some(ChannelDef { scale: Some(ScaleType::Category), ..ChannelDef::field("gdp") });
    let d = check(&spec, &data());
    assert!(d.iter().any(|m| m.kind == DiagnosticKind::Illegal
        && m.message.contains("`x(gdp, scale = \"category\")`")), "{:?}", msgs(&d));
    assert!(!d.iter().any(|m| m.message.contains("is a number, and on a `zone`")),
        "one mistake, one refusal: {:?}", msgs(&d));
    // A `bounds` zone reads neither position, so nothing stated on them is asked.
    let mut bounded = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Zone).transform(Transform::Bounds));
    bounded.x = Some(ChannelDef { scale: Some(ScaleType::Log), ..ChannelDef::field("gdp") });
    assert!(!check(&bounded, &data()).iter().any(|m| m.message.contains("scale = \"log\"")),
        "a bounds zone does not read its positions");
}

// -----------------------------------------------------------------------
// The two-dimensional readings — `path * density` (the contour) and
// `zone * density` (the estimated heatmap)
//
// The dimensionality is read off the mark, so these tests are about the *rule*
// rather than about two features: what makes both positions compulsory, what
// makes the measurement unbindable, and which knob means nothing in which
// reading. Written against `has_no_measure_axis` so a third such mark inherits
// them rather than needing its own copy.
// -----------------------------------------------------------------------

#[test]
fn a_field_needs_both_axes_because_it_cuts_both() {
    // A `bar * density` invents its measure and needs one axis. A field has no
    // axis to invent onto, so a missing position is refused rather than drawn as
    // an empty panel — the silent drop §12 forbids.
    for mark in [Mark::Path, Mark::Zone] {
        for spec in [
            PlotSpec::new().data("t").x("gdp"),
            PlotSpec::new().data("t").y("life"),
        ] {
            let s = spec.layer(Layer::new(mark.clone()).transform(Transform::Density));
            let d = check(&s, &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("both axes")),
                "{mark:?} * density with one axis must be refused: {:?}", msgs(&d));
        }
    }
    // With both, it draws.
    for mark in [Mark::Path, Mark::Zone] {
        let s = base().layer(Layer::new(mark.clone()).transform(Transform::Density));
        assert!(!check(&s, &data()).iter().any(|x| x.is_fatal()),
            "{mark:?} * density with both axes is legal: {:?}", msgs(&check(&s, &data())));
    }
}

#[test]
fn a_field_refuses_a_color_it_did_not_compute_and_names_the_one_it_did() {
    // Both positions are spoken for, so the measurement goes to `color` — and any
    // *other* column is a request for rows the transform replaced. The refusal
    // names the binding that does mean something (Law 5), which differs by
    // reading: a ring has a level, a cell has a density.
    for (mark, synthesized) in [
        (Mark::Path, crate::transform::FIELD_LEVEL),
        (Mark::Zone, crate::transform::FIELD_DENSITY),
    ] {
        let bad = base().layer(Layer::new(mark.clone())
            .transform(Transform::Density)
            .encode(Channel::Color, "continent"));
        let d = check(&bad, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains(synthesized)),
            "{mark:?} * density must refuse color(continent) toward `{synthesized}`: {:?}",
            msgs(&d));

        // Naming it out loud is the same plot, and must not be refused as a
        // missing column — the column exists only downstream of the transform.
        let good = base().layer(Layer::new(mark.clone())
            .transform(Transform::Density)
            .encode(Channel::Color, synthesized));
        assert!(!check(&good, &data()).iter().any(|x| x.is_fatal()),
            "color({synthesized}) says the default out loud: {:?}",
            msgs(&check(&good, &data())));
    }
}

#[test]
fn a_field_refuses_a_categorical_axis_toward_the_curve_per_group() {
    // `bin`'s refusal one transform over: an estimate needs an interval to spread
    // along, and a category is one slot. The direction is the plot the user
    // probably wanted — one density curve per category.
    let s = PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Path).transform(Transform::Density));
    let d = check(&s, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("line * density")),
        "a categorical axis on a contour points at the per-group curve: {:?}", msgs(&d));
}

#[test]
fn each_density_knob_is_refused_in_the_reading_it_cannot_mean() {
    // The mirror of `bin(tiling = )`'s refusal, both halves.
    // A curve reads one axis and draws its estimate on the other, so it binds no
    // `y`; a contour and a zone read both.
    let estimated = |mark: Mark, levels: Option<usize>, bandwidth: Option<f64>| {
        let one_axis = mark == Mark::Line;
        let mut l = Layer::new(mark).transform(Transform::Density);
        l.density = Some(crate::ir::DensitySpec { adjust: None, bandwidth, levels , compare: None, reach: None, support: (None, None) });
        if one_axis { PlotSpec::new().data("t").x("gdp").layer(l) } else { base().layer(l) }
    };

    // `levels` counts iso-lines, and a curve is one line.
    let d = check(&estimated(Mark::Line, Some(8), None), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("path * density(levels")),
        "levels on a curve is refused toward the contour: {:?}", msgs(&d));

    // A `zone` **is** legal with `levels`, and the history is worth keeping because
    // the bug and the feature were the same discovery. `zone * density(levels = 4)`
    // was first accepted and ignored — byte-identical output, the silent drop §12
    // forbids. It was then briefly *refused*, which closed the drop by deleting the
    // request. What it actually means is the third reading: cut the field into
    // levels and **fill** the bands, whose edges are the very curves a `path`
    // strokes. So the parameter is answered rather than removed.
    let banded = estimated(Mark::Zone, Some(4), None);
    assert!(!check(&banded, &data()).iter().any(|x| x.is_fatal()),
        "levels on a zone fills the bands: {:?}", msgs(&check(&banded, &data())));

    // `bandwidth` is a width in one column's units, and a field has two columns
    // measuring different things — so it is refused toward the dimensionless knob.
    let d = check(&estimated(Mark::Path, None, Some(5.0)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("adjust")),
        "bandwidth on a field is refused toward adjust: {:?}", msgs(&d));

    // And each is legal in its own reading.
    for (mark, levels, bandwidth) in [
        (Mark::Line, None, Some(5.0)),
        (Mark::Path, Some(8), None),
        (Mark::Zone, Some(8), None),
    ] {
        let s = estimated(mark.clone(), levels, bandwidth);
        assert!(!check(&s, &data()).iter().any(|x| x.is_fatal()),
            "{mark:?} with its own knob is legal: {:?}", msgs(&check(&s, &data())));
    }

    // `adjust` is dimensionless, so it reaches every reading — the painted field
    // included, which is what makes the refusal above a narrowing of one knob
    // rather than of the mark.
    let mut z = Layer::new(Mark::Zone).transform(Transform::Density);
    z.density = Some(crate::ir::DensitySpec { adjust: Some(1.6), bandwidth: None, levels: None , compare: None, reach: None, support: (None, None) });
    let s = base().layer(z);
    assert!(!check(&s, &data()).iter().any(|x| x.is_fatal()),
        "adjust is legal on a painted field: {:?}", msgs(&check(&s, &data())));
}

#[test]
fn a_field_does_not_synthesize_a_measure_so_a_mistyped_axis_is_still_caught() {
    // The hole `synthesizes_measure` would have had without the mark: `density`
    // is on the synthesizing list, so asked of the transforms alone it would
    // exempt a field's `y` from the column check and draw nothing at all.
    assert!(!synthesizes_measure(&Mark::Path, &[Transform::Density]),
        "a contour binds both positions and invents neither");
    assert!(synthesizes_measure(&Mark::Line, &[Transform::Density]),
        "a curve still invents its measure");
    let s = PlotSpec::new().data("t").x("gdp").y("nosuch")
        .layer(Layer::new(Mark::Path).transform(Transform::Density));
    let d = check(&s, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("nosuch")),
        "a mistyped y on a contour is reported, not skipped: {:?}", msgs(&d));
}

#[test]
fn a_zone_may_not_have_its_sides_named_and_cut_at_once() {
    // Two extent sources disagree the moment the bin layout moves, so this is
    // refused rather than resolved by letting one quietly win.
    let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Zone).transform(Transform::Bin).bounds("lo", "hi")), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("twice")),
        "`zone * bounds * bin` says where the sides are twice: {:?}", msgs(&d));
}

#[test]
fn a_binned_zone_needs_both_axes_and_colors_by_the_count() {
    // Both positions. A *bounded* zone may name neither axis and still draw —
    // it spans the panel where it is not given a pair — so this requirement is
    // the binned reading's alone, and its absence used to be an empty panel.
    for spec in [
        PlotSpec::new().data("t").x("gdp"),
        PlotSpec::new().data("t").y("life"),
        PlotSpec::new().data("t"),
    ] {
        let d = check(&spec.layer(Layer::new(Mark::Zone).transform(Transform::Bin)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("cuts both axes")),
            "a binned zone missing an axis must be refused: {:?}", msgs(&d));
    }

    // The count *is* the color, so any other field names a column the
    // transform has already replaced. Refused, naming the one binding that
    // still means something.
    let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Zone).transform(Transform::Bin)
            .encode(Channel::Color, "continent")), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("color(count)")),
        "a foreign color on a binned zone is refused toward `count`: {:?}", msgs(&d));

    // And naming it out loud is legal — the long form of what the short form
    // already does (Law 5), so it must not be refused as a missing column.
    let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Zone).transform(Transform::Bin)
            .encode(Channel::Color, "count")), &data());
    assert!(!d.iter().any(|x| x.is_fatal()),
        "`color(count)` names the synthesized column: {:?}", msgs(&d));
}

/// The advice beside that refusal reads the column's type. A category is offered
/// a facet; a number is offered the summary that colors each cell by it, since
/// a facet on a number is refused in turn, which is where `zone * bin +
/// color(life)` used to send the reader.
#[test]
fn a_number_on_a_measured_zones_color_is_sent_to_a_summary_not_a_facet() {
    let refusal = |t: Transform, color: &str| {
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Zone).transform(t).encode(Channel::Color, color)), &data());
        d.into_iter().find(|x| x.message.contains("has nothing to read"))
            .unwrap_or_else(|| panic!("no refusal for color({color})"))
            .message
    };
    let binned = refusal(Transform::Bin, "value");
    assert!(binned.contains("`zone * bin * mean + color(value)`") && !binned.contains("facet"),
        "{binned}");
    let categorical = refusal(Transform::Bin, "continent");
    assert!(categorical.contains("facet on it: `| continent`"), "{categorical}");
    let tallied = check(&PlotSpec::new().data("t").x("continent").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Count)
            .encode(Channel::Color, "value")), &data());
    assert!(tallied.iter().any(|x| x.message.contains("`zone * mean + color(value)`")),
        "{:?}", msgs(&tallied));
}

#[test]
fn bin_cuts_and_count_tallies_on_a_zone_as_they_do_on_a_bar() {
    // One rule with two polarities, and the pair is what makes it a rule rather
    // than two refusals: a **cut** needs room to cut, so `bin` refuses a category;
    // a **tally** needs cells that already exist, so `count` refuses a number.
    // The same division `bar * bin` and `bar * count` have had all along, with
    // both axes in it — which is why each refusal names the *other* transform.
    //
    // Read one axis at a time, which is what the mixed mesh below turns on: a
    // `bin` refuses only where **neither** axis can be cut.
    let d = check(&PlotSpec::new().data("t").x("continent").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Bin)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("no width to cut") && x.message.contains("zone * count")),
        "two categorical axes leave a cut nothing to cut: {:?}", msgs(&d));
    assert!(!d.iter().any(|x| x.message.contains("bar * count")),
        "and not toward a bar chart, which is a different plot: {:?}", msgs(&d));

    let d = check(&PlotSpec::new().data("t").x("gdp").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Count)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("owns no cell") && x.message.contains("`bin`")),
        "a continuous axis on a tile plot is refused toward the cut: {:?}", msgs(&d));

    // Both categorical is the tile plot, and nothing about it is fatal.
    let d = check(&PlotSpec::new().data("t").x("continent").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Count)), &data());
    assert!(!d.iter().any(|x| x.is_fatal()), "the tile plot draws: {:?}", msgs(&d));
}

#[test]
fn one_categorical_axis_is_the_mixed_mesh_and_two_is_nothing_to_cut() {
    // The rule above read one axis at a time. A `bin` needs *something to cut*,
    // not two continuous axes — so exactly one category is legal, and it is the
    // mixed mesh: the continuous axis cut into cells, one row of them per
    // category. Both orientations, because a mesh that worked one way round and
    // not the other would be the Law-2 exception the pair exists to refuse.
    for (x, y) in [("gdp", "region"), ("region", "gdp")] {
        let d = check(&PlotSpec::new().data("t").x(x).y(y)
            .layer(Layer::new(Mark::Zone).transform(Transform::Bin)), &data());
        assert!(!d.iter().any(|x| x.is_fatal()),
            "`zone * bin + x({x}) + y({y})` is the mixed mesh: {:?}", msgs(&d));
    }

    // A **field** is not the same case and keeps needing two continuous axes: a
    // density per category is a *conditional* estimate, normalized within each
    // slot, so its cells would not be comparable across slots while the ramp
    // said they were. That is the which-margin-normalizes question, not this one.
    let d = check(&PlotSpec::new().data("t").x("gdp").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Density)), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("Both axes must be continuous")),
        "a field still needs a plane to spread over: {:?}", msgs(&d));
}

#[test]
fn a_mixed_mesh_has_two_axes_and_still_no_plane_to_tile() {
    // A tiling partitions a *plane*, and `hex` interleaves two lattices by
    // weighting a step in y against a step in x — so it needs a distance on both
    // axes. A category's slots are an order, not a metric. `"rect"` is what the
    // mixed mesh already cuts, so only a non-rectangular tiling is refused.
    let tiled = |t: &str| {
        let mut l = Layer::new(Mark::Zone).transform(Transform::Bin);
        l.bin = Some(crate::ir::BinSpec {
            bins: None, width: None, tiling: Some(t.to_string()),
        });
        check(&PlotSpec::new().data("t").x("gdp").y("region").layer(l), &data())
    };
    let d = tiled("hex");
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("an order, not a distance")),
        "a hexagon has nothing to be regular against here: {:?}", msgs(&d));
    assert!(!tiled("rect").iter().any(|x| x.is_fatal()),
        "and rectangles are what it cuts anyway: {:?}", msgs(&tiled("rect")));
}

#[test]
fn a_tiling_is_validated_and_belongs_to_a_mark_with_a_plane() {
    let binned = |mark: Mark, tiling: &str| {
        let mut l = Layer::new(mark).transform(Transform::Bin);
        l.bin = Some(crate::ir::BinSpec {
            bins: None, width: None, tiling: Some(tiling.to_string()),
        });
        PlotSpec::new().data("t").x("gdp").y("life").layer(l)
    };

    // Every declared tiling is legal on the mark that has a plane to tile.
    for t in TILINGS {
        let d = check(&binned(Mark::Zone, t), &data());
        assert!(!d.iter().any(|x| x.is_fatal()),
            "`zone * bin(tiling = \"{t}\")` must be legal: {:?}", msgs(&d));
    }

    // An invented one is a typo, answered with the list.
    let d = check(&binned(Mark::Zone, "octagon"), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("is not a tiling")),
        "an invented tiling is refused: {:?}", msgs(&d));

    // A tiling on a one-dimensional bin is not a typo, it is a reasonable
    // guess — so the refusal explains that an interval has no shape and names
    // the mark that does tile a plane.
    let d = check(&binned(Mark::Bar, "hex"), &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("interval has no shape")
        && x.message.contains("zone")),
        "a tiling on a histogram is refused toward `zone`: {:?}", msgs(&d));
}

#[test]
fn a_rule_takes_no_transform_and_no_second_position() {
    // Two facts that are the mark's definition seen from other axes of the
    // grid, both pinned so a later change cannot quietly grant them.
    for t in USER_TRANSFORMS {
        assert_eq!(mark_takes_transform(&Mark::Rule, &t), TransformLegality::None,
            "`rule * {}` must not combine", transform_name(&t));
    }
    // A rule has nothing to connect, so `group` is refused the way it is on
    // `point`; and it has no glyph, so `shape` is too.
    for c in [Channel::Group, Channel::Shape, Channel::Label] {
        assert_eq!(rule_for(&Mark::Rule, &c).obligation, Obligation::Cannot,
            "`{}` must be refused on a rule", channel_name(&c));
    }
}

/// A tally's `y`, split and stacked with no `x`, is the key of a bar on its side,
/// as it is unsplit: `bar * count + y(season)` lies on its side, and adding
/// `* stack + color(dir)` does not change what `y(season)` means (Law 6). The
/// one-pile relaxation used to swallow that key and title the pile's axis with it.
/// No `y`, a `y` a statistic reads, or a name the table does not hold still leaves
/// one pile.
#[test]
fn a_split_tally_with_a_key_on_y_lies_on_its_side() {
    let pile = |t: Transform, y: Option<&str>| {
        let spec = PlotSpec::new().data("t");
        let spec = match y { Some(y) => spec.y(y), None => spec };
        spec.layer(Layer::new(Mark::Bar).transform(t).transform(Transform::Stack)
            .encode(Channel::Color, "region"))
    };
    let d = data();
    let df = d.get("t");
    for (t, y) in [(Transform::Count, "continent"), (Transform::Count, "life"),
                   (Transform::Proportion, "continent"), (Transform::Bin, "life")] {
        let spec = pile(t.clone(), Some(y));
        assert!(!bar_is_one_pile(&spec, &spec.layers[0], df), "{t:?} + y({y}) is a key");
        assert_eq!(synth_axis(&spec, &spec.layers[0], df), Channel::X,
                   "{t:?} + y({y}) is counted along x");
        let out = check(&spec, &d);
        assert!(!out.iter().any(Diagnostic::is_fatal), "{t:?} + y({y}): {:?}", msgs(&out));
    }
    for spec in [pile(Transform::Count, None), pile(Transform::Sum, Some("life")),
                 pile(Transform::Count, Some("count"))] {
        assert!(bar_is_one_pile(&spec, &spec.layers[0], df), "{:?}", spec.layers[0].transforms);
        let out = check(&spec, &d);
        assert!(!out.iter().any(Diagnostic::is_fatal), "{:?}", msgs(&out));
    }
}

#[test]
fn a_positionless_bar_refuses_the_statistics_that_need_an_axis() {
    // `count`/`sum` answer "one value for these rows", which means something
    // with nothing to group by. `bin`/`density`/`smooth` describe how values are
    // spread *along* an axis, and there is no axis — refused with direction
    // rather than quietly returning the input unchanged (§12).
    let fatal = |d: &[Diagnostic]| d.iter().any(|x| x.is_fatal());
    let with = |t: Transform| PlotSpec::new().data("t").y("life").layer(
        Layer::new(Mark::Bar).transform(t).encode(Channel::Color, "continent"));
    // A tally reads no column, so its one pile is written with no `y`: a column
    // named there is the key of a bar on its side (the test above).
    let tally = |t: Transform| PlotSpec::new().data("t").layer(
        Layer::new(Mark::Bar).transform(t).encode(Channel::Color, "continent"));

    for spec in [tally(Transform::Count), with(Transform::Sum), with(Transform::Mean),
                 tally(Transform::Proportion)] {
        assert!(!fatal(&check(&spec, &data())),
                "{:?} should be legal with no x", spec.layers[0].transforms);
    }
    for spec in [tally(Transform::Bin), tally(Transform::Density), with(Transform::Smooth)] {
        let d = check(&spec, &data());
        assert!(
            d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("Add `x(<column>)`")),
            "{:?} with no x should be refused with direction: {:?}",
            spec.layers[0].transforms, d.iter().map(|x| x.message.clone()).collect::<Vec<_>>()
        );
    }
}

// The settable rule (spec §4): a setting spans its geometry class with no
// per-mark gap, and is refused off it. These test the class membership directly
// through the check functions — the settable analog of
// `every_mark_channel_pair_has_a_rule`.

#[test]
fn a_halo_over_a_transparent_panel_names_its_color() {
    // An unset halo color is the panel's background, and a transparent one
    // would draw a halo of no color: accepted and invisible. Refused toward
    // `border_color`, which is the one word that fixes it.
    let data = HashMap::from([("t".to_string(), DataFrame::new()
        .with_float("x", vec![1.0, 2.0])
        .with_float("y", vec![1.0, 2.0])
        .with_str("name", vec!["a".into(), "b".into()]))]);
    let spec_with = |text: Layer| {
        let mut spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(text.encode(Channel::Label, "name"));
        spec.theme = ThemeSpec { background: Some("transparent".into()), ..Default::default() };
        spec
    };
    let halo_refusals = |spec: &PlotSpec| check(spec, &data).into_iter()
        .filter(|d| d.kind == DiagnosticKind::Illegal && d.message.contains("halo"))
        .count();
    let mut sized = Layer::new(Mark::Text);
    sized.style.border_size = Some(3.0);
    assert_eq!(halo_refusals(&spec_with(sized)), 1, "a halo left to a transparent panel is refused");
    let named = Layer::new(Mark::Text).style_border("white", 3.0);
    assert_eq!(halo_refusals(&spec_with(named)), 0, "a halo with its color named draws");
}

#[test]
fn border_spans_the_closed_glyph_fills() {
    // A border rims a filled glyph — available on the closed-glyph fills (bar,
    // box, point, since 2026-07-27 `zone`, the region mark the mosaic could not
    // be read without, and since 2026-09-26 `text`, whose letters are filled
    // glyphs and take it as a halo), refused on the curve fills (edge is a
    // layered line) and the strokes. None answers `Unsupported`: the class is
    // built, not "designed but not drawn."
    //
    // `zone` was the gap this test could not see: it sat in *neither* list, so
    // moving it from one side of the class to the other left the assertion green
    // either way. Naming every mark is what makes a class membership testable —
    // the same lesson `pattern_spans_*` below records one paragraph down.
    let style = StyleSpec { border_color: Some("black".into()), border_size: Some(1.0), ..Default::default() };
    for m in [Mark::Bar, Mark::Box, Mark::Point, Mark::Zone, Mark::Text] {
        let mut out = Vec::new();
        check_border(&mut out, &m, &style, &[]);
        assert!(out.is_empty(), "{m:?} accepts a border with no complaint: {out:?}");
    }
    for m in [Mark::Area, Mark::Ribbon, Mark::Line, Mark::Step, Mark::Interval] {
        let mut out = Vec::new();
        check_border(&mut out, &m, &style, &[]);
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal),
            "{m:?} refuses a border with direction: {out:?}");
    }
    // Under `flow` no mark traces a band's edge (`line * flow` is refused), so a
    // ribbon's direction is the strata, which take the border.
    let mut out = Vec::new();
    check_border(&mut out, &Mark::Ribbon, &style, &[Transform::Flow]);
    assert!(out.iter().any(|d| d.message.contains("`zone * flow(<a>, <b>)")
        && !d.message.contains("layer a `line`")), "{out:?}");

    // And the drift guard the two lists above cannot be: walk **every** mark and
    // require the class's two statements to agree. `mark_takes_setting` is what
    // `check_style` gates on and what the book's generated grid prints;
    // `check_border` is what the caller actually meets. A mark named in neither
    // hand-written list is invisible to the assertions above, which is exactly
    // how `zone` and `surface` both sat outside this test while being inside the
    // class it describes.
    for m in &ALL_MARKS {
        let mut out = Vec::new();
        check_border(&mut out, m, &style, &[]);
        let refused = out.iter().any(|d| d.kind == DiagnosticKind::Illegal);
        assert_eq!(refused, !mark_takes_setting(m, Setting::BorderColor),
            "{m:?}: `check_border` and `mark_takes_setting` disagree about the \
             closed-glyph-fill class");
    }
}

#[test]
fn pattern_spans_the_path_strokes_and_the_fills() {
    // `pattern` is a general texture aesthetic (spec §4), realized per geometry:
    // the dash on the path strokes, a hatch on the fills — *both built*, so no
    // member of either class answers `Unsupported` any more. Each geometry takes
    // its own values; the glyphs (`point`/`text`) have no region to texture.
    let pat = |v: &str| StyleSpec { pattern: Some(v.into()), ..Default::default() };

    // The drift guard, and the one that was missing: walk **every** mark and
    // require the two statements of the class to agree. `rule_for(_).settable`
    // is what the book's generated grid prints and what `check_style` gates
    // on; `texture_of` is what the value check reads. `path` shipped with
    // those two disagreeing — the grid promised a dash the checker refused —
    // because both this test and the checker enumerated marks by hand.
    for m in &ALL_MARKS {
        assert_eq!(
            texture_of(m).is_some(),
            rule_for(m, &Channel::Pattern).settable,
            "{m:?}: the grid and the value check disagree about `pattern`"
        );
        // And whichever class it is in, its own values are accepted and the
        // other class's are refused with direction — no mark is in a class
        // whose values it then rejects.
        let Some(kind) = texture_of(m) else { continue };
        let (mine, theirs): (&[&str], &[&str]) = match kind {
            Texture::Dash => (&STROKE_DASHES, &["hatch"]),
            Texture::Hatch => (&FILL_TEXTURES, &["dashed"]),
        };
        for v in mine {
            let mut out = Vec::new();
            check_pattern(&mut out, m, &pat(v));
            assert!(out.is_empty(), "{m:?} must accept its own {v:?}: {out:?}");
        }
        for v in theirs {
            let mut out = Vec::new();
            check_pattern(&mut out, m, &pat(v));
            assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal),
                "{m:?} must refuse the other class's {v:?}: {out:?}");
        }
    }

    // **The refusal a glyph gets must name every mark that does take a texture.**
    // It listed them by hand and had gone stale: the fills read
    // "`bar`/`box`/`area`/`ribbon`", omitting `zone`, which has taken a hatch since
    // it shipped — so the engine told a reader in its own voice that the mark they
    // might want a texture on cannot have one. Asserted against `texture_of`
    // rather than against a second typed list, which is the only version of this
    // test that cannot go stale the same way (`check_polar`'s lesson).
    for glyph in [Mark::Point, Mark::Text] {
        let mut out = Vec::new();
        check_pattern(&mut out, &glyph, &pat("hatch"));
        let msg = &out[0].message;
        for m in &ALL_MARKS {
            let Some(kind) = texture_of(m) else { continue };
            assert!(
                msg.contains(&format!("`{}`", mark_name(m))),
                "the {glyph:?} refusal omits `{}`, which takes a {kind:?}: {msg}",
                mark_name(m)
            );
        }
    }

    // Strokes take the dash values; a fill texture on a stroke is refused.
    for m in [Mark::Line, Mark::Step, Mark::Interval, Mark::Path, Mark::Rule] {
        for v in ["solid", "dashed", "dotted"] {
            let mut out = Vec::new();
            check_pattern(&mut out, &m, &pat(v));
            assert!(out.is_empty(), "{m:?} accepts the stroke pattern {v:?}: {out:?}");
        }
        let mut out = Vec::new();
        check_pattern(&mut out, &m, &pat("hatch"));
        assert!(out.iter().any(|d| d.message.contains("dotted")),
            "{m:?} refuses a fill texture on a stroke, pointing at the dashes: {out:?}");
    }

    // Fills take the hatch values — none `Unsupported`, the class is closed.
    for m in [Mark::Bar, Mark::Box, Mark::Area, Mark::Ribbon] {
        for v in FILL_TEXTURES {
            let mut out = Vec::new();
            check_pattern(&mut out, &m, &pat(v));
            assert!(out.is_empty(), "{m:?} accepts the fill texture {v:?}: {out:?}");
        }
        // A stroke's dash on a fill is refused with direction — never reserved.
        let mut out = Vec::new();
        check_pattern(&mut out, &m, &pat("dashed"));
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal && d.message.contains("hatch")),
            "{m:?} refuses a stroke dash on a fill, pointing at the textures: {out:?}");
        assert!(!out.iter().any(|d| d.kind == DiagnosticKind::Unsupported),
            "{m:?} no longer reserves the fill texture: {out:?}");
    }

    // The glyphs refuse any texture.
    for m in [Mark::Point, Mark::Text] {
        let mut out = Vec::new();
        check_pattern(&mut out, &m, &pat("hatch"));
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal), "{m:?} refuses a pattern: {out:?}");
    }

    // An unknown value is refused on each geometry, each pointing at its own set.
    let mut out = Vec::new();
    check_pattern(&mut out, &Mark::Line, &pat("wiggly"));
    assert!(out.iter().any(|d| d.message.contains("dotted")), "unknown stroke pattern → the dash set: {out:?}");
    let mut out = Vec::new();
    check_pattern(&mut out, &Mark::Bar, &pat("wiggly"));
    assert!(out.iter().any(|d| d.message.contains("crosshatch")), "unknown fill pattern → the texture set: {out:?}");
}

#[test]
fn pattern_channel_maps_on_strokes_and_fills_and_refused_on_glyphs() {
    // The mapped `pattern()` channel (spec §5) — `shape`'s twin, dispatching by
    // geometry: a dash per series on the strokes, a hatch per series on the fills.
    // It renders on both classes; the glyph marks refuse it.
    for m in [Mark::Line, Mark::Step, Mark::Interval, Mark::Bar, Mark::Box, Mark::Area, Mark::Ribbon] {
        assert!(rule_for(&m, &Channel::Pattern).renders.is_some(), "{m:?} draws a mapped pattern");
    }
    for m in [Mark::Point, Mark::Text] {
        assert!(rule_for(&m, &Channel::Pattern).renders.is_none(), "{m:?} refuses a mapped pattern");
    }

    // A category maps; a numeric column is refused with direction (discrete, like shape).
    let ok = base().layer(Layer::new(Mark::Line).encode(Channel::Pattern, "continent"));
    assert!(check(&ok, &data()).is_empty(), "a line mapped by a category is legal: {:?}", check(&ok, &data()));
    let bad = base().layer(Layer::new(Mark::Line).encode(Channel::Pattern, "gdp"));
    assert!(check(&bad, &data()).iter().any(|x| x.message.contains("pattern")),
        "a numeric pattern column is refused: {:?}", check(&bad, &data()));

    // Mapping and setting the texture at once is contradictory (like color/shape).
    let both = PlotSpec::new().data("t").x("continent").y("gdp")
        .layer(Layer::new(Mark::Bar).encode(Channel::Pattern, "continent").style_pattern("hatch"));
    assert!(check(&both, &data()).iter().any(|x| x.message.contains("cannot do both")),
        "map + set pattern is refused: {:?}", check(&both, &data()));
}

// -- limits: what a stated domain refuses and what it reports (spec §10) --

/// `x(<field>, limits = …)` on the plot's x axis.
fn limited(field: &str, lo: Option<f64>, hi: Option<f64>) -> PlotSpec {
    let mut s = base();
    s.x = Some(ChannelDef::field(field).with_limits(lo, hi));
    s
}

#[test]
fn a_stated_domain_that_keeps_everything_says_nothing() {
    // gdp runs 1..3. A domain around it excludes no row, so there is nothing
    // to report — the extension direction is silent, which is what makes it
    // usable for a periodic axis.
    let spec = limited("gdp", Some(0.0), Some(10.0)).layer(Layer::new(Mark::Point));
    assert!(check(&spec, &data()).is_empty(), "{:?}", msgs(&check(&spec, &data())));
}

#[test]
fn excluded_rows_are_counted_aloud_but_still_draw() {
    // The rule is not *never drop a row* but *never drop one in silence*, and
    // here the dropping is the instruction — so it is an Assumption where the
    // same condition under `scale = "log"` is a refusal.
    let spec = limited("gdp", Some(2.0), None).layer(Layer::new(Mark::Point));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Assumption], "{:?}", msgs(&d));
    assert!(d[0].message.contains("excludes 1 of 3 rows"), "{}", d[0].message);
}

#[test]
fn a_domain_that_keeps_no_row_is_fatal() {
    // An empty panel with fabricated axes is the failure §12 has been burned
    // by three times, so this one end of the rule is a refusal.
    let spec = limited("gdp", Some(100.0), Some(200.0)).layer(Layer::new(Mark::Point));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(d[0].message.contains("leaves no rows at all"), "{}", d[0].message);
    assert!(d[0].message.contains("runs 1 to 3"), "it names the range the column has: {}", d[0].message);
}

#[test]
fn a_backwards_domain_is_refused_once_and_not_blamed_on_the_data() {
    // Both a malformed pair and an emptied frame would fire here. Only the
    // first is the caller's actual mistake, and §12 wants one diagnostic per
    // undrawable thing rather than the true-but-useless second.
    let spec = limited("gdp", Some(20.0), Some(5.0)).layer(Layer::new(Mark::Point));
    let d = check(&spec, &data());
    assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
    assert!(d[0].message.contains("run backwards"), "{}", d[0].message);
    assert!(d[0].message.contains("Write 5 to 20") && !d[0].message.contains("c("),
        "it shows the fix, in words every binding reads: {}", d[0].message);
}

#[test]
fn a_category_has_no_range_to_lie_inside_and_the_refusal_names_order() {
    // ggplot2 gives discrete limits a second meaning — select and reorder.
    // gog has `order()` for that job, so the word keeps one meaning (§13).
    let mut spec = PlotSpec::new().data("t").y("gdp");
    spec.x = Some(ChannelDef::field("continent").with_limits(Some(0.0), Some(5.0)));
    let spec = spec.layer(Layer::new(Mark::Bar));
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("order(continent)")), "{:?}", msgs(&d));
}

#[test]
fn limits_span_exactly_the_channels_that_read_a_scale() {
    // Law 1: a domain is a scale property, so it reaches every channel that
    // carries a magnitude and no channel that answers *which one?*.
    for c in ALL_CHANNELS {
        let takes = reads_a_scale(&c);
        let field = if takes { "gdp" } else { "continent" };
        let ftype = if takes { VarType::Continuous } else { VarType::Discrete };
        // A mark that actually *has* this channel, so the loop reaches the
        // limits check rather than refusing the binding one step earlier.
        let Some(mark) = ALL_MARKS.iter().find(|m| {
            let r = rule_for(m, &c);
            r.obligation != Obligation::Cannot && r.accepts.accepts(ftype)
        }) else { continue };
        let spec = base().layer(
            Layer::new(mark.clone()).encode_def(
                c.clone(), ChannelDef::field(field).with_limits(Some(0.0), Some(100.0))));
        let refused = check(&spec, &data()).iter().any(|x|
            x.message.contains("no range along it for limits to cut"));
        assert_eq!(refused, !takes,
            "{c:?} on {mark:?}: limits should be {} here",
            if takes { "accepted" } else { "refused" });
    }
}

#[test]
fn a_tick_count_spans_the_channels_with_an_axis_and_no_others() {
    // The line between `tick_count` and `limits`, stated as a test because it
    // is the one thing about this parameter that is easy to get wrong. Both
    // describe the scale, so both ride the binding — but `limits` states a
    // **domain**, which all six magnitude channels have, while this states how
    // many ticks an **axis** gets, and only the three positions draw one. A
    // legend names three rows structurally, so there is no count to choose.
    for c in ALL_CHANNELS {
        let position = matches!(c, Channel::X | Channel::Y | Channel::Z);
        let Some(mark) = ALL_MARKS.iter().find(|m| {
            let r = rule_for(m, &c);
            r.obligation != Obligation::Cannot && r.accepts.accepts(VarType::Continuous)
        }) else { continue };
        let spec = base().layer(
            Layer::new(mark.clone()).encode_def(
                c.clone(), ChannelDef::field("gdp").with_tick_count(8)));
        let refused = check(&spec, &data()).iter().any(|x|
            x.message.contains("has no axis, so there are no ticks to count"));
        assert_eq!(refused, !position,
            "{c:?} on {mark:?}: tick_count should be {} here",
            if position { "accepted" } else { "refused" });
    }
}

#[test]
fn a_tick_count_under_two_is_refused_rather_than_clamped() {
    // One tick shows a place and no direction; zero is an axis with no scale
    // on it. Reported rather than quietly replaced by the default, because
    // `tick_count = 1` is a mistake about what a tick is, and drawing five
    // would leave the caller believing otherwise.
    for n in [0usize, 1] {
        let mut spec = base();
        spec.x = Some(ChannelDef::field("gdp").with_tick_count(n));
        let spec = spec.layer(Layer::new(Mark::Point));
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("at least two ticks")),
            "tick_count = {n} was accepted: {:?}", msgs(&d));
    }
}

#[test]
fn a_categorical_axis_takes_its_tick_count_from_the_data() {
    // The levels *are* the ticks, so a count would have to invent or hide
    // one. Directed at the two atoms that do the jobs a caller might be
    // reaching for, exactly as a categorical `limits` is.
    let mut spec = base();
    spec.x = Some(ChannelDef::field("continent").with_tick_count(3));
    let spec = spec.layer(Layer::new(Mark::Point));
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("one tick per category")
        && x.message.contains("order(continent)")), "{:?}", msgs(&d));
}

#[test]
fn a_layer_may_not_state_its_own_tick_count() {
    // Third parameter through the plot-scoped-scale door, and the mildest:
    // two layers asking for different counts is one axis asked to carry two
    // sets of ticks, and whichever came first would win silently.
    let mut spec = base();
    spec.x = Some(ChannelDef::field("gdp").with_tick_count(4));
    let spec = spec.layer(
        Layer::new(Mark::Point).encode_def(
            Channel::X, ChannelDef::field("gdp").with_tick_count(9)));
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("its own tick count")), "{:?}", msgs(&d));
}

#[test]
fn a_layer_may_not_state_its_own_domain() {
    // One axis, one domain — a layer stating its own is the same two
    // coordinate spaces in one panel that a per-layer *scale* would be.
    let spec = limited("gdp", Some(0.0), Some(10.0)).layer(
        Layer::new(Mark::Point).encode_def(
            Channel::X, ChannelDef::field("gdp").with_limits(Some(0.0), Some(2.0))));
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("its own limits")), "{:?}", msgs(&d));
}

#[test]
fn a_log_domain_cannot_reach_zero() {
    let mut spec = base();
    spec.x = Some(ChannelDef::field("gdp").with_scale(ScaleType::Log)
        .with_limits(Some(0.0), Some(100.0)));
    let spec = spec.layer(Layer::new(Mark::Point));
    let d = check(&spec, &data());
    assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
        && x.message.contains("undefined at zero and below")), "{:?}", msgs(&d));
}

// -----------------------------------------------------------------------
// Accepted and dropped: the 2026-09-23 round, one refusal each
// -----------------------------------------------------------------------

fn strip() -> PlotSpec {
    PlotSpec::new().data("t").x("continent").y("life")
}

fn jittered(amount: f64) -> PlotSpec {
    let mut layer = Layer::new(Mark::Point).transform(Transform::Jitter);
    layer.jitter = Some(crate::ir::JitterSpec { amount: Some(amount) });
    strip().layer(layer)
}

/// `jitter` stays inside its own category: up to 1.25 draws, past it is refused
/// with the amount that fits. A renderer test holds the number to the band.
#[test]
fn a_jitter_wide_enough_to_cross_into_the_next_category_is_refused() {
    for ok in [0.5, 1.0, JITTER_MAX] {
        assert!(check(&jittered(ok), &data()).is_empty(), "jitter({ok}) was refused");
    }
    // Zero moves nothing: drawn, with a note naming the ways to mean something.
    let d = check(&jittered(0.0), &data());
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert_eq!(d[0].kind, DiagnosticKind::Assumption, "{:?}", msgs(&d));
    assert!(d[0].message.contains("`jitter(0)` moves no point")
        && d[0].message.contains("Drop `jitter`")
        && d[0].message.contains("Drawn as asked."), "{:?}", msgs(&d));
    for wide in [1.3, 2.0, 3.0] {
        let d = check(&jittered(wide), &data());
        assert_eq!(d.len(), 1, "one refusal for jitter({wide}): {:?}", msgs(&d));
        assert_eq!(d[0].kind, DiagnosticKind::Illegal);
        assert!(d[0].message.contains(&format!("`jitter({wide})`"))
            && d[0].message.contains("`jitter(1.25)`"), "{:?}", msgs(&d));
    }
}

fn ordered_by(key: &str) -> Vec<Diagnostic> {
    let spec = PlotSpec::new().data("t").x("continent").y("gdp")
        .layer(Layer::new(Mark::Bar)).order(key);
    check(&spec, &data())
}

/// `order()` by another column reads numbers. Text has none to sort by and a
/// missing column has nothing, and both drew the axis in row order in silence.
#[test]
fn order_by_a_column_of_text_or_no_column_is_refused_with_the_two_ways_that_work() {
    for fine in ["continent", "gdp", "life"] {
        assert!(ordered_by(fine).is_empty(), "order({fine}) was refused");
    }
    let d = ordered_by("region");
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert!(d[0].message.contains("`region` holds text")
        && d[0].message.contains("`order(continent)`"), "{:?}", msgs(&d));
    let d = ordered_by("nowhere");
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert!(d[0].message.contains("no `nowhere` column"), "{:?}", msgs(&d));
}

/// A rule is one line per row. A layer naming both positions asked for two, and
/// the second was never read.
#[test]
fn a_rule_naming_both_positions_is_refused_toward_two_layers() {
    let both = base().layer(Layer::new(Mark::Point)).layer(
        Layer::new(Mark::Rule).encode(Channel::X, "gdp").encode(Channel::Y, "life"));
    let d = check(&both, &data());
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert!(d[0].message.contains("`rule + x(gdp) + rule + y(life)`"), "{:?}", msgs(&d));
    // One position on each of two layers is the sentence the refusal names.
    let two = base().layer(Layer::new(Mark::Point))
        .layer(Layer::new(Mark::Rule).encode(Channel::X, "gdp"))
        .layer(Layer::new(Mark::Rule).encode(Channel::Y, "life"));
    assert!(check(&two, &data()).is_empty(), "{:?}", msgs(&check(&two, &data())));
}

/// In a cube `z` is a position like `x` and `y`. A rule placed by one of the
/// three is the plane across the other two, refused as that plane and named by
/// its own axis, and a layer naming two of the three is refused as it is on the
/// page, with no plane to name. `rule + z(h)` was told it had no position, and
/// `rule + x(a) + z(h)` was read as the plane `x = a` with `h` unread.
#[test]
fn a_rule_in_a_cube_takes_one_position_and_is_named_by_it() {
    let level = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Rule).encode(Channel::Z, "gdp"));
    let d = check(&level, &data());
    assert_eq!(d.len(), 1, "the level plane has a position: {:?}", msgs(&d));
    assert!(d[0].message.contains(
        "`rule + z(gdp)` in a cube is a **plane**: it marks `gdp` on `z` and spans `x` and `y`"),
        "{:?}", msgs(&d));

    let upright = base().z("value").layer(Layer::new(Mark::Point))
        .layer(Layer::new(Mark::Rule).encode(Channel::X, "gdp"));
    let d = check(&upright, &data());
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert!(d[0].message.contains(
        "`rule + x(gdp)` in a cube is a **plane**: it marks `gdp` on `x` and spans `y` and `z`"),
        "{:?}", msgs(&d));

    let two = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Rule).encode(Channel::X, "gdp").encode(Channel::Z, "value"));
    let d = check(&two, &data());
    assert_eq!(d.len(), 1, "one refusal, and no plane named: {:?}", msgs(&d));
    assert!(d[0].message.contains("names two — `x(gdp)` and `z(value)`")
        && d[0].message.contains("`rule + x(gdp)` or `rule + z(value)`")
        && !d[0].message.contains("plane"), "{:?}", msgs(&d));
}

/// A mark that refuses a transform says so once. The group-by's questions and the
/// tally's used to follow, asked of a sentence the reader had been told to rewrite,
/// and the tally's named `zone` in a sentence with no zone in it.
#[test]
fn a_path_refusing_a_statistic_says_it_once() {
    for t in [Transform::Mean, Transform::Sum, Transform::Count] {
        let spec = PlotSpec::new().data("t").x("continent").y("life")
            .layer(Layer::new(Mark::Path).transform(t.clone()));
        let d = check(&spec, &data());
        assert_eq!(d.len(), 1, "path * {t:?}: {:?}", msgs(&d));
        assert!(d[0].message.contains("Use `line *"), "{:?}", msgs(&d));
        assert!(!d[0].message.contains("zone"), "{:?}", msgs(&d));
    }
}

/// Both positions wrong is one mistake, and it is said once, naming both.
#[test]
fn a_two_axis_refusal_names_both_axes_once() {
    let tile = PlotSpec::new().data("t").x("continent").y("region")
        .layer(Layer::new(Mark::Zone).transform(Transform::Bin));
    let d = check(&tile, &data());
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert!(d[0].message.contains("`x(continent)` and `y(region)` are both categorical"),
            "{:?}", msgs(&d));
    let loose = base().layer(Layer::new(Mark::Zone).transform(Transform::Count));
    let d = check(&loose, &data());
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert!(d[0].message.contains("`x(gdp)` and `y(life)` are both continuous"),
            "{:?}", msgs(&d));
}

/// Composed with a measurement, `proportion` normalizes it rather than tallying,
/// so the tally's question about the axes is not asked of it.
#[test]
fn a_normalizer_is_not_refused_as_a_tally() {
    let spec = base().layer(Layer::new(Mark::Zone)
        .transform(Transform::Density).transform(Transform::Proportion));
    let d = check(&spec, &data());
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert!(d[0].message.contains("has nothing to normalize"), "{:?}", msgs(&d));
}

/// The bare-zone refusal lists every source of sides the book teaches, the
/// boundary included.
#[test]
fn a_zone_with_no_sides_is_told_all_five_sources() {
    let d = check(&base().layer(Layer::new(Mark::Zone)), &data());
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert!(d[0].message.contains("Five things can")
        && d[0].message.contains("group(<region>)"), "{:?}", msgs(&d));
}

fn hollow(style: impl Fn(&mut StyleSpec)) -> Vec<Diagnostic> {
    let mut layer = Layer::new(Mark::Point);
    layer.style.color = Some("none".into());
    style(&mut layer.style);
    check(&base().layer(layer), &data())
}

/// `opacity` fades what `color` paints. With no fill there is nothing to fade,
/// and the direction is the border color that draws what was asked.
#[test]
fn opacity_with_nothing_to_fade_is_refused_toward_a_translucent_border() {
    let d = hollow(|s| {
        s.border_color = Some("steelblue".into());
        s.opacity = Some(0.3);
    });
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert!(d[0].message.contains("border_color = \"#4682b44d\"")
        && d[0].message.contains("steelblue outline at 30%"), "{:?}", msgs(&d));
    // The answer it gives draws, and a hollow point with no opacity is untouched.
    assert!(hollow(|s| s.border_color = Some("#4682b44d".into())).is_empty());
    // A fill to fade is the ordinary case, border or not.
    let mut filled = Layer::new(Mark::Point);
    filled.style.opacity = Some(0.3);
    filled.style.border_color = Some("black".into());
    assert!(check(&base().layer(filled), &data()).is_empty());
    // A mark with no border is told to drop it or give a color.
    let mut line = Layer::new(Mark::Line);
    line.style.color = Some("transparent".into());
    line.style.opacity = Some(0.5);
    let d = check(&base().layer(line), &data());
    assert!(d.iter().any(|x| x.message.contains("give the line a color")), "{:?}", msgs(&d));
}

/// Every channel that splits, splits: `pattern` and `shape` are a split for a
/// statistic's rows and for `dodge`/`stack`, where only `color` and `group` were.
#[test]
fn every_channel_that_splits_is_a_split_for_the_statistics() {
    let layer = Layer::new(Mark::Point).encode(Channel::Color, "continent")
        .encode(Channel::Group, "region").encode(Channel::Shape, "region");
    assert_eq!(split_fields_of(&layer), vec!["continent", "region"]);
    let spec = PlotSpec::new().data("t").x("continent").y("life").layer(
        Layer::new(Mark::Bar).transform(Transform::Mean).transform(Transform::Dodge)
            .encode(Channel::Pattern, "region"));
    let d = check(&spec, &data());
    assert!(d.is_empty(), "a pattern split is a split to dodge: {:?}", msgs(&d));
}

/// A tally or an estimate draws its own number, so a column the table holds, named
/// on the axis it draws on, is read by nothing: refused, toward the reading that
/// uses the column. The measure's own name titles the axis.
#[test]
fn a_column_named_where_a_tally_draws_is_refused() {
    let on = |mark: Mark, ts: &[Transform], x: &str, y: &str| {
        let mut layer = Layer::new(mark);
        for t in ts { layer = layer.transform(t.clone()); }
        check(&PlotSpec::new().data("t").x(x).y(y).layer(layer), &data())
    };
    for (mark, ts, x, y, direction) in [
        (Mark::Bar, vec![Transform::Count], "continent", "life", "`bar * mean`"),
        (Mark::Bar, vec![Transform::Proportion], "continent", "gdp", "`bar * sum * proportion`"),
        (Mark::Bar, vec![Transform::Bin], "gdp", "life", "`bar * bin * mean`"),
        (Mark::Line, vec![Transform::Density], "gdp", "life", "`path * density + x(gdp) + y(life)`"),
        (Mark::Point, vec![Transform::Bin, Transform::Stack], "gdp", "region", "`color(region)`"),
    ] {
        let d = on(mark.clone(), &ts, x, y);
        assert_eq!(d.len(), 1, "{mark:?} {ts:?} + y({y}): {:?}", msgs(&d));
        assert!(d[0].message.contains(&format!("`y({y})` names a column it never reads"))
            && d[0].message.contains(direction) && d[0].message.contains("`y_label()`"),
            "{:?}", msgs(&d));
    }
    // With no other position, dropping `y` would leave the layer none, which is
    // the next refusal: the direction names the axis density reads instead.
    let alone = check(&PlotSpec::new().data("t").y("life")
        .layer(Layer::new(Mark::Line).transform(Transform::Density)), &data());
    assert!(alone.iter().any(|d| d.message.contains(
        "To estimate how `life` is spread, write it on `x` instead: `line * density + x(life)`")),
        "{:?}", msgs(&alone));
    assert!(!alone.iter().any(|d| d.message.contains("Drop `y(life)`")), "{:?}", msgs(&alone));
    // The measure's own name is the output's name, and titles the axis.
    assert!(on(Mark::Bar, &[Transform::Count], "continent", "count").is_empty());
    // Beside a statistic that reads a column, the name is that statistic's input.
    assert!(on(Mark::Bar, &[Transform::Bin, Transform::Mean], "gdp", "life").is_empty());
}

/// **One axis holds one kind of number.** Two layers whose transforms invent their
/// measure on one axis must invent the same one. A count beside a density
/// estimate, a share beside an estimate, or a count beside a share is refused,
/// once, toward two plots, one below the other. Two counts, or a line over an area
/// of one estimate, still share the axis.
#[test]
fn an_axis_takes_one_invented_measure() {
    let with = |layers: &[(Mark, &[Transform])], x: &str| {
        let mut spec = PlotSpec::new().data("t").x(x);
        for (mark, ts) in layers {
            let mut layer = Layer::new(mark.clone());
            for t in ts.iter() { layer = layer.transform(t.clone()); }
            spec = spec.layer(layer);
        }
        check(&spec, &data())
    };
    let one = "An axis holds one kind of number";
    let (bin, density) = (&[Transform::Bin][..], &[Transform::Density][..]);
    let bin_share = &[Transform::Bin, Transform::Proportion][..];
    for (layers, x, first, second) in [
        (vec![(Mark::Bar, bin), (Mark::Line, density)], "gdp",
         "`bar * bin` draws counts on `y`", "`line * density` draws density estimates there"),
        (vec![(Mark::Bar, bin_share), (Mark::Line, density)], "gdp",
         "`bar * bin * proportion` draws shares", "`line * density` draws density estimates"),
        (vec![(Mark::Bar, bin), (Mark::Point, bin_share)], "gdp",
         "`bar * bin` draws counts", "`point * bin * proportion` draws shares"),
        (vec![(Mark::Bar, &[Transform::Count][..]), (Mark::Point, &[Transform::Proportion][..])],
         "continent", "`bar * count` draws counts", "`point * proportion` draws shares"),
    ] {
        let d = with(&layers, x);
        let said: Vec<&Diagnostic> = d.iter().filter(|m| m.message.contains(one)).collect();
        assert_eq!(said.len(), 1, "{layers:?}: {:?}", msgs(&d));
        assert_eq!(said[0].kind, DiagnosticKind::Illegal);
        for part in [first, second, "the second below the first with `/`"] {
            assert!(said[0].message.contains(part), "{part:?} missing: {}", said[0].message);
        }
    }
    for layers in [
        vec![(Mark::Bar, bin), (Mark::Line, bin)],
        vec![(Mark::Area, density), (Mark::Line, density)],
    ] {
        let d = with(&layers, "gdp");
        assert!(d.iter().all(|m| !m.message.contains(one)), "{layers:?}: {:?}", msgs(&d));
    }
}

/// A reading that makes its own measure axis names it too, so that axis takes
/// the measure's name or none. Another transform's word is refused toward the
/// reading that computes it, a near miss of the measure's name as a misspelling,
/// and any other name as a title, toward `<axis>_label()` with that title. Each
/// offers the measure's own name and leaving the position out.
#[test]
fn a_tally_axis_takes_its_measure_name_or_none() {
    let on = |mark: Mark, ts: &[Transform], x: &str, y: &str, shares: bool| {
        let mut layer = Layer::new(mark);
        for t in ts { layer = layer.transform(t.clone()); }
        if shares {
            layer.stack = Some(crate::ir::StackSpec { share: Some(true), baseline: None });
            layer = layer.encode(Channel::Color, "region");
        }
        check(&PlotSpec::new().data("t").x(x).y(y).layer(layer), &data())
    };
    let bin = [Transform::Bin];
    for (mark, ts, x, y, shares, own, direction) in [
        (Mark::Bar, &bin[..], "gdp", "density", false, "y(count)",
         "`line * density + x(gdp)`"),
        (Mark::Bar, &bin[..], "gdp", "proportion", false, "y(count)",
         "`bar * bin * proportion`"),
        (Mark::Bar, &bin[..], "gdp", "mean", false, "y(count)",
         "`bar * bin * mean + y(<column>)`"),
        (Mark::Bar, &[Transform::Count][..], "continent", "proportion", false, "y(count)",
         "`bar * count * proportion`"),
        (Mark::Line, &[Transform::Density][..], "gdp", "count", false, "y(density)",
         "`bar * bin + x(gdp)`"),
        (Mark::Bar, &[Transform::Bin, Transform::Proportion][..], "gdp", "count", false,
         "y(proportion)", "drop `proportion`"),
        (Mark::Point, &[Transform::Bin, Transform::Stack][..], "gdp", "density", false,
         "y(count)", "`line * density + x(gdp)`"),
        (Mark::Bar, &[Transform::Count, Transform::Stack][..], "continent", "proportion", true,
         "y(count)", "not of that column"),
    ] {
        let d = on(mark.clone(), ts, x, y, shares);
        assert_eq!(d.len(), 1, "{mark:?} {ts:?} + y({y}): {:?}", msgs(&d));
        assert!(d[0].kind == DiagnosticKind::Illegal
            && d[0].message.contains(&format!("`{y}` is the name of a transform"))
            && d[0].message.contains(&format!("`{own}`"))
            && d[0].message.contains(direction) && d[0].message.contains("`y_label()`"),
            "{mark:?} {ts:?} + y({y}): {:?}", msgs(&d));
    }
    // Any other name: a near miss of the measure's is a misspelling, and the
    // rest would only title the axis, so the title is offered as a title.
    for (mark, ts, x, y, own, said) in [
        (Mark::Bar, &bin[..], "gdp", "frequency", "y(count)",
         "`frequency` is not a column of the table, so `y(frequency)` would change only"),
        (Mark::Bar, &bin[..], "gdp", "n_rows", "y(count)", "`y_label(\"N rows\")`"),
        (Mark::Bar, &bin[..], "gdp", "cuont", "y(count)",
         "it looks like a misspelling of `count`"),
        (Mark::Line, &[Transform::Density][..], "gdp", "dens", "y(density)",
         "`dens` is not a column of the table"),
        (Mark::Bar, &[Transform::Bin, Transform::Proportion][..], "gdp", "share",
         "y(proportion)", "`y_label(\"Share\")`"),
    ] {
        let d = on(mark.clone(), ts, x, y, false);
        assert_eq!(d.len(), 1, "{mark:?} {ts:?} + y({y}): {:?}", msgs(&d));
        assert!(d[0].kind == DiagnosticKind::Illegal
            && d[0].message.contains(&format!("Write `{own}`"))
            && d[0].message.contains("leave `y()` out") && d[0].message.contains(said),
            "{mark:?} {ts:?} + y({y}): {:?}", msgs(&d));
    }
    assert!(!on(Mark::Bar, &bin[..], "gdp", "cuont", false)[0].message.contains("y_label(\""),
        "a misspelling is not offered as a title");
    // The same rule on the third axis a 3-D histogram stands its count on.
    let cube = |z: &str| check(&PlotSpec::new().data("t").x("gdp").y("life").z(z)
        .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin)), &data());
    let d = cube("rows");
    assert!(d.iter().any(|x| x.message.contains("Write `z(count)`")), "{:?}", msgs(&d));
    assert!(cube("count").iter().all(|x| !x.is_fatal()), "{:?}", msgs(&cube("count")));
    // The measure's own name, and no position at all, title the axis.
    assert!(check(&PlotSpec::new().data("t").x("gdp")
        .layer(Layer::new(Mark::Bar).transform(Transform::Bin)), &data()).is_empty());
    for (mark, ts, x, y, shares) in [
        (Mark::Bar, &bin[..], "gdp", "count", false),
        (Mark::Bar, &[Transform::Bin, Transform::Proportion][..], "gdp", "proportion", false),
        (Mark::Line, &[Transform::Density][..], "gdp", "density", false),
        // A filled stack rescales the tally's column in place, so the name is
        // still `count`; its axis reads "Share" whatever `y()` names.
        (Mark::Bar, &[Transform::Count, Transform::Stack][..], "continent", "count", true),
    ] {
        let d = on(mark.clone(), ts, x, y, shares);
        assert!(d.is_empty(), "{mark:?} {ts:?} + y({y}): {:?}", msgs(&d));
    }
}

/// **A binding with an empty name is refused, on every channel** (ruled
/// 2026-10-09). R's missing argument sent one: `y(limits = c(0, 60))` drew the
/// counts under `bar * count`, with "y is " and nothing after it in the
/// picture's description, and on a `point` was refused as a misspelling of a
/// column called nothing. The direction names the column the settings belong
/// beside: the measure's own name on the axis a transform makes.
#[test]
fn a_binding_that_names_no_column_is_refused() {
    let only = |spec: PlotSpec| {
        let d = check(&spec, &data());
        assert_eq!(d.len(), 1, "{:?}", msgs(&d));
        assert!(d[0].kind == DiagnosticKind::Illegal, "{:?}", msgs(&d));
        d[0].message.clone()
    };
    let tally = only(PlotSpec::new().data("t").x("continent").y("")
        .layer(Layer::new(Mark::Bar).transform(Transform::Count)));
    assert!(tally.contains("`y()` names no column")
        && tally.contains("On `bar * count` the y axis shows the `count`")
        && tally.contains("`y(count, limits = …)`") && tally.contains("leave `y()` out"),
        "{tally}");
    let density = only(PlotSpec::new().data("t").x("gdp").y("")
        .layer(Layer::new(Mark::Line).transform(Transform::Density)));
    assert!(density.contains("`y(density, limits = …)`"), "{density}");
    // An axis no transform makes takes a column of the table, and so does the
    // other axis of a tally.
    let plain = only(PlotSpec::new().data("t").x("gdp").y("").layer(Layer::new(Mark::Point)));
    assert!(plain.contains("`y()` names no column")
        && plain.contains("`y(<column>, limits = …)`"), "{plain}");
    let key = only(PlotSpec::new().data("t").x("").layer(Layer::new(Mark::Bar)
        .transform(Transform::Count)));
    assert!(key.contains("`x(<column>, limits = …)`"), "{key}");
    let color = only(PlotSpec::new().data("t").x("gdp").y("life")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "")));
    assert!(color.contains("`color()` names no column")
        && color.contains("`color(<column>)`"), "{color}");
}

/// **A `bin` handed to a statistic makes no number of its own**, so the column the
/// statistic reads has to be named and has to exist. Read as a tally, a misspelled
/// column drew an empty panel on 0..1 axes, and a missing `y` the same panel
/// titled "Count", both in silence.
#[test]
fn a_binned_statistic_reads_a_column_that_must_exist() {
    for t in [Transform::Mean, Transform::Sum, Transform::Median] {
        let layer = |y: Option<&str>| {
            let spec = PlotSpec::new().data("t").x("gdp").layer(
                Layer::new(Mark::Bar).transform(Transform::Bin).transform(t.clone()));
            check(&match y { Some(y) => spec.y(y), None => spec }, &data())
        };
        let typo = layer(Some("lfe"));
        assert!(typo.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("`y(lfe)` refers to a column that is not in the data")),
            "{t:?}: {:?}", msgs(&typo));
        let none = layer(None);
        assert!(none.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("needs `y()`")), "{t:?}: {:?}", msgs(&none));
        assert!(layer(Some("life")).is_empty(), "{t:?}");
    }
    assert!(!synthesizes_measure(&Mark::Bar, &[Transform::Bin, Transform::Mean]));
    assert!(synthesizes_measure(&Mark::Bar, &[Transform::Bin]));
    // The refusal names the statistic short of a column, and the counting
    // reading beside a summary: by bin over numbers, by slot over categories.
    let short = |ts: &[Transform], x: &str| {
        let mut layer = Layer::new(Mark::Bar);
        for t in ts { layer = layer.transform(t.clone()); }
        check(&PlotSpec::new().data("t").x(x).layer(layer), &data())
    };
    for (ts, x, said, count) in [
        (vec![Transform::Bin, Transform::Mean], "gdp", "`mean` averages", "drop `mean`: `bar * bin`"),
        (vec![Transform::Mean], "continent", "`mean` averages", "write `bar * count`"),
        (vec![Transform::Sum], "gdp", "`sum` adds up", "write `bar * bin`"),
    ] {
        let d = short(&ts, x);
        assert!(d.iter().any(|d| d.message.contains(said) && d.message.contains(count)),
            "{ts:?} + x({x}): {:?}", msgs(&d));
    }
}

/// `theme(gap = )` is a page's: pixels, 0 or more, and refused on one plot,
/// which has no plots beside it.
#[test]
fn a_page_takes_a_gap_and_a_plot_refuses_one() {
    let plot = || base().layer(Layer::new(Mark::Point));
    let page = |gap: f64| Figure::Page(crate::ir::PageSpec {
        arrange: crate::ir::Arrange::Beside,
        cells: vec![plot().into(), plot().into()],
        theme: ThemeSpec { gap: Some(gap), ..ThemeSpec::default() },
    });
    assert!(check_figure(&page(0.0), &data()).is_empty());
    assert!(check_figure(&page(8.0), &data()).is_empty());
    assert!(check_figure(&page(-1.0), &data()).iter()
        .any(|d| d.kind == DiagnosticKind::Illegal && d.message.contains("is not a space")));
    let mut alone = plot();
    alone.theme.gap = Some(8.0);
    assert!(check(&alone, &data()).iter().any(|d| d.kind == DiagnosticKind::Illegal
        && d.message.contains("`theme(gap = )` is the space between the plots of an arrangement")));
}

/// `gap` belongs to the two marks with a body in a slot, takes a share below 1,
/// and is refused in a packing; a violin is sent to the setting that already
/// sets its width.
#[test]
fn a_gap_is_a_share_beside_a_bar_or_a_box() {
    let with_gap = |mark: Mark, ts: &[Transform], gap: f64| {
        let mut layer = Layer::new(mark);
        for t in ts { layer = layer.transform(t.clone()); }
        layer.style.gap = Some(gap);
        layer
    };
    let flat = |layer: Layer| check(&PlotSpec::new().data("t").x("continent").y("life")
        .layer(layer), &data());
    assert!(flat(with_gap(Mark::Bar, &[Transform::Mean], 0.3)).is_empty());
    assert!(flat(with_gap(Mark::Box, &[], 0.0)).is_empty());
    let said = |d: Vec<Diagnostic>, what: &str| d.iter()
        .any(|d| d.kind == DiagnosticKind::Illegal && d.message.contains(what));
    assert!(said(flat(with_gap(Mark::Bar, &[Transform::Mean], 1.0)), "outside 0 to 1"));
    assert!(said(flat(with_gap(Mark::Bar, &[Transform::Mean], -0.1)), "outside 0 to 1"));
    assert!(said(flat(with_gap(Mark::Interval, &[Transform::Range], 0.2)),
                 "takes no width from its slot"));
    assert!(said(flat(with_gap(Mark::Ribbon, &[Transform::Density], 0.2)),
                 "`density(reach = )`"));
    let nest = check(&PlotSpec::new().data("t").x("gdp").coord(CoordSpace::Nest)
        .layer(with_gap(Mark::Bar, &[], 0.2)), &data());
    assert!(said(nest, "packed region"));
}

/// **A column a transform writes may be left out, and a `text` layer's label
/// is one.** Over `partition`, `flow` and `layout` it labels each node with the
/// `name` those transforms write, exactly as `label(name)` does; a label the
/// reader wrote stays, and a plain `text` still needs one.
#[test]
fn a_text_layer_over_named_nodes_takes_their_name_by_default() {
    let flow = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Ribbon).flow(&["continent", "region"]))
        .layer(Layer::new(Mark::Text).flow(&["continent", "region"]));
    let resolved = resolve_scopes(&flow);
    assert_eq!(resolved.layers[1].encodings.get(&Channel::Label).map(|d| d.field.as_str()),
               Some(crate::transform::NODE_NAME));
    assert!(resolved.layers[0].encodings.get(&Channel::Label).is_none(),
            "only the text layer takes a label");
    let d = check(&flow, &data());
    assert!(!d.iter().any(|d| d.message.contains("needs `label()`")), "{:?}", msgs(&d));
    let partition = PlotSpec::new().data("t")
        .layer(Layer::new(Mark::Zone).partition(&["continent", "region"]))
        .layer(Layer::new(Mark::Text).partition(&["continent", "region"]));
    assert_eq!(resolve_scopes(&partition).layers[1].encodings.get(&Channel::Label)
        .map(|d| d.field.as_str()), Some(crate::transform::NODE_NAME));
    let written = Layer::new(Mark::Text).flow(&["continent", "region"])
        .encode(Channel::Label, "continent");
    assert_eq!(resolve_scopes(&PlotSpec::new().data("t").layer(written)).layers[0]
        .encodings.get(&Channel::Label).map(|d| d.field.as_str()), Some("continent"));
    let plain = check(&base().layer(Layer::new(Mark::Text)), &data());
    assert!(plain.iter().any(|d| d.message.contains("needs `label()`")), "{:?}", msgs(&plain));
}

/// One mistake, one refusal: once `check_box` has said a box takes no transform,
/// the chain is not refused again for pairing `mean` with the box's own summary.
/// Where no refusal about the mark was printed, the chain still speaks, which is
/// the case the ruling in `check_chain_jobs` was written for.
#[test]
fn a_transform_the_mark_refused_is_not_refused_again_as_a_chain() {
    let boxed = PlotSpec::new().data("t").x("continent").y("life")
        .layer(Layer::new(Mark::Box).transform(Transform::Mean));
    let d = check(&boxed, &data());
    assert_eq!(d.len(), 1, "{:?}", msgs(&d));
    assert!(d[0].message.contains("takes no transform"), "{:?}", msgs(&d));
    let unrefused = PlotSpec::new().data("t").x("continent").y("life").layer(
        Layer::new(Mark::Interval).transform(Transform::Bounds).transform(Transform::Mean)
            .bounds("gdp", "value"));
    let d = check(&unrefused, &data());
    assert!(d.iter().any(|x| x.message.contains("says what this rectangle is twice")),
            "the chain must still refuse where the mark said nothing: {:?}", msgs(&d));
}
