pub(crate) mod encode;
pub(crate) mod geo;
pub(crate) mod globe;
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

/// Every row one layer draws, for fitting the scales its channels are read by.
///
/// A writer is handed a **share** of its layer: one panel of a facet, one moment
/// of a `play` sequence, one pass of a selection. A scale fitted over the share is
/// a different scale in every share, and the plot has one legend. That drew
/// Australia at China's size in the Oceania panel of a faceted bubble chart,
/// drew each year's most populous country at the largest size in a played one,
/// and gave a dash to a different category in a panel that lacked the first.
/// So the scale is fitted here, over every share at once, and a writer reads its
/// share's rows against it. The legend reads the same fit.
///
/// The categorical color map already worked this way, for the reason given where
/// it is built: a reader tracks a mark by what it looks like, and nothing on the
/// page would say the mapping had changed under them.
pub(crate) struct Whole<'a> {
    /// The layer's frame before a facet, a moment or a selection took a share of
    /// it, in the table's own order. A category's place comes from here, as a
    /// color does, so splitting the rows into panels cannot reorder them.
    pub(crate) unsplit: &'a DataFrame,
    /// Each panel's frame at each moment: the rows the layer actually draws. A
    /// range is fitted over these rather than over `unsplit`, because a
    /// transform run per panel yields values the unsplit frame never holds.
    pub(crate) shares: Vec<&'a DataFrame>,
}

impl<'a> Whole<'a> {
    /// A frame drawn in one piece, which is its own whole.
    #[cfg(test)]
    pub(crate) fn of(df: &'a DataFrame) -> Self {
        Whole { unsplit: df, shares: vec![df] }
    }

    /// The scale `field` is read by: its range over every share, with the
    /// binding's stated ends laid over it.
    pub(crate) fn scale(
        &self, field: &str, def: Option<&crate::ir::ChannelDef>,
    ) -> crate::scale::ChannelScale {
        let vals: Vec<f64> = self.shares.iter()
            .filter_map(|df| df.float_col(field))
            .flat_map(|col| col.iter().copied())
            .collect();
        crate::scale::ChannelScale::of(&vals, def)
    }

    /// Whether any share holds a number in `field`, which is when
    /// [`scale`](Self::scale) has a range to fit.
    pub(crate) fn has_numbers(&self, field: &str) -> bool {
        self.shares.iter().any(|df| df.float_col(field).is_some_and(|col| !col.is_empty()))
    }

    /// The scale a channel `layer` binds is read by, or an unbound one when the
    /// layer maps no numbers to it.
    pub(crate) fn channel_scale(
        &self, layer: &crate::ir::Layer, channel: &Channel,
    ) -> crate::scale::ChannelScale {
        match layer.encodings.get(channel) {
            Some(def) if self.has_numbers(&def.field) => self.scale(&def.field, Some(def)),
            _ => crate::scale::ChannelScale::unbound(),
        }
    }

    /// The order `field`'s categories take: the unsplit table's, then any a
    /// transform made that the table does not hold.
    pub(crate) fn categories(&self, field: &str) -> Vec<String> {
        let frames: Vec<&DataFrame> = std::iter::once(self.unsplit)
            .chain(self.shares.iter().copied())
            .collect();
        crate::data::categories_across(&frames, field)
    }
}

/// A deterministic `u64 → [0, 1)` hash (SplitMix64's finalizer). Pure and
/// dependency-free — the seeded-from-data rule the whole engine follows for
/// anything that must look arbitrary yet render identically every run.
///
/// It lives here rather than in a mark because two marks need it and neither owns
/// it: `jitter` spreads a strip plot's points from it, and `repel` parts two labels
/// that landed on the exact same pixel, where there is no direction in the data to
/// part along. Both are the same rule — one spec is one picture, so anything that
/// must *look* arbitrary is a function of the row rather than of the clock.
pub(crate) fn hash01(mut z: u64) -> f64 {
    z = z.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    // Top 53 bits → the same resolution an f64 mantissa carries.
    (z >> 11) as f64 / ((1u64 << 53) as f64)
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
    /// What the ticks were chosen over, in the scale's units: the data's span,
    /// stretched to a bar's baseline where there is one. Narrower than `range`
    /// by the breathing margin, and the page needs both: two plots sharing an
    /// axis share the range, and choose their ticks over the union of these, so
    /// a shared axis is ticked as each plot alone would tick it rather than as
    /// the wider range rounds.
    pub(crate) ticks_over: (f64, f64),
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

