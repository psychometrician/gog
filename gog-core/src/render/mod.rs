pub(crate) mod encode;
pub(crate) mod geo;
pub mod layout;
pub(crate) mod page;
pub(crate) mod pattern;
pub mod legend;
pub mod palette;
pub(crate) mod nest;
pub(crate) mod polar;
pub mod project;
pub mod shape;
pub mod svg;
pub mod text;
pub mod ticks;
pub(crate) mod marks;

use crate::data::DataFrame;
use crate::ir::{Channel, PlotSpec};
use crate::legality::Diagnostic;
use std::collections::HashMap;

/// Shared context passed to every renderer.
pub struct RenderContext<'a> {
    pub spec: &'a PlotSpec,
    pub data: &'a HashMap<String, DataFrame>,
}

impl<'a> RenderContext<'a> {
    pub fn new(spec: &'a PlotSpec, data: &'a HashMap<String, DataFrame>) -> Self {
        Self { spec, data }
    }

    /// Resolve the data table for a layer (layer-local first, then plot-level).
    pub fn resolve_data(&self, layer_data: &Option<String>) -> Option<&DataFrame> {
        let name = layer_data.as_ref().or(self.spec.data.as_ref())?;
        self.data.get(name)
    }

    /// The name a coordinate axis (x, y, z) goes by.
    ///
    /// The plot's binding when there is one, else the first layer that names its
    /// own — see [`PlotSpec::axis_def`]. Every layer's column has been resolved
    /// onto this name by the time a frame is read, so one name per axis still
    /// describes the whole plot; what a layer can differ in is only which column
    /// of *its* table supplies the values (spec §8).
    pub fn coord_field(&self, channel: &Channel) -> Option<&str> {
        self.spec.axis_def(channel).map(|c| c.field.as_str())
    }
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

pub(crate) struct Layout {
    pub(crate) x0: f64,
    pub(crate) y0: f64,
    pub(crate) x1: f64,
    pub(crate) y1: f64,
}

// ---------------------------------------------------------------------------
// What a render says about itself
// ---------------------------------------------------------------------------

/// A drawn plot, and the two facts a *page* needs back from it.
///
/// A composed plot is drawn twice: once to find out where it would put its
/// panels and what its axes measure, and once for real, with the page's answer
/// (`render::page`). The measuring pass is a whole render because the panel
/// rectangle is the *end* of the layout — it depends on the tick labels, which
/// depend on the ticks, which depend on the transformed frames. Anything cheaper
/// would be a second implementation of the layout, drifting from the first.
pub(crate) struct Drawn {
    pub(crate) svg: String,
    /// The panel area, in this plot's own coordinates. The rectangle a page
    /// intersects with its siblings' to fit a shared axis.
    pub(crate) panel: Layout,
    pub(crate) x: AxisFacts,
    pub(crate) y: AxisFacts,
    /// What drawing the plot found that the legality check could not.
    ///
    /// **A stage that can drop something has to be able to say so** (§12), and a
    /// few facts are only knowable once the page has a size: whether a label fits
    /// the region it names is the first of them, since the region comes from the
    /// panel and the ink from the font. `legality::check` has neither, and giving
    /// it a layout to reason about would be the renderer written twice.
    ///
    /// These are never fatal — the plot drew. They ride back to `plot::Drawing`
    /// beside the check's own, on the rule that a caller reports one list.
    pub(crate) remarks: Vec<Diagnostic>,
}

/// What one axis of a drawn plot turned out to measure.
#[derive(Debug, Clone, Default)]
pub(crate) struct AxisFacts {
    /// The column on it, empty when nothing is bound. Two plots share an axis
    /// when this matches — the rule the whole of composition rests on.
    pub(crate) field: String,
    /// The range the axis ran over, in the units the *scale* works in — decades
    /// on a log axis, seconds on a calendar, slot indices on a categorical one.
    pub(crate) range: (f64, f64),
    /// The categories, in order, when the axis is categorical.
    pub(crate) cats: Option<Vec<String>>,
    /// The base, when the axis is logarithmic. A stated domain arrives in the
    /// data's own units (spec §10), so a shared range has to be converted back
    /// out of decades before it can be handed to the other plot.
    pub(crate) log_base: Option<f64>,
    /// Does this axis measure in units that are **not the column's own**?
    ///
    /// True for exactly one space today: a `map` reprojects its frames before the
    /// scales are fitted, so `range` is in projected units while `lon` and `lat`
    /// are still degrees.
    ///
    /// It exists because a page shares a scale by writing `limits`, and `limits`
    /// does **two jobs at once**: it selects rows, in the column's own units, and
    /// it sets the scale, in the scale's. On every other axis those are the same
    /// units and the double duty is invisible. On a map they diverge, and neither
    /// choice is right — degrees select correctly and scale wrongly, projected
    /// numbers scale correctly and exclude every row. So a page reads this and
    /// declines to share the scale at all, rather than picking a side.
    ///
    /// [`log_base`](Self::log_base) looks like the same problem and is not: decades
    /// convert back by `base.powf`, so one range serves both jobs. A projection
    /// mixes the two axes together, so no per-axis formula recovers degrees from it.
    ///
    /// Before this, composing two maps wrote each cell a domain of projected
    /// numbers against a degree column, excluding every row and drawing two empty
    /// panels. Silently, because a page injects that domain *after*
    /// `check_limit_rows` — the check that refuses this exact mistake, in those
    /// words, when a reader makes it by hand.
    pub(crate) projected: bool,
}

impl Layout {
    pub(crate) fn w(&self) -> f64 { self.x1 - self.x0 }
    pub(crate) fn h(&self) -> f64 { self.y1 - self.y0 }

    pub(crate) fn map_x(&self, v: f64, smin: f64, smax: f64) -> f64 {
        let span = (smax - smin).max(1e-12);
        self.x0 + (v - smin) / span * self.w()
    }

    pub(crate) fn map_y(&self, v: f64, smin: f64, smax: f64) -> f64 {
        let span = (smax - smin).max(1e-12);
        self.y1 - (v - smin) / span * self.h()
    }
}

