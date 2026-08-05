//! The one way into the engine — a spec in, an SVG or a refusal out.
//!
//! Everything below this module can draw; only this module decides *whether to*.
//! The legality gate and the strictness policy live here together, above both
//! `legality` and `render`, because they are one decision and splitting them is
//! what caused the defect this module exists to close.
//!
//! **The defect.** The gate used to live in `gog-cli/src/main.rs`: the bridge
//! called `legality::check`, applied `GOG_STRICT`, and only then called
//! `SvgRenderer::render`. But `render` was `pub`, so anything that was not the
//! CLI — the four in-tree examples, and any future Rust, WASM or FFI binding —
//! reached the renderer without passing the gate. `point + size(continent)`
//! driven from Rust returned 2302 bytes of SVG with no error and no warning,
//! where the same sentence from R exits 2 with direction. That is a binding
//! accepted and silently dropped, which spec §12 forbids outright, and it was
//! reachable only because a policy two callers needed was written into one of
//! them (`CONTRIBUTING.md` rule 4: a helper shared by two callers belongs to
//! neither).
//!
//! **The rule this encodes.** A binding is thin — it converts a wire format and
//! prints. It does not decide what is legal. So the gate is not something a
//! caller remembers to run; it is the only door, and `SvgRenderer::render` is
//! `pub(crate)` behind it. A new binding inherits enforcement by construction
//! rather than by re-implementing the policy correctly, and the compiler is what
//! checks it: a caller outside the crate that reaches for the renderer directly
//! does not build.

use std::collections::HashMap;

use crate::data::DataFrame;
use crate::ir::{Figure, PlotSpec};
use crate::legality::{self, Diagnostic, DiagnosticKind};
use crate::render::page;
use crate::render::svg::{SvgRenderer, CANVAS};

/// A drawn plot, and everything the engine wants to say about it.
///
/// The diagnostics ride along on success because most of them are not refusals:
/// an **Assumption** renders and still has to be reported (§12 — "it renders,
/// but a default was chosen; confirm it is what you meant"). Returning the SVG
/// alone would drop them, which is the same silent drop one level along — so
/// the success path carries its diagnostics rather than discarding them.
///
/// Under [`Strictness::Permissive`] this list also carries the fatal ones the
/// caller asked to draw anyway. A caller reports the whole list either way.
pub struct Drawing {
    pub svg: String,
    /// Every diagnostic the check produced, in spec order. Empty means the plot
    /// is grammatical and nothing was assumed on the caller's behalf.
    pub diagnostics: Vec<Diagnostic>,
}

/// Whether a fatal diagnostic stops the render.
///
/// The escape hatch is engine-wide policy, not a CLI flag, which is why it is
/// declared here: `GOG_STRICT=0` has to mean the same thing from R, from Python
/// and from Rust, or it is a rule one binding gets wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strictness {
    /// A fatal diagnostic refuses the plot. The default, and what §12 describes.
    Strict,
    /// Draw anyway, reporting the fatal diagnostics. For migrating existing
    /// plots, and for a reader who has read the refusal and wants the picture.
    Permissive,
}

impl Strictness {
    /// Read `GOG_STRICT` — strict unless it is explicitly `0`.
    pub fn from_env() -> Self {
        match std::env::var("GOG_STRICT") {
            Ok(v) if v == "0" => Strictness::Permissive,
            _ => Strictness::Strict,
        }
    }
}

/// What to tell a reader whose plot was refused, after the diagnostics.
///
/// Lives beside the policy rather than in the caller, for the reason the whole
/// module exists: a second binding would otherwise write its own sentence, and
/// two wordings of one rule is how a rule stops being one.
pub const REFUSED: &str =
    "gog: nothing was rendered. Fix the above, or set GOG_STRICT=0 to draw anyway.";

/// Check `spec` against the grammar, then draw it — honoring `GOG_STRICT`.
///
/// `Err` carries every diagnostic, at least one of them fatal; the plot was not
/// drawn. `Ok` carries the SVG and any non-fatal remarks. This is the entry
/// point every binding should call.
pub fn render(
    spec: &PlotSpec,
    data: &HashMap<String, DataFrame>,
) -> Result<Drawing, Vec<Diagnostic>> {
    render_with(spec, data, Strictness::from_env())
}

/// [`render`] with the strictness named outright instead of read from the
/// environment — for a caller that has its own policy, and for tests, which must
/// not depend on the ambient environment to decide what the engine does.
pub fn render_with(
    spec: &PlotSpec,
    data: &HashMap<String, DataFrame>,
    strictness: Strictness,
) -> Result<Drawing, Vec<Diagnostic>> {
    render_figure_with(&Figure::Plot(Box::new(spec.clone())), data, strictness)
}

/// [`render`] for a figure that may be a *page* of plots (spec §11).
///
/// One door for both, for the reason this module exists: the gate, the
/// strictness policy and the sentence a refusal ends with are one decision, and
/// a page reaching the renderer down a second path is how a binding comes to
/// enforce a rule the engine already owns.
pub fn render_figure(
    figure: &Figure,
    data: &HashMap<String, DataFrame>,
) -> Result<Drawing, Vec<Diagnostic>> {
    render_figure_with(figure, data, Strictness::from_env())
}

pub fn render_figure_with(
    figure: &Figure,
    data: &HashMap<String, DataFrame>,
    strictness: Strictness,
) -> Result<Drawing, Vec<Diagnostic>> {
    let mut diagnostics = legality::check_figure(figure, data);

    if strictness == Strictness::Strict && diagnostics.iter().any(Diagnostic::is_fatal) {
        return Err(diagnostics);
    }

    // `render` resolves channel scope itself, idempotently, so the check having
    // already resolved a copy costs a clone and buys the two stages agreeing.
    let svg = match figure {
        // A plot states its own size, and takes the canvas when it does not
        // (`ThemeSpec::width`). Composed, the same statement sizes its cell —
        // which is `render::page`'s arithmetic, not this line's.
        // `draw` rather than `render`, for its second return: a few things are only
        // knowable once the page has a size — whether a label fits the region it
        // names is the first — and a stage that can leave something out has to be
        // able to say so (§12). They join the check's own list; none is fatal.
        Figure::Plot(spec) => {
            let theme = spec.theme.resolved();
            let drawn = SvgRenderer::for_theme(
                &theme,
                theme.width.unwrap_or(CANVAS.0),
                theme.height.unwrap_or(CANVAS.1),
            )
            .draw(spec, data);
            diagnostics.extend(drawn.remarks);
            drawn.svg
        }
        // The same two lines as the plot arm, and that is the point: a figure
        // states its own size and takes the canvas when it does not, whether it
        // is one plot or a page of them. This arm read `CANVAS` unconditionally
        // until a page had a theme to read, which made the composed figure the
        // one thing in the grammar whose size nobody could state.
        Figure::Page(spec) => {
            let theme = spec.theme.resolved();
            let (svg, remarks) = page::render(
                spec,
                data,
                theme.width.unwrap_or(CANVAS.0),
                theme.height.unwrap_or(CANVAS.1),
            );
            diagnostics.extend(remarks);
            svg
        }
    };

    Ok(Drawing { svg, diagnostics })
}

/// One still SVG per moment of a played plot, in order — what a caller needs to
/// assemble a file that moves where SVG animation is not read.
///
/// **Conversion, never a second renderer.** Each still comes out of the same
/// [`SvgRenderer::draw`] that draws the plot, asked to leave a different moment
/// showing; nothing here decides a tick, a color or a layout. The `png.rs`
/// history is why that distinction is written down rather than assumed — a
/// second writer with its own opinions drifted until it drew untransformed rows
/// under a transform's name, and a frame that only selects has no opinion to
/// drift from. Every scale, the color map and each legend are fitted across the
/// whole sequence one level below this, so the stills agree by construction.
///
/// `Err` for a plot with no `play`, which is not an animation and cannot become
/// one, and for a composed page — the moments of two plots are two clocks, and
/// nothing yet says whose is the file's.
pub fn render_frames(
    figure: &Figure,
    data: &HashMap<String, DataFrame>,
) -> Result<Vec<String>, Vec<Diagnostic>> {
    render_frames_with(figure, data, Strictness::from_env())
}

pub fn render_frames_with(
    figure: &Figure,
    data: &HashMap<String, DataFrame>,
    strictness: Strictness,
) -> Result<Vec<String>, Vec<Diagnostic>> {
    let mut diagnostics = legality::check_figure(figure, data);
    if strictness == Strictness::Strict && diagnostics.iter().any(Diagnostic::is_fatal) {
        return Err(diagnostics);
    }

    let Figure::Plot(spec) = figure else {
        diagnostics.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: "gog: a composed page has no single sequence to write. Two \
                      plots on one page keep two clocks, and nothing says which \
                      one the file runs on. Save the played plot on its own."
                .to_string(),
        });
        return Err(diagnostics);
    };

    let levels = crate::render::svg::play_levels(spec, data);
    if levels.len() < 2 {
        diagnostics.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: "gog: this plot does not play, so it has no moments to \
                      write. Bind a column with an order to `play` — \
                      `play(year)` — or save the still picture it already is."
                .to_string(),
        });
        return Err(diagnostics);
    }

    let theme = spec.theme.resolved();
    let mut frames = Vec::with_capacity(levels.len());
    for moment in 0..levels.len() {
        let mut renderer = SvgRenderer::for_theme(
            &theme,
            theme.width.unwrap_or(CANVAS.0),
            theme.height.unwrap_or(CANVAS.1),
        );
        renderer.still = Some(moment);
        // The remarks are the same sentence every moment — one render's worth is
        // what a reader needs, and twelve copies of it is noise.
        let drawn = renderer.draw(spec, data);
        if moment == 0 {
            diagnostics.extend(drawn.remarks);
        }
        frames.push(drawn.svg);
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Channel, Layer, Mark};
    use crate::legality::DiagnosticKind;

    /// The gapminder-shaped three rows the recorded repro used.
    fn data() -> HashMap<String, DataFrame> {
        let df = DataFrame::new()
            .with_float("gdp", vec![1329.0, 2182.0, 3677.0])
            .with_float("life", vec![43.8, 47.2, 51.5])
            .with_str(
                "continent",
                vec!["Asia", "Africa", "Europe"].into_iter().map(String::from).collect(),
            );
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        data
    }

    fn base() -> PlotSpec {
        PlotSpec::new().data("t").x("gdp").y("life")
    }

    /// A legal plot draws, and says nothing.
    #[test]
    fn a_grammatical_plot_renders_with_no_diagnostics() {
        let spec = base().layer(Layer::new(Mark::Point));
        let drawn = render_with(&spec, &data(), Strictness::Strict).expect("should render");
        assert!(drawn.svg.contains("<svg"));
        assert!(
            drawn.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            drawn.diagnostics
        );
    }

    /// The recorded repro, and the reason this module exists: `size` is
    /// continuous-only on `point`, so a categorical column is Illegal. Before the
    /// gate moved down, this returned 2302 bytes of SVG from Rust while exiting 2
    /// from R — the one path that silently dropped a binding.
    #[test]
    fn an_illegal_plot_is_refused_not_drawn() {
        let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Size, "continent"));
        let diagnostics = render_with(&spec, &data(), Strictness::Strict)
            .err()
            .expect("`point + size(continent)` must not render");
        assert!(diagnostics.iter().any(Diagnostic::is_fatal));
        assert!(
            diagnostics.iter().any(|d| d.kind == DiagnosticKind::Illegal
                && d.message.contains("size(continent)")),
            "refusal must name the binding it refuses: {diagnostics:?}"
        );
    }

    /// `GOG_STRICT=0` draws the same plot and still reports why it should not
    /// have. The escape hatch downgrades the refusal; it never hides it.
    #[test]
    fn permissive_draws_the_illegal_plot_and_still_reports_it() {
        let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Size, "continent"));
        let drawn = render_with(&spec, &data(), Strictness::Permissive)
            .expect("GOG_STRICT=0 draws anyway");
        assert!(drawn.svg.contains("<svg"));
        assert!(
            drawn.diagnostics.iter().any(Diagnostic::is_fatal),
            "the fatal diagnostic must survive into the drawing, not be swallowed"
        );
    }

    /// An Assumption is not a refusal, so it must arrive *with* the picture.
    /// Dropping it here would be the same silent drop the module closes, one
    /// level along. A bare `bar` with no `y` assumes `count`.
    #[test]
    fn a_non_fatal_remark_rides_along_with_the_drawing() {
        let df = DataFrame::new().with_str(
            "continent",
            vec!["Asia", "Africa", "Asia"].into_iter().map(String::from).collect(),
        );
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);

        let spec = PlotSpec::new()
            .data("t")
            .x("continent")
            .layer(Layer::new(Mark::Bar).transform(crate::ir::Transform::Count));
        let drawn = render_with(&spec, &data, Strictness::Strict).expect("should render");
        assert!(drawn.svg.contains("<svg"));
        assert!(
            drawn.diagnostics.iter().all(|d| !d.is_fatal()),
            "nothing fatal here: {:?}",
            drawn.diagnostics
        );
    }

    /// The default entry point is the one that reads the environment. Without an
    /// explicit `GOG_STRICT=0` it must refuse, so a binding that forgets to think
    /// about strictness gets the strict behavior rather than the lenient one.
    #[test]
    fn the_default_entry_point_is_strict() {
        if std::env::var("GOG_STRICT").is_ok() {
            return; // the ambient environment is deciding; nothing to prove here
        }
        let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Size, "continent"));
        assert!(render(&spec, &data()).is_err());
    }

    // ---- a page states its own size ---------------------------------------
    //
    // The tests below are at *this* level and not in `render::page`, because the
    // defect they pin was here: `render::page` has always drawn at whatever size
    // it was handed, and this module handed it the canvas whatever the figure
    // said. A test one layer down would have passed throughout.

    fn beside(cells: Vec<Figure>, theme: crate::ir::ThemeSpec) -> Figure {
        Figure::Page(crate::ir::PageSpec { arrange: crate::ir::Arrange::Beside, cells, theme })
    }

    fn two_plots() -> Vec<Figure> {
        vec![
            base().layer(Layer::new(Mark::Point)).into(),
            base().layer(Layer::new(Mark::Point)).into(),
        ]
    }

    /// A composed figure is drawn at the size it asks for.
    ///
    /// Two plots side by side split the *width* and each keep the whole height,
    /// so a page that cannot say how tall it is gives every cell a plot's worth
    /// of height however little is in it. That is what left two thirds of a
    /// composed cube's panel empty: the cube fits its panel with one uniform
    /// scale, the width bound it, and nothing could ask for a shorter figure.
    #[test]
    fn a_page_is_drawn_at_the_size_it_states() {
        let theme = crate::ir::ThemeSpec { height: Some(310.0), ..Default::default() };
        let drawn = render_figure(&beside(two_plots(), theme), &data()).expect("should render");
        assert!(
            drawn.svg.contains(r#"width="800" height="310""#),
            "the page asked to be 310px tall and was drawn {}",
            &drawn.svg[..drawn.svg.find('>').unwrap_or(120)]
        );
        assert!(drawn.svg.contains(r#"viewBox="0 0 800 310""#), "and the viewBox agrees");
    }

    /// And a page that asks for nothing is still the canvas, which is every
    /// composed figure written before this could be stated.
    #[test]
    fn a_page_that_states_no_size_takes_the_canvas() {
        let drawn = render_figure(&beside(two_plots(), Default::default()), &data())
            .expect("should render");
        assert!(drawn.svg.contains(r#"width="800" height="600""#));
    }

    /// A page takes the two theme properties whose subject is the figure, and
    /// refuses the ones whose subject is a panel — with the sentence that says
    /// where to write them instead. Silence here would be the accept-and-drop
    /// §12 forbids, and it is what a page did with every atom until now.
    #[test]
    fn a_panel_property_on_a_page_is_refused_with_direction() {
        let theme = crate::ir::ThemeSpec {
            grid: Some("none".to_string()),
            height: Some(310.0),
            ..Default::default()
        };
        let figure = beside(two_plots(), theme);
        let refused = render_figure(&figure, &data())
            .err()
            .expect("a page must say it cannot use `grid`, not drop it");
        let said = refused
            .iter()
            .find(|d| d.message.contains("theme(grid = )"))
            .expect("and the refusal must name the property it refuses");
        assert_eq!(said.kind, DiagnosticKind::Unsupported);
        assert!(
            said.message.contains("before composing"),
            "a refusal names what to do next: {}",
            said.message
        );
        // And the size it *can* state is untouched by the refusal of the one it
        // cannot: under `GOG_STRICT=0` the same figure draws, 310px tall.
        let drawn = render_figure_with(&figure, &data(), Strictness::Permissive)
            .expect("GOG_STRICT=0 draws anyway");
        assert!(drawn.svg.contains(r#"height="310""#));
    }

    // -- the palette vocabulary, checked across the two layers that hold it ---
    //
    // `legality` says which names may be written; `render::palette` says what
    // colors they are. The layering forbids either from reading the other, so
    // this module — the only one above both — is where the two halves are made
    // to agree. Both directions matter and they fail differently: a name legal
    // but unresolvable draws the *default's* colors with no complaint, and a
    // name resolvable but illegal is a palette nobody can reach.

    /// Every legal name resolves to colors of its own.
    #[test]
    fn every_named_palette_resolves_to_its_own_colors() {
        use crate::legality::{CATEGORICAL_PALETTES, DIVERGING_RAMPS, SEQUENTIAL_RAMPS};
        use crate::render::palette::{named_palette, named_ramp, PALETTE_GOG, RAMP_BLUE};

        for name in SEQUENTIAL_RAMPS.iter().chain(DIVERGING_RAMPS) {
            let stops = named_ramp(name)
                .unwrap_or_else(|| panic!("`palette(\"{name}\")` is legal but resolves to nothing"));
            assert!(stops.len() >= 2, "{name}: a ramp needs two stops to interpolate");
            if *name != "blue" {
                assert_ne!(stops, RAMP_BLUE, "{name} silently resolves to the default ramp");
            }
        }
        for name in CATEGORICAL_PALETTES {
            let colors = named_palette(name)
                .unwrap_or_else(|| panic!("`palette(\"{name}\")` is legal but resolves to nothing"));
            if *name != "gog" {
                assert_ne!(colors, PALETTE_GOG, "{name} silently resolves to the default palette");
            }
        }
    }

    /// And every set of colors has a name that can reach it.
    #[test]
    fn every_resolvable_palette_is_legal_to_write() {
        use crate::legality::{CATEGORICAL_PALETTES, DIVERGING_RAMPS, SEQUENTIAL_RAMPS};
        use crate::render::palette::{PALETTES, RAMPS};

        for (name, _) in RAMPS {
            assert!(
                SEQUENTIAL_RAMPS.contains(name) || DIVERGING_RAMPS.contains(name),
                "the ramp `{name}` exists but no sentence can ask for it"
            );
        }
        for (name, _) in PALETTES {
            assert!(
                CATEGORICAL_PALETTES.contains(name),
                "the palette `{name}` exists but no sentence can ask for it"
            );
        }
    }

    /// The ramps gog derives keep their palest stop on the page.
    ///
    /// `RAMP_BLUE`'s range is compressed for this reason — gog draws *points*,
    /// and a 4.5px dot at 1.25:1 against the panel is invisible, so a scale
    /// whose pale end fades into the surface cannot say "small" and "absent"
    /// apart. The claim spans two modules (the stops here, `PANEL_BG` in the
    /// renderer), which is what puts the test in this one.
    ///
    /// The imported ramps are deliberately exempt and listed by name: viridis
    /// and its three siblings end on a near-white by design, and re-tuning them
    /// would leave gog shipping something called `magma` that is not magma.
    /// That is the trade the book states rather than a gap in the rule.
    #[test]
    fn gog_derived_ramps_keep_their_pale_end_on_the_page() {
        use crate::render::palette::{parse_color, RAMPS};
        use crate::render::svg::PANEL_BG;

        let lum = |hex: &str| {
            let (r, g, b) = parse_color(hex).unwrap();
            let ch = |v: f64| {
                let c = v / 255.0;
                if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
        };
        let panel = lum(PANEL_BG);
        let contrast = |hex: &str| {
            let (a, b) = (lum(hex) + 0.05, panel + 0.05);
            if a > b { a / b } else { b / a }
        };

        const IMPORTED: &[&str] = &["viridis", "magma", "inferno", "plasma", "cividis"];
        for (name, stops) in RAMPS {
            if IMPORTED.contains(name) {
                continue;
            }
            let palest = stops
                .iter()
                .copied()
                .max_by(|a, b| lum(a).partial_cmp(&lum(b)).unwrap())
                .unwrap();
            assert!(
                contrast(palest) >= 2.10,
                "{name}: palest stop {palest} holds only {:.2}:1 against the panel — \
                 `RAMP_BLUE` sets the bar at 2.10",
                contrast(palest)
            );
        }
    }
}
