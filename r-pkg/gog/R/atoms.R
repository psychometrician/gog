# atoms.R — Mark constants and channel-encoding functions
#
# Marks are pre-built singleton objects (consonants in the grammar).
# Channel functions create atoms that bind to the most recent mark.

# ---------------------------------------------------------------------------
# Masked base-R names — one helper, used by every atom that can prove the mistake
# ---------------------------------------------------------------------------
# Nineteen of this package's exports are also names in the attached base
# packages, and they divide in two. Nine are *objects* (`mean`, `sum`, `line`,
# `text` and the rest of the mark and transform constants), and R skips a
# non-function binding when it resolves a call, so `mean(x)` still reaches
# `base::mean` and those nine cannot break anything. Ten are *functions* and do
# take over: `box`, `data`, `density`, `jitter`, `order`, `palette`,
# `quantile`, `range`, `stack`, `title`.
#
# For those eight the collision costs one thing only — not knowing which
# function answered. So where the argument shape proves the caller meant the
# base one, the refusal names it. A whole column, or a table, arriving where an
# atom wants a single setting is that proof.
masked_hint <- function(value, qualified) {
  if (length(value) > 1L || is.data.frame(value) || is.list(value)) {
    paste0(" Data arrived where one setting belongs, so the call you want is ",
           "probably `", qualified, "()`, the function this atom's name masks.")
  } else {
    ""
  }
}

# ---------------------------------------------------------------------------
# A channel takes a column, never a value — the capture-side check
# ---------------------------------------------------------------------------
# `color("red")` deparses to the five characters `"red"`, quotes included, and
# used to travel to the engine as a column of that name — whose missing-column
# refusal then blamed the reader for what the binding lost. The other three
# bindings refuse a value where a channel wants a column; R's bare names simply
# move the check from an accessor to the capture. Only a *literal* is caught: a
# bare name is a column whatever the workspace holds under it, which is Law 4's
# whole point, and a call (`x(a + b)`) still deparses as written.
column_name <- function(sub, atom, settable = FALSE) {
  if (is.character(sub) && length(sub) == 1L && !is.na(sub)) {
    shown <- deparse(sub)
    direction <- if (grepl("^[A-Za-z.][A-Za-z0-9._]*$", sub)) {
      paste0("`", atom, "(", sub, ")` maps the column called `", sub, "`")
    } else {
      paste0("a column's bare name maps it, as in `", atom, "(gdp)`")
    }
    setting <- if (settable) {
      paste0("\n  To fix one value for the whole layer instead \u2014 no ",
             "legend, nothing to decode \u2014 that is a setting: `style(",
             atom, " = ", shown, ")`.")
    } else {
      ""
    }
    stop("gog: `", atom, "(", shown, ")` binds a *value*, and a channel takes ",
         "a *column*: ", direction, ".", setting, call. = FALSE)
  }
  if (is.numeric(sub) || is.logical(sub)) {
    stop("gog: `", atom, "(", deparse(sub), ")` binds a *value*, and a channel ",
         "takes a *column* \u2014 the bare name of one of the table's columns, ",
         "as in `", atom, "(gdp)`.", call. = FALSE)
  }
  deparse(sub)
}

# A viewing angle is one finite number of degrees — checked at the line the
# caller wrote, as Julia and JavaScript check it, so `space(turn = "left")` is
# refused here rather than shipped to the wire as a string. The engine checks
# too; a rule implemented in one binding is a rule the other three get wrong.
degrees <- function(value, atom, name) {
  if (!is.numeric(value) || length(value) != 1L || !is.finite(value)) {
    stop("gog: `", atom, "(", name, " = )` needs a single number of degrees.",
         call. = FALSE)
  }
  as.numeric(value)
}

# A label is one string — the check the other three bindings already run, so
# `x_label(42)` is refused where it was typed rather than drawn as text.
text_value <- function(value, atom) {
  if (!is.character(value) || length(value) != 1L || is.na(value)) {
    stop("gog: `", atom, "()` needs a string, e.g. `", atom,
         "(\"Life expectancy\")`.", call. = FALSE)
  }
  value
}

# ---------------------------------------------------------------------------
# Mark atoms — geometric forms
# ---------------------------------------------------------------------------

#' Marks -- the shapes a plot draws
#'
#' A **mark** is the shape a layer draws, and every plot needs exactly one. A mark
#' carries no data of its own: it is combined with **channels** that say which
#' column goes to which part of the picture, and the engine decides every stroke
#' from there.
#'
#' Marks are the grammar's consonants. They are plain objects rather than
#' functions, so they are written bare in a sentence:
#'
#' `data(table) + point + x(column) + y(column)`
#'
#' A mark and its required positions together are the smallest thing that draws;
#' neither renders alone. Which channels a mark accepts is a property of the mark,
#' and asking for one it cannot carry is refused with a direction rather than
#' silently ignored.
#'
#' @section The marks:
#' \describe{
#'   \item{`point`}{One glyph per row. How two measurements move together.}
#'   \item{`line`}{Connects rows in **x order**. How one value changed over time.}
#'   \item{`path`}{Connects rows in **data order** rather than x order, so a series
#'     may double back on itself.}
#'   \item{`area`}{Fills the region between the line and the baseline.}
#'   \item{`bar`}{A rectangle from the baseline to the value. How large each
#'     category is.}
#'   \item{`step`}{The line family's right-angled member: a value holds steady
#'     until it jumps.}
#'   \item{`interval`}{A segment from a low value to a high one. Needs a pair, so
#'     it is usually written with `range` or `confidence`.}
#'   \item{`ribbon`}{A band between two values across a span.}
#'   \item{`text`}{Draws the string given by `label` at each position.}
#'   \item{`rule`}{A reference line across the panel -- the threshold everything
#'     else is compared against.}
#'   \item{`zone`}{Shades a rectangular region of the panel.}
#'   \item{`surface`}{A sheet over a grid of samples, for three-dimensional data.}
#' }
#'
#' `box` is a mark too, but takes an argument, so it is documented separately.
#'
#' @format Each is a `gog_atom` object, used bare rather than called.
#' @seealso [box()] for the box-and-whisker mark; [x()] and [y()] for positions;
#'   [style()] for settings that do not vary with the data.
#' @examples
#' # A mark plus its positions is the smallest sentence that draws.
#' p <- data(mtcars) + point + x(wt) + y(mpg)
#'
#' # The same data, a different shape: one atom is all that changes.
#' q <- data(mtcars) + line + x(wt) + y(mpg)
#'
#' @name marks
NULL

#' @rdname marks
#' @export
point <- structure(list(type = "mark", mark = "point"), class = "gog_atom")

#' @rdname marks
#' @export
line  <- structure(list(type = "mark", mark = "line"),  class = "gog_atom")

# `path` is `line`'s twin, parting from it on one question: which order the
# vertices are visited in.  A line sorts by x, because it draws a function -- one
# y per x, read along a domain.  A path visits row 1, then row 2, and may double
# back, cross itself, or return where it started: the connected scatterplot, a
# trajectory over time, a route.  Two consequences.  Its two axes are the same
# kind of thing (two positions, neither a domain), so unlike `line` it takes
# either type on both.  And it takes almost no transform: a statistic replaces the
# rows, and the row order a path is does not survive that -- `path * mean` is
# refused toward `line`.
#
# The one exception is `density`, and it is an exception to that sentence rather
# than to the rule.  A path has no measure axis, so `density` on it cuts *both*
# axes (the rule that makes `zone * bin` a heatmap), and a field's iso-lines are
# not one summary per key -- they are vertices in the order they were traced, which
# is exactly what a path draws.  `path * density + x(a) + y(b)` is the contour
# plot; `density(levels = )` says how many.
#
# It is also the only mark that can carry `style(arrow = )`, because only a path
# has a direction: a line's last vertex is wherever the domain ends.
#' @rdname marks
#' @export
path  <- structure(list(type = "mark", mark = "path"),  class = "gog_atom")

# `rule` is placed by *one* position and spans the other axis, which the panel
# supplies -- the reference line at a threshold, and the rug tick at each
# observation.  One mark rather than ggplot2's three (`vline`/`hline`/`abline`),
# because a vertical and a horizontal reference line differ only in which axis
# carries the position, and the grammar reads that off the bindings the way it
# reads a bar's orientation.  One mark rather than two (rule + rug), because a
# rug tick is the same geometry reaching a shorter way: `style(reach = "edge")`.
#
# Its position is a *column*, never a bare number, which is the rule the whole
# grammar keeps -- and it pays: one table of thresholds draws every line at once.
# Which axis it lands on is read off which of the plot's two position columns the
# rule's own table holds, so a table with both (the plot's own data) is refused
# with direction rather than guessed at.  It takes no transform, having no
# measure to compute.
#' @rdname marks
#' @export
rule  <- structure(list(type = "mark", mark = "rule"),  class = "gog_atom")

# `zone` shades a rectangle -- `rule`'s sibling one dimension up.  Where a rule
# takes one position and spans the axis it does not name, a zone takes a *pair*
# and spans the axis it is not given a pair for; give it both pairs and it is a
# box.  Its sides ride `bounds`: `zone * bounds(lo, hi)` is a band across the
# panel, `zone * bounds(start = a, end = b)` a band up it, all four a box.
#
# A rectangle bounded on both axes already draws as `ribbon * bounds` over a
# two-row table.  What only a zone does is **reach the panel** on the axis you do
# not bound -- a ribbon stops at the numbers it was given, and padding them
# outward widens the axis instead, changing the plot in order to decorate it.
#
# One row is one rectangle, so one table of recessions draws every band at once --
# `rule`'s payoff for taking columns rather than numbers.
#
# It takes two other transforms, and they are the same question answered the other
# way: `bounds` *names* the sides, `bin` and `density` *cut* them.  `zone * bin +
# x(a) + y(b)` cuts both axes into cells and colors each by how many rows fell in
# it, which is the heatmap; `zone * density` estimates a value at each cell instead
# of counting, and `zone * density(levels = 6)` fills the bands between six contours
# of that estimate -- whose edges are the curves `path * density(levels = 6)` draws.
# A cell is a zone rather than a second bar-like mark because a bar's identity is a
# length from a baseline and a cell measures nothing by length: its measure is
# `color`, its extent its slot on each axis.  Rectangularity was never this mark's
# identity either -- a band is bounded by a curve, and `zone` is the region mark.
#' @rdname marks
#' @export
zone  <- structure(list(type = "mark", mark = "zone"),  class = "gog_atom")

# A filled region between the data and a baseline at zero.  Both axes measure,
# so there is no orientation question and no categorical form -- an area always
# fills downward to its baseline.
#' @rdname marks
#' @export
area  <- structure(list(type = "mark", mark = "area"),  class = "gog_atom")

#' @rdname marks
#' @export
bar   <- structure(list(type = "mark", mark = "bar"),   class = "gog_atom")

# `step` is the line family's staircase: it holds each value until it changes,
# then jumps.  `step * bin` draws a histogram as a silhouette outline; `step + x
# + y` draws a step function (a CDF, a survival curve, a rate that steps on the
# day it changed).  Same channels as `line`.
#' @rdname marks
#' @export
step  <- structure(list(type = "mark", mark = "step"),  class = "gog_atom")

# `interval` spans from a low value to a high one at each x -- the error-bar /
# range whisker.  Unlike `bar` (baseline to value) it floats between two extents,
# so it needs a range transform to supply them: its minimum syllable is
# `interval * range`.  Without one the engine refuses with direction.
#' @rdname marks
#' @export
interval <- structure(list(type = "mark", mark = "interval"), class = "gog_atom")

# `box` draws the box-and-whisker of y's distribution within each x group -- a box
# from the lower quartile to the upper, a line at the median, whiskers to the data,
# and (by default) the extreme points beyond 1.5*IQR drawn as outliers.  Unlike
# `interval` (which needs a range transform), `box` carries its own summary:
# `box + x(group) + y(value)` is its whole minimum syllable, no `*`.
#
# `box` is a *function* so it can take its one knob -- the whisker rule -- the way
# `bin`/`confidence` take theirs.  Called bare (`box + x + y`) the `+` operator
# invokes it with the default, so bare `box` and `box(whiskers = "range")` reach
# the same code path.  Masks `graphics::box` (a frame around a base plot); load gog
# after graphics -- the default -- so its name wins, or qualify `gog::box`.
#' Box-and-whisker mark.
#'
#' @param whiskers  `"tukey"` (default) runs the whiskers to the most extreme
#'   point within 1.5*IQR of the box and draws the points beyond as outliers --
#'   the standard box plot.  `"range"` runs them to the true minimum and maximum
#'   with no outliers (the plain five-number summary).
#' @export
box <- function(whiskers = NULL) {
  if (!is.null(whiskers) && (!is.character(whiskers) || length(whiskers) != 1 ||
                             !whiskers %in% c("tukey", "range"))) {
    stop("gog: `box(whiskers = )` is either \"tukey\" (the default \u2014 whiskers to ",
         "1.5*IQR, points beyond drawn as outliers) or \"range\" (whiskers to the ",
         "true min and max, no outliers).", call. = FALSE)
  }
  structure(
    list(type = "mark", mark = "box",
         box = if (is.null(whiskers)) NULL else list(whiskers = whiskers)),
    class = "gog_atom"
  )
}

# `ribbon` draws a filled band from a low boundary to a high one across x -- the
# confidence / spread band.  It is `area`'s fill (one region, no stroke, `opacity`
# a setting, `color` splits it) fed by `interval`'s machinery: it floats between
# two extents a range transform supplies, so its minimum syllable is `ribbon *
# range` (or `ribbon * confidence`).  Without one the engine refuses with
# direction, exactly as `interval` does.  x stays continuous -- a band has no gap
# to fill between categories.
#' @rdname marks
#' @export
ribbon <- structure(list(type = "mark", mark = "ribbon"), class = "gog_atom")

# `text` draws a string at each (x, y) -- a glyph mark whose glyph is the value
# of the `label` column, `point`'s sibling.  Its minimum syllable includes
# `label`: without it there is nothing to draw and the engine refuses with
# direction.  Masks base R's `graphics::text` (as gog already masks `data`,
# `range`, `order`); load gog after graphics -- the default -- so its name wins
# inside a spec, or qualify `gog::text`.
#' @rdname marks
#' @export
text  <- structure(list(type = "mark", mark = "text"),  class = "gog_atom")

# `surface` is a sheet through the samples, and the only mark that draws in the
# cube alone.  Its rows are *nodes*: the grid the two position columns describe is
# recovered rather than declared, and a face is drawn wherever all four corners of
# a cell are there.  So the table it wants is one row per (x, y) crossing -- the
# shape `expand.grid()` makes -- and a scatter is refused with direction rather
# than drawn as an empty panel.
#
# Three positions, all required and all numeric.  Required because a sheet with no
# height is not a surface, which also means it needs no `space()`: binding `z` is
# what puts a plot in the cube.  Numeric because a face asserts every value
# *between* two nodes, and between two categories there is nothing to assert --
# where a 3-D `bar` may stand in a categorical slot, because a column claims
# nothing about the space between cells.  For a mesh over categories that is the
# mark: `bar * bin + x(a) + y(b) + space()`.
#
# It takes one transform, `density`, and that makes it the third geometry of one
# field: `zone * density` paints it as cells, `path * density` traces its contours,
# `surface * density` raises it -- with the estimate becoming the height, so
# `surface * density + x(a) + y(b) + space()` needs no `z()`.  `bin` is refused,
# because it leaves empty cells out and a sheet cannot span a gap it cannot see.
#
# `style(border_color = )` draws the mesh lines over the sheet, and a measured
# `color` ramps it face by face -- which a `zone` can do and an `area` cannot,
# a face being small enough to hold one value where a region would need a gradient.
#' @rdname marks
#' @export
surface <- structure(list(type = "mark", mark = "surface"), class = "gog_atom")

# ---------------------------------------------------------------------------
# Transform atoms — statistical and spatial derivations
# Used with the * operator:  bar * bin,  line * smooth, etc.
# ---------------------------------------------------------------------------

# `bin` is the one transform that takes a parameter, so unlike its bare siblings
# it is a *function*. Called bare (`bar * bin`) it still works: `*.gog_atom`
# calls an un-called transform function with its defaults. Positional arg is the
# bin *count* (the common intent); a width is said out loud, `bin(width = 5)`.
#
# How many dimensions it cuts is read off the *mark*, not asked for: `bar * bin`
# cuts the one axis a bar leaves free, `zone * bin` cuts both, because a zone has
# no measure axis and needs an extent on each.  Same transform, same parameters.
#
# `tiling` is the mesh: how the plane is partitioned, not how a cell is painted.
# It belongs to `bin` rather than to the mark because a different mesh puts
# different rows in different cells, so it changes the counts (spec §5).  It
# means nothing to a one-dimensional bin, whose cells are intervals, and the
# engine refuses it there naming `zone`.
#
# `...` exists only to refuse: without it an unknown argument gets R's "unused
# argument", which says nothing about what gog does take.
#' Bin a continuous variable into equal-width buckets (the histogram transform).
#'
#' @param bins  Number of bins — one positive whole number. Positional: `bin(30)`.
#' @param width Bin width in the data's own units, e.g. `bin(width = 5)`.
#'   Mutually exclusive with `bins`.
#' @param tiling How the plane is partitioned when both axes are cut:
#'   `"rect"` (the default) or `"hex"`. Two-dimensional bins only —
#'   `zone * bin(tiling = "hex")`.
#' @param ...   Not used. Any argument here is refused with direction.
#' @export
bin <- function(bins = NULL, width = NULL, tiling = NULL, ...) {
  dots <- list(...)
  if (length(dots)) {
    nm <- names(dots)
    nm <- if (is.null(nm)) rep("", length(dots)) else nm
    stop("gog: `bin()` takes `bins`, `width` or `tiling` \u2014 `bin(30)` for a bin ",
         "count, `bin(width = 5)` for a bin width, `bin(tiling = \"hex\")` for a ",
         "hexagonal mesh. Got: `",
         paste(ifelse(nzchar(nm), nm, "<unnamed>"), collapse = "`, `"), "`.",
         call. = FALSE)
  }
  if (!is.null(tiling)) {
    if (!is.character(tiling) || length(tiling) != 1L || is.na(tiling)) {
      stop("gog: `bin(tiling = )` needs one name, `\"rect\"` or `\"hex\"`.",
           call. = FALSE)
    }
  }
  if (!is.null(bins) && !is.null(width)) {
    stop("gog: `bin()` takes either `bins` or `width`, not both. ",
         "Write `bin(30)` for a bin count or `bin(width = 5)` for a bin width.",
         call. = FALSE)
  }
  if (!is.null(bins)) {
    if (!is.numeric(bins) || length(bins) != 1L || is.na(bins) ||
        bins < 1 || bins != round(bins)) {
      stop("gog: `bin(bins = )` needs one positive whole number, e.g. `bin(30)`.",
           call. = FALSE)
    }
    bins <- as.integer(bins)
  }
  if (!is.null(width)) {
    if (!is.numeric(width) || length(width) != 1L || is.na(width) || width <= 0) {
      stop("gog: `bin(width = )` needs one positive number, e.g. `bin(width = 5)`.",
           call. = FALSE)
    }
    width <- as.numeric(width)
  }
  structure(
    list(type = "transform", transform = "bin", bins = bins, width = width,
         tiling = tiling),
    class = "gog_atom"
  )
}

#' Transforms -- statistics computed before the mark is drawn
#'
#' A **transform** replaces the rows with a summary of them, and the mark then
#' draws that summary. It is joined to a mark with `*`, which reads as
#' derivation: `bar * count` is a bar of counts, and it is where a histogram
#' comes from rather than being a chart type to look up.
#'
#' The same transform behaves identically on every mark it is legal on. Keep the
#' statistic and change the shape and the same numbers draw a line, an area or a
#' step -- that is the point of separating the two.
#'
#' @section The transforms:
#' \describe{
#'   \item{`count`}{The number of rows for each value of the grouping position.}
#'   \item{`proportion`}{Each group's share of the total, rather than its count.}
#'   \item{`sum`, `mean`, `median`, `max`, `min`}{The aggregate of the measure
#'     within each group.}
#'   \item{`range`}{The lowest and highest value in each group, as a pair -- which
#'     is what `interval` and `ribbon` need.}
#'   \item{`smooth`}{A fitted trend through the points rather than the points.}
#' }
#'
#' Transforms that take an argument are documented on their own pages:
#' [bin()], [density()], [confidence()], [bounds()], [partition()].
#' [stack()], [dodge] and [jitter()] are a different kind of atom -- they resolve
#' the *overlap* of marks rather than computing a statistic.
#'
#' @format Each is a `gog_atom` object, used bare rather than called.
#' @seealso [marks] for the shapes these are drawn with; [stack()] for resolving
#'   overlap.
#' @examples
#' # A histogram is a bar of binned counts -- derived, not a chart type.
#' h <- data(mtcars) + bar * count + x(cyl)
#'
#' # The same statistic, a different shape.
#' l <- data(mtcars) + line * count + x(cyl)
#'
#' # An aggregate of a measure within each group.
#' m <- data(mtcars) + bar * mean + x(cyl) + y(mpg)
#'
#' @name transforms
NULL

#' @rdname transforms
#' @export
smooth  <- structure(list(type = "transform", transform = "smooth"),  class = "gog_atom")

#' @rdname transforms
#' @export
count   <- structure(list(type = "transform", transform = "count"),   class = "gog_atom")

# Like `bin`, `density` carries a parameter, so it is a *function*. Called bare
# (`line * density`) it still works: `*.gog_atom` calls the un-called transform
# function with its defaults → Silverman's automatic bandwidth. Positional arg is
# the *adjust* multiplier (the common intent — "twice as smooth"); an absolute
# bandwidth is said out loud, `density(bandwidth = 5)`.
#' Estimate the smooth probability density of a continuous variable (a KDE).
#'
#' @param adjust    Multiplier on the automatically-chosen bandwidth: `density(2)`
#'   is twice as smooth, `density(0.5)` rougher. Positional: `density(2)`.
#' @param bandwidth Absolute bandwidth in the data's own units, e.g.
#'   `density(bandwidth = 5)`. Mutually exclusive with `adjust`, and meaningless
#'   on the two-dimensional reading, where the two axes carry different
#'   quantities — refused there toward `adjust`.
#' @param levels    How many levels to cut a field into: `path * density(levels = 8)`
#'   traces their boundaries, `zone * density(levels = 8)` fills between them.
#'   Refused on a density *curve*, which is one line with nothing to cut into.
#' @param compare   What a **violin**'s width means from one slot to the next:
#'   `"count"` (the default) scales each violin by its group's row count, so a thin
#'   violin is a small group and the widths mean one thing across the panel;
#'   `"shape"` draws every violin to the same area, comparing shapes and nothing
#'   else. Meaningful only in the violin reading
#'   (`ribbon * density + x(<category>) + y(<number>)`), and refused on the curve
#'   and the field, which have no slots to compare.
#' @param reach     How far each violin reaches from the line its category sits on,
#'   **in slots**: `0.4` (the default) keeps every shape inside its own slot — two
#'   facing violins fill four fifths of the space between their categories, a
#'   categorical bar's own rule — and past `0.5` they run into their neighbors,
#'   which is the ridgeline plot
#'   (`area * density(reach = 2.5) + x(<number>) + y(<category>)`). Measured one
#'   way, so a `ribbon` reaches it on each side and an `area` on one — which keeps
#'   the half violin exactly half of the violin at any value. Violin-only, like
#'   `compare`.
#' @export
density <- function(adjust = NULL, bandwidth = NULL, levels = NULL, compare = NULL,
                    reach = NULL) {
  if (!is.null(adjust) && !is.null(bandwidth)) {
    stop("gog: `density()` takes either `adjust` or `bandwidth`, not both. ",
         "Write `density(2)` to scale the automatic bandwidth, or ",
         "`density(bandwidth = 5)` to set it in the data's own units.",
         call. = FALSE)
  }
  if (!is.null(adjust)) {
    if (!is.numeric(adjust) || length(adjust) != 1L || is.na(adjust) || adjust <= 0) {
      stop("gog: `density(adjust = )` needs one positive number, e.g. `density(2)`.",
           masked_hint(adjust, "stats::density"), call. = FALSE)
    }
    adjust <- as.numeric(adjust)
  }
  if (!is.null(bandwidth)) {
    if (!is.numeric(bandwidth) || length(bandwidth) != 1L || is.na(bandwidth) ||
        bandwidth <= 0) {
      stop("gog: `density(bandwidth = )` needs one positive number, ",
           "e.g. `density(bandwidth = 5)`.", call. = FALSE)
    }
    bandwidth <- as.numeric(bandwidth)
  }
  # A count of contours, so the same shape of check `bin(bins = )` makes: one
  # positive whole number. Which *readings* it is legal in is the engine's
  # question, not the binding's — `legality::check_density_params` refuses it on a
  # curve with direction, so this only rejects what could never be a count.
  if (!is.null(levels)) {
    if (!is.numeric(levels) || length(levels) != 1L || is.na(levels) ||
        levels < 1 || levels != round(levels)) {
      stop("gog: `density(levels = )` needs one positive whole number, ",
           "e.g. `path * density(levels = 8)`.", call. = FALSE)
    }
    levels <- as.integer(levels)
  }
  # One of two words, checked here only for *shape* — one string — since which
  # readings it means anything in is the engine's question, as with `levels`. The
  # word itself is the engine's too: `check_density_params` refuses an unknown one
  # by name, so a typo gets the same message in all four bindings rather than four.
  if (!is.null(compare)) {
    if (!is.character(compare) || length(compare) != 1L || is.na(compare)) {
      stop("gog: `density(compare = )` takes one word \u2014 \"shape\" or \"count\".",
           call. = FALSE)
    }
  }
  # A positive number of slots. Which *readings* it is legal in is the engine's
  # question, as with `levels` and `compare`; this only rejects what could never
  # be a distance.
  if (!is.null(reach)) {
    if (!is.numeric(reach) || length(reach) != 1L || is.na(reach) || reach <= 0) {
      stop("gog: `density(reach = )` needs one positive number of slots, ",
           "e.g. `density(reach = 2.5)` for overlapping ridges.", call. = FALSE)
    }
    reach <- as.numeric(reach)
  }
  structure(
    list(type = "transform", transform = "density", adjust = adjust,
         bandwidth = bandwidth, levels = levels, compare = compare, reach = reach),
    class = "gog_atom"
  )
}

#' @rdname transforms
#' @export
sum     <- structure(list(type = "transform", transform = "sum"),     class = "gog_atom")

#' @rdname transforms
#' @export
mean    <- structure(list(type = "transform", transform = "mean"),    class = "gog_atom")

#' @rdname transforms
#' @export
median  <- structure(list(type = "transform", transform = "median"),  class = "gog_atom")

#' @rdname transforms
#' @export
max     <- structure(list(type = "transform", transform = "max"),     class = "gog_atom")

#' @rdname transforms
#' @export
min     <- structure(list(type = "transform", transform = "min"),     class = "gog_atom")

#' @rdname transforms
#' @export
proportion <- structure(list(type = "transform", transform = "proportion"), class = "gog_atom")

# `quantile` is the aggregation family's seventh member, and the only one whose
# parameter is required rather than defaulted: the sensible default would be the
# middle, and that already has a plain name.  It masks `stats::quantile`, and the
# refusal splits by what it was given for the reason `range`'s does -- a vector
# where a probability belongs is that other function being reached.
#' The p-th quantile of `y` per group.
#'
#' @param p The quantile probability, one number between 0 and 1.  Positional:
#'   `quantile(0.9)` is the 90th percentile.  There is no default; at 0, 0.5 and
#'   1 the plot draws and says that `min`, `median` and `max` are the plain names
#'   for the same numbers.
#' @export
quantile <- function(p = NULL) {
  if (!is.null(p)) {
    if (!is.numeric(p) || length(p) != 1L || is.na(p)) {
      stop("gog: `quantile()` takes one number between 0 and 1, the probability ",
           "it reduces to, e.g. `quantile(0.9)`. For the quantiles of a vector, ",
           "gog masks that name: use `stats::quantile()`.", call. = FALSE)
    }
    if (p < 0 || p > 1) {
      stop("gog: `quantile(", p, ")` is not a probability \u2014 a quantile is ",
           "between 0 and 1. `quantile(0.9)` is the 90th percentile, ",
           "`quantile(0.5)` the middle.", call. = FALSE)
    }
    p <- as.numeric(p)
  }
  structure(list(type = "transform", transform = "quantile", p = p),
            class = "gog_atom")
}

# `range` reduces y to a band within each x group -- the two extents an `interval`
# spans.  It carries the band's two ends, so like `bin`/`density`/`confidence` it
# is a *function*: called bare (`interval * range`) the band is the whole group,
# its minimum to its maximum, which is what `range` has always drawn.
# `range(0.25, 0.75)` is the interquartile band instead.
#
# Like `sum`/`mean`/`min`/`max` it masks the base function of the same name; use
# `base::range()` for that.  A reading transform: it needs `y()`.
#' Quantile band per group, for the span marks.
#'
#' @param low,high The band's two ends, as quantile probabilities between 0 and
#'   1.  Positional: `range(0.25, 0.75)` is the middle half.  An unset end is
#'   that side's extreme, so bare `range` is the whole group and
#'   `range(high = 0.9)` runs from the minimum to the 90th percentile.
#' @export
range <- function(low = NULL, high = NULL) {
  # A number apiece, and nothing else.  Two different mistakes reach here and
  # they want different directions, so the message splits.  A scalar out of
  # 0..1 is a mistyped quantile.  Anything else -- a vector, a string -- is
  # almost always `base::range(x)` landing on gog's name, and telling *that*
  # caller about quantiles would answer a question they did not ask.
  for (nm in c("low", "high")) {
    v <- get(nm)
    if (is.null(v)) next
    quantile_shaped <- is.numeric(v) && length(v) == 1L && !is.na(v)
    if (!quantile_shaped) {
      stop("gog: `range()` takes the band's two ends, each one number between 0 ",
           "and 1, e.g. `range(0.25, 0.75)`. It was given ",
           if (is.numeric(v)) paste(length(v), "numbers") else "something that is not a number",
           ". For the smallest and largest of a vector, gog masks that name: use ",
           "`base::range()`.", call. = FALSE)
    }
    if (v < 0 || v > 1) {
      stop("gog: `range(", nm, " = ", v, ")` is not a probability \u2014 the band's ",
           "ends are quantiles, so each is between 0 and 1. `range(0.25, 0.75)` ",
           "is the middle half, `range(0.1, 0.9)` the middle 80 percent, and bare ",
           "`range` the whole group.", call. = FALSE)
    }
    assign(nm, as.numeric(v))
  }
  structure(
    list(type = "transform", transform = "range", low = low, high = high),
    class = "gog_atom"
  )
}

# `confidence` carries a level, so like `bin`/`density` it is a *function*.
# Called bare (`interval * confidence`) it uses 0.95; `confidence(0.99)` widens
# it.  Computes the mean's confidence interval (mean +/- t * se) per group, with
# the mean as the center.  A reading transform: it needs `y()`.
# `deviation` is `confidence`'s twin: the same low/high pair with a center, and a
# different question.  `confidence` says how well the mean is pinned down;
# `deviation` says how spread the data is.  Drawing them as the same whisker is
# the error the `interval` chapter exists to name, so both are written out.
#' Spread band per group, mean +/- k standard deviations.
#'
#' @param multiplier How many standard deviations reach each side of the mean;
#'   default 1.  Positional: `deviation(2)`.
#' @export
deviation <- function(multiplier = NULL) {
  if (!is.null(multiplier)) {
    if (!is.numeric(multiplier) || length(multiplier) != 1L || is.na(multiplier) ||
        multiplier <= 0) {
      stop("gog: `deviation(multiplier = )` needs one positive number \u2014 it counts ",
           "standard deviations out from the mean. `deviation` is one, ",
           "`deviation(2)` is two.", call. = FALSE)
    }
    multiplier <- as.numeric(multiplier)
  }
  structure(
    list(type = "transform", transform = "deviation", multiplier = multiplier),
    class = "gog_atom"
  )
}

#' Confidence interval of the mean per group, for the `interval` mark.
#'
#' @param level Confidence level, one number strictly between 0 and 1; default
#'   0.95.  Positional: `confidence(0.99)`.
#' @export
confidence <- function(level = NULL) {
  if (!is.null(level)) {
    if (!is.numeric(level) || length(level) != 1L || is.na(level) ||
        level <= 0 || level >= 1) {
      stop("gog: `confidence(level = )` needs one number strictly between 0 and 1, ",
           "e.g. `confidence(0.95)`.", call. = FALSE)
    }
    level <- as.numeric(level)
  }
  structure(
    list(type = "transform", transform = "confidence", level = level),
    class = "gog_atom"
  )
}

# `bounds(lower, upper)` reshapes two *pre-computed* columns into the low/high pair
# the span marks (`interval`, `ribbon`) read -- the non-computing counterpart to
# `range`.  It computes nothing: the two columns you already have (a model's SE, a
# psychometric CSEM, a bootstrap interval) *are* the extents, so a confidence band
# whose bounds came from upstream draws with no `ymin`/`ymax` channels.  Column
# names are bare, like a channel's: `ribbon * bounds(lo, hi) + x(score)`.
#
# `start`/`end` are the *second* pair, and only a `zone` has an axis to use them
# on: a rectangle is bounded along the domain as well as the measure, where a band
# spans the measure at each position and has no domain extent at all.  They are
# deliberately not called "left"/"right" -- that would bake in horizontality, and
# this grammar reads orientation off the bindings.  Every pair is optional here
# because which ones a mark needs is the mark's question: a band requires
# lower/upper, a zone requires at least one complete pair and takes the axis it is
# not given from the panel.
#' Pre-computed bounds.
#'
#' @param lower,upper  Columns bounding the **measure** axis — the span every band
#'   mark draws.  Required by `ribbon`/`interval`/`line`/`step`.
#' @param start,end  Columns bounding the **domain** axis.  `zone` only: they give
#'   a rectangle its other two sides.  Omit a pair and a `zone` spans the panel on
#'   that axis, which is the whole reason the mark exists.
#' @export
bounds <- function(lower, upper, start, end) {
  lo <- if (missing(lower)) NULL else deparse(substitute(lower))
  hi <- if (missing(upper)) NULL else deparse(substitute(upper))
  st <- if (missing(start)) NULL else deparse(substitute(start))
  en <- if (missing(end))   NULL else deparse(substitute(end))
  if (is.null(lo) && is.null(hi) && is.null(st) && is.null(en)) {
    stop("gog: `bounds()` needs column names \u2014 `bounds(lower, upper)` bounds the ",
         "measure axis, and on a `zone` `bounds(start = a, end = b)` bounds the domain ",
         "axis.", call. = FALSE)
  }
  structure(
    list(type = "transform", transform = "bounds",
         lower = lo, upper = hi, start = st, end = en),
    class = "gog_atom"
  )
}

#' Divide a whole among nested parts — one ring per level of a hierarchy.
#'
#' The hierarchy arrives as **columns**, outermost first: one row of the table is
#' one leaf, and `partition(group, item, detail)` says which columns spell the
#' path down to it.  A blank level ends that branch early, which is what gives a
#' real hierarchy its ragged rim.
#'
#' `zone * partition(...)` flat is the **icicle**; the same sentence `+ polar()`
#' is the **sunburst**, and that one atom is the whole difference.  `text *
#' partition(...) + label(name)` names each node where it sits, reading the center
#' the same computation published.
#'
#' What each branch is weighed by rides on `x`: `+ x(amount)` apportions that
#' column, and binding nothing at all makes every leaf weigh 1.  Compose with
#' `proportion` for shares.  `y(depth, limits = c(0, 4))` puts the hole in the
#' middle, `depth` being the ring the transform synthesizes.
#'
#' Every number belongs to a **leaf**.  An interior node carrying one of its own
#' is ambiguous — is its arc the children's total, or its own beside them? — so
#' it is refused rather than guessed.
#'
#' `cross = TRUE` turns the levels across each other instead of down one axis:
#' the first divides the width, the second divides the height *within* each of
#' those columns.  That is the **mosaic**, and because both directions are then
#' spent on the hierarchy there is no ring left to step and only the leaves are
#' drawn.
#'
#' @param ...   The hierarchy's columns, bare names, outermost first.
#' @param cross Cross the levels rather than nesting them down one axis —
#'   `partition(decade, theme, cross = TRUE)` is the mosaic.  Must be written
#'   out in full; it sits after `...`, so R will not match it by prefix.
#' @export
partition <- function(..., cross = FALSE) {
  levels <- vapply(as.list(substitute(list(...)))[-1L], deparse, character(1))
  if (!length(levels)) {
    stop("gog: `partition()` needs the hierarchy's columns, outermost first \u2014 ",
         "`partition(group, item, detail)` puts `group` on the innermost ring and ",
         "`detail` on the rim.", call. = FALSE)
  }
  if (!is.logical(cross) || length(cross) != 1L || is.na(cross)) {
    stop("gog: `partition(cross = )` is TRUE or FALSE \u2014 TRUE crosses the levels ",
         "(the mosaic: the first divides the width, the second the height within ",
         "each column), FALSE nests them down one axis (the icicle, and the ",
         "sunburst in `polar()`).", call. = FALSE)
  }
  structure(
    list(type = "transform", transform = "partition", levels = levels,
         cross = cross),
    class = "gog_atom"
  )
}

# `dodge` is the first *collision modifier* (not a statistic): where a `color`
# split would stack several marks at one shared position, it sets them side by
# side within that position's slot.  Legal on the width-bearing marks it can
# subdivide — `bar`, `box`, `interval` — and refused with direction elsewhere
# (`point` -> `jitter`, `line`/`area` -> `stack`).  It carries no parameter (the
# width comes from the slot), so like `count`/`smooth` it is a bare atom, and it
# rides the `*` slot uniformly: `bar * count * dodge`, `box * dodge`,
# `interval * range * dodge`.  A common English word, masking nothing in base R.
#' Resolving overlap -- `stack`, `dodge` and `jitter`
#'
#' When several marks land in the same place, something has to decide where they
#' go. These are **collision modifiers**: they do not compute a statistic, they
#' move marks that would otherwise cover each other. Like transforms they ride the
#' `*` slot, so they compose with a statistic rather than replacing it --
#' `bar * count * stack` bins, counts, and then piles the result.
#'
#' \describe{
#'   \item{`stack`}{Piles the marks of each group end to end, so the total is the
#'     height of the pile.}
#'   \item{`dodge`}{Places them side by side instead, so each is measured from the
#'     same baseline and they can be compared directly.}
#' }
#'
#' Both need something to separate: a `color` or `group` binding that says which
#' mark belongs to which group. Without one there is nothing to stack or dodge,
#' and the engine says so.
#'
#' @param share `TRUE` fills every pile to the same height, so the plot shows each
#'   group's share rather than its amount. Defaults to `FALSE`.
#' @param baseline Where the pile starts: `"zero"` (the default), `"center"` for a
#'   stream centered on its own middle, or `"wiggle"` to minimize the movement of
#'   the bands.
#' @return A `gog_atom` joined to a mark with `*`.
#' @seealso [jitter()], which resolves overlap by displacing marks slightly rather
#'   than by arranging them; [transforms] for the statistics these compose with.
#' @examples
#' # Piled: the height of the pile is the total.
#' s <- data(mtcars) + bar * count * stack + x(cyl) + color(gear)
#'
#' # Side by side: each bar measured from the same baseline.
#' d <- data(mtcars) + bar * count * dodge + x(cyl) + color(gear)
#'
#' # Every pile the same height, so the plot shows shares.
#' f <- data(mtcars) + bar * count * stack(share = TRUE) + x(cyl) + color(gear)
#'
#' @name stack
#' @rdname stack
#' @export
dodge   <- structure(list(type = "transform", transform = "dodge"),   class = "gog_atom")

# `stack` is `dodge`'s sibling — the collision modifier that accumulates along the
# *measure* axis.  It hands every mark the same span, from a foot to a top, and
# each mark draws that span as its own geometry: `bar` and `area` fill it (each
# group sitting on the cumulative height of the ones below), and `point`, having
# no length to stretch, spends it on how many dots there are — the dot plot,
# `point * bin * stack`.  Refused with direction elsewhere (`line`/`step` ->
# `area`, `box`/`interval` -> `dodge`).  Used bare it takes no parameter:
# `bar * count * stack`, `area * sum * stack`, `point * bin * stack`.  What it
# needs beside itself differs by mark for one reason — a bar element *is* a
# quantity, so a `color` split is what piles; a point element is a *row*, so a
# counting transform (`bin`/`count`) is, and no split is needed.  It masks
# `utils::stack` (the data-frame reshaper), the same way `range`/`max`/`min`
# already mask their base-R namesakes — a DSL keeps its own vocabulary.
#
# `stack(share = TRUE)` fills every pile to 1 — the 100% stacked bar.  A
# parameter here rather than a fourth collision modifier or a second reading of
# `proportion`, because filling is a *position* adjustment: it changes where the
# marks sit and what the scale reads, never what was counted.  That is also why
# it composes with any measurement — `bar * sum * stack(share = TRUE)` is a
# share of summed revenue, which `proportion` could not say, having no column to
# sum.  The two normalizers part on the denominator: `proportion` divides by the
# whole frame's total and so still says how big each slot is, this divides by
# the slot's own and deliberately throws that away.
#
# `stack(baseline = )` says where each pile *hangs*, which is the other free
# choice once the heights are fixed.  `"zero"` stands every pile on the axis (the
# plain stacked bar, and the default), `"center"` hangs each so its middle is at
# zero, `"wiggle"` chooses the foot that makes the bands as flat as it can — the
# streamgraph.  Orthogonal to `share`, which scales the heights rather than
# placing them, so the two compose.
#
# A *function*, where `dodge` is a bare object, because it now takes a knob —
# `spec.R` invokes a bare transform for exactly this case, so `bar * count *
# stack` still reads as it always did.
#' @rdname stack
#' @export
stack <- function(share = NULL, baseline = NULL) {
  if (!is.null(share)) {
    if (!is.logical(share) || length(share) != 1L || is.na(share)) {
      stop("gog: `stack(share = )` is TRUE or FALSE \u2014 TRUE fills every pile to 1 ",
           "(the 100% stacked bar), FALSE piles the values themselves. For shares ",
           "of the whole plot rather than of each slot, `proportion` is the ",
           "transform you want.", masked_hint(share, "utils::stack"), call. = FALSE)
    }
    share <- as.logical(share)
  }
  if (!is.null(baseline)) {
    if (!is.character(baseline) || length(baseline) != 1L || is.na(baseline)) {
      stop("gog: `stack(baseline = )` is one of \"zero\", \"center\" or \"wiggle\" \u2014 ",
           "\"zero\" stands every pile on the axis, \"center\" hangs each pile so its ",
           "middle is at zero, \"wiggle\" chooses the foot that makes the bands as ",
           "flat as it can (the streamgraph).", call. = FALSE)
    }
  }
  structure(
    list(type = "transform", transform = "stack", share = share,
         baseline = baseline),
    class = "gog_atom"
  )
}

# `jitter` is the trio's third collision modifier — the offset for a mark with no
# width to subdivide (`dodge`), along an axis with no magnitude to spend: `point`
# on a *category*.  (Along a *measure* axis the same mark takes `stack` instead,
# which piles its points into countable dots.)  A strip plot's coincident points
# land on one line; `jitter` spreads them apart.
# It nudges *only* a categorical position axis, never one carrying a measured
# value (moving that would falsify it), and it needs no `color` split — it
# resolves same-position overlap of individual points.  Legal on `point` alone;
# every other mark is refused with direction (the width marks -> `dodge`).
#
# `amount` scales the spread: a dimensionless multiple of the slot-derived
# default, like `density`'s `adjust` — `jitter(0.5)` half, `jitter(2)` double,
# bare `jitter` = `jitter(1)`.  It takes a knob (where `dodge` does not) because
# the spread is a free legibility choice with no single right value, unlike
# `dodge`'s width, which the group count determines.  It masks `base::jitter`
# (the numeric-vector jitterer), the way `stack`/`range` mask their base-R
# namesakes — a DSL keeps its own vocabulary.
#' Spread a strip plot's coincident points apart, along the categorical axis only.
#'
#' @param amount Spread as a multiple of the default, one non-negative number;
#'   default 1.  Positional: `jitter(0.5)` for half the spread.
#' @export
jitter <- function(amount = NULL) {
  if (!is.null(amount)) {
    if (!is.numeric(amount) || length(amount) != 1L || is.na(amount) || amount < 0) {
      stop("gog: `jitter(amount = )` needs one non-negative number \u2014 the spread as a ",
           "multiple of the default, e.g. `jitter(0.5)` for half or `jitter(2)` for double.",
           masked_hint(amount, "base::jitter"), call. = FALSE)
    }
    amount <- as.numeric(amount)
  }
  structure(
    list(type = "transform", transform = "jitter", amount = amount),
    class = "gog_atom"
  )
}

# `repel` is the fourth collision modifier, and the one whose collision is made of
# ink.  The other three answer "two marks landed on one position"; a label is as
# wide as the word it draws, so two labels overlap at positions their points never
# shared.  Legal on `text` alone — the only mark whose glyph is a word — and
# refused with direction elsewhere (`point` -> `jitter`, the width marks ->
# `dodge`).
#
# It is the data-derived counterpart to `style(nudge = )`: a nudge shifts every
# label the same way, which clears a label off its own dot and does nothing for a
# crowd overlapping itself.  The two compose, the nudge naming the side a label
# prefers.  Bare, like `dodge` and for the same reason: a label moves as far as the
# overlap requires and no further, so there is no free spread to put a knob on.
# It masks nothing in base R.
#' Move overlapping labels apart until they can be read.
#'
#' `text * repel` reads where the labels and the points actually sit and pushes
#' each label a different way until they stop overlapping, drawing a thin leader
#' line back to the point when a label has moved clear of it. Where there is no
#' arrangement that fits, every label is still drawn and the plot reports how many
#' are still crowded.
#'
#' @return A `gog_atom` joined to a mark with `*`.
#' @seealso [style()]'s `nudge`, the constant offset this is the data-derived
#'   counterpart to; [jitter()], the same idea for a scatter's points.
#' @examples
#' \dontrun{
#' # Every country named, with the names moved off one another.
#' gapminder_2007 <- book_table("gapminder_2007")
#' p <- data(gapminder_2007) + point + text * repel +
#'   x(gdp) + y(life) + label(country)
#' }
#'
#' @export
repel <- structure(list(type = "transform", transform = "repel"), class = "gog_atom")

# ---------------------------------------------------------------------------
# Coordinate atoms — always plot-scoped (shared by all marks)
# ---------------------------------------------------------------------------

# The scales a binding may name.  Kept beside the atoms that accept it rather
# than duplicated into each one.
SCALE_NAMES <- c("linear", "log", "time", "category")

# Validate here rather than in Rust, for the same reason `style()` does: the
# caller gets the error at the line that wrote it, and a misspelling never
# reaches the wire as an enum serde cannot decode.
check_scale <- function(scale) {
  if (is.null(scale)) return(NULL)
  if (!is.character(scale) || length(scale) != 1L) {
    stop("gog: `scale = ` needs a single string, e.g. `x(gdp, scale = \"log\")`.",
         call. = FALSE)
  }
  if (!scale %in% SCALE_NAMES) {
    stop("gog: `scale = \"", scale, "\"` is not a scale. gog has ",
         paste0("\"", SCALE_NAMES, "\"", collapse = ", "), ".", call. = FALSE)
  }
  scale
}

# The base is a plain number, not a name: `base = 2`, not `scale = "log2"`.
# Enumerating "log2"/"log10"/"ln" is the shape this package argues against —
# one parameter derives all of them.  R has no `e` constant, so natural log is
# `base = exp(1)`, and the tick labels switch to e, e², e³ because 2.718 and
# 7.389 are not numbers anyone reads off an axis.
check_base <- function(base) {
  if (is.null(base)) return(NULL)
  if (!is.numeric(base) || length(base) != 1L || !is.finite(base)) {
    stop("gog: `base = ` needs a single number, e.g. `x(bits, scale = \"log\", base = 2)`.",
         call. = FALSE)
  }
  if (base <= 1) {
    stop("gog: `base = ", base, "` is not a base a logarithm can have \u2014 it must be ",
         "greater than 1. Use 10 (the default), 2 for doublings, or `exp(1)` ",
         "for e-foldings.", call. = FALSE)
  }
  as.numeric(base)
}

# The domain the channel runs over, when the data is not the authority (spec
# §10).  Two numbers, either of which may be `NA` on its own to leave that end
# to the data: `limits = c(0, NA)` pins a baseline and lets the top follow.
#
# `NA` crosses the wire as JSON `null` (`na = "null"` in `toJSON`), which is
# exactly the engine's shape for an unstated end — so the R spelling and the
# wire agree without a special case.  Validated here, like `scale` and `base`,
# so the caller gets the error at the line that wrote it; the engine checks it
# again because the other three bindings reach it too.
check_limits <- function(limits) {
  if (is.null(limits)) return(NULL)
  # A domain on a temporal axis is written in dates, not in epoch arithmetic:
  # `limits = c(as.Date("2024-01-01"), as.Date("2024-12-31"))`.  Converted here
  # by the same rule `df_to_wire` converts the *column* by — the engine's one
  # temporal unit is seconds since 1970 — because representation is the
  # binding's job.  Without this the two disagree silently: a `Date` is days, so
  # the limits would arrive 86400x too small and exclude every row.
  if (inherits(limits, "Date")) {
    limits <- as.numeric(limits) * 86400
  } else if (inherits(limits, "POSIXct")) {
    limits <- as.numeric(as.POSIXct(format(limits, "%Y-%m-%d %H:%M:%S"), tz = "UTC"))
  }
  if (!is.numeric(limits) || length(limits) != 2L) {
    stop("gog: `limits = ` needs two numbers, e.g. `x(hour, limits = c(0, 24))`. ",
         "On a date axis use dates: `c(as.Date(\"2024-01-01\"), as.Date(\"2024-12-31\"))`. ",
         "Use `NA` for an end the data should decide: `c(0, NA)`.", call. = FALSE)
  }
  lo <- limits[[1]]; hi <- limits[[2]]
  if (!is.na(lo) && !is.na(hi) && !(lo < hi)) {
    stop("gog: `limits = c(", lo, ", ", hi, ")` runs backwards or has no width \u2014 ",
         "the first number is the low end. Write `c(", min(lo, hi), ", ",
         max(lo, hi), ")`.", call. = FALSE)
  }
  as.numeric(limits)
}

# How many ticks an axis should aim for (spec §10).  A *target*, not a promise:
# the count picks a step and the step is then rounded to a human number, so 8 on a
# 0..100 axis gets a step of 10 and nine ticks.  Two is the floor — one tick shows
# a place but no direction — and the engine says so as well, because a binding is
# not the only way in.
# `free = TRUE` — fit this axis from each panel's own rows (spec §11).
#
# Which axis is freed is not stated: it is whichever channel this was written
# on. A flag rather than a value for the same reason — there is one thing to
# ask for, and the rest of the question was answered by where you asked it.
check_free <- function(free) {
  if (is.null(free) || isFALSE(free)) return(NULL)
  if (!isTRUE(free)) {
    stop("gog: `free = ` is TRUE or FALSE \u2014 it says whether this axis is fitted ",
         "per panel. Which axis is up to which binding you write it on: ",
         "`y(life, free = TRUE)` frees y, `x(gdp, free = TRUE)` frees x.",
         call. = FALSE)
  }
  TRUE
}

check_tick_count <- function(tick_count) {
  if (is.null(tick_count)) return(NULL)
  if (!is.numeric(tick_count) || length(tick_count) != 1L || is.na(tick_count)) {
    stop("gog: `tick_count = ` needs one number, e.g. `x(gdp, tick_count = 8)`. ",
         "It is how many ticks the axis aims for.", call. = FALSE)
  }
  if (tick_count != as.integer(tick_count)) {
    stop("gog: `tick_count = ", tick_count, "` is not a whole number \u2014 an axis ",
         "cannot have a fraction of a tick. Try `tick_count = ",
         round(tick_count), "`.", call. = FALSE)
  }
  if (tick_count < 2) {
    stop("gog: `tick_count = ", tick_count, "` \u2014 an axis needs at least two ticks ",
         "to show a direction as well as a place. Ask for 2 or more, or leave ",
         "`tick_count` off for the default of 5.", call. = FALSE)
  }
  as.integer(tick_count)
}

# How fast a `play` sequence runs, as a multiple of the normal pace (spec §15).
#
# The fourth binding parameter and the narrowest: `limits` needs a domain,
# `tick_count` needs an axis, and this needs a duration — which only `play` has.
# The engine refuses it on any other channel; this only catches the shapes that
# are not a number at all, so `play(year, speed = "fast")` fails where the user
# wrote it rather than as JSON the engine has to interpret.
check_speed <- function(speed) {
  if (is.null(speed)) return(NULL)
  if (!is.numeric(speed) || length(speed) != 1L || !is.finite(speed)) {
    stop("gog: `speed = ` needs a single number, e.g. `play(year, speed = 2)`. ",
         "It is how many times faster than normal the frames run.", call. = FALSE)
  }
  if (speed <= 0) {
    stop("gog: `speed = ", speed, "` \u2014 a speed is a multiple of the normal pace, ",
         "so it has to be above zero. `speed = 2` is twice as fast, ",
         "`speed = 0.5` half.", call. = FALSE)
  }
  as.numeric(speed)
}

#' Bind the x-axis to a column.
#'
#' @param field  Column to bind (bare name).
#' @param scale  How the number becomes a position: \code{"linear"} (the
#'   default), \code{"log"}, \code{"time"}, or \code{"category"}.  A log scale
#'   leaves the data in its own units and labels the ticks 1, 10, 100 — it is
#'   not the same as plotting \code{log(x)}, which would run the axis 0, 1, 2
#'   and leave the reader to exponentiate.
#' @param base   Base of a \code{"log"} scale; 10 unless given.  Use 2 for
#'   doublings, octaves or bits, and \code{exp(1)} for e-foldings.  The base is
#'   very nearly cosmetic — every base draws the same picture, because the axis
#'   is normalized by its own range — so what it really chooses is where the
#'   gridlines fall and how they are labeled.
#'
#' @examples
#' \dontrun{
#' data(gapminder_2007) + point + x(gdp, scale = "log") + y(life)
#' data(audio) + line + x(freq, scale = "log", base = 2) + y(level)
#' }
#' @param limits  The two ends of the axis, as `c(low, high)`. `NA` on either
#'   end leaves that end fitted to the data.
#' @param tick_count  How many ticks the axis should aim for. A target, not a
#'   promise: the chosen values are rounded to readable numbers.
#' @param free  When the plot is faceted, `TRUE` lets each panel fit this axis
#'   to its own rows instead of sharing one scale across all of them.
#' @export
x <- function(field, scale = NULL, base = NULL, limits = NULL, tick_count = NULL,
              free = FALSE) {
  structure(list(type = "coord_x", field = column_name(substitute(field), "x"),
                 scale = check_scale(scale), base = check_base(base),
                 limits = check_limits(limits),
                 tick_count = check_tick_count(tick_count),
                 free = check_free(free)),
            class = "gog_atom")
}

#' Bind the y-axis to a column.
#'
#' @inheritParams x
#' @param limits  The two ends of the axis, as `c(low, high)`. `NA` on either
#'   end leaves that end fitted to the data.
#' @param tick_count  How many ticks the axis should aim for. A target, not a
#'   promise: the chosen values are rounded to readable numbers.
#' @param free  When the plot is faceted, `TRUE` lets each panel fit this axis
#'   to its own rows instead of sharing one scale across all of them.
#' @export
y <- function(field, scale = NULL, base = NULL, limits = NULL, tick_count = NULL,
              free = FALSE) {
  structure(list(type = "coord_y", field = column_name(substitute(field), "y"),
                 scale = check_scale(scale), base = check_base(base),
                 limits = check_limits(limits),
                 tick_count = check_tick_count(tick_count),
                 free = check_free(free)),
            class = "gog_atom")
}

#' Bind the z-axis to a column.
#'
#' @inheritParams x
#' @param limits  The two ends of the axis, as `c(low, high)`. `NA` on either
#'   end leaves that end fitted to the data.
#' @param tick_count  How many ticks the axis should aim for. A target, not a
#'   promise: the chosen values are rounded to readable numbers.
#' @param free  When the plot is faceted, `TRUE` lets each panel fit this axis
#'   to its own rows instead of sharing one scale across all of them.
#' @export
z <- function(field, scale = NULL, base = NULL, limits = NULL, tick_count = NULL,
              free = FALSE) {
  structure(list(type = "coord_z", field = column_name(substitute(field), "z"),
                 scale = check_scale(scale), base = check_base(base),
                 limits = check_limits(limits),
                 tick_count = check_tick_count(tick_count),
                 free = check_free(free)),
            class = "gog_atom")
}

#' View a 3-D plot from an angle.
#'
#' Binding `z` is what makes a plot three-dimensional — one more vowel, not a
#' chart type. `space()` sets the angle that plot is *viewed* from. That angle is
#' a view parameter of the coordinate space — the way a polar plot needs a start
#' angle — not a channel (it maps no column) and not a mark. Omit it and a 3-D
#' plot takes a default three-quarter view that shows all three axes.
#'
#' @param turn Degrees swung around the upright axis — which side you view the
#'   scene from. Default 30.
#' @param tilt Degrees the eye is lifted above the floor — how steeply you look
#'   down. Default 25.
#' @examples
#' \dontrun{
#' data(clusters) + point + x(a) + y(b) + z(c) +
#'   space(turn = -45, tilt = 30)
#' }
#' @export
space <- function(turn = 30, tilt = 25) {
  structure(list(type = "coord_space",
                 turn = degrees(turn, "space", "turn"),
                 tilt = degrees(tilt, "space", "tilt")),
            class = "gog_atom")
}

#' Bend the plane into a circle.
#'
#' The polar coordinate space. `x` becomes the **angle** and `y` the **radius**,
#' so the axis a bar chart stands its categories on is the axis a rose wraps
#' around the circle. Nothing else about the sentence changes: `bar * count +
#' x(month)` is a bar chart flat and a rose in polar, from the same words.
#'
#' The angular axis is *periodic* — one turn spans exactly the fitted range of
#' `x`, so the circle closes with no seam. Angles run clockwise from twelve
#' o'clock.
#'
#' @param start Degrees clockwise from the top where the circle begins. `0`
#'   (the default) starts the axis at twelve o'clock; `-22.5` centers the first
#'   of eight categories there instead of starting it there.
#' @examples
#' \dontrun{
#' data(winds) + bar * count + x(direction) + polar()
#' data(winds) + bar * count + x(direction) + polar(start = -22.5)
#' }
#' @export
polar <- function(start = 0) {
  structure(list(type = "coord_polar", start = degrees(start, "polar", "start")),
            class = "gog_atom")
}

#' Pack the panel with nested regions.
#'
#' The nested coordinate space, and the treemap is what it draws. Every row's
#' measure becomes an **area** rather than a length, and the areas partition the
#' panel: a region's share of the picture is its share of the total.
#'
#' Nothing else about the sentence changes, which is what makes this a space and
#' not a chart type. `bar * sum + y(revenue) + color(region)` is a stacked column
#' flat, a pie in `polar()`, and a treemap here — three answers to the one
#' question of what carries a share, and the words are the same in all three.
#'
#' A packed panel has **no axes**: its two directions carry no variable, and two
#' neighboring regions are not near each other in the data. So there is nothing
#' to label and nothing to tick, and the color legend is what decodes the
#' regions. Bind a position to split the packing one level further — each
#' category gets a region of its own and its rows are packed inside it.
#'
#' `dodge`, `stack` and `jitter` are refused here: they decide what happens when
#' two marks land in the same place, and in a packing none can.
#'
#' @examples
#' \dontrun{
#' data(sales) + bar * sum + y(revenue) + color(region) + nest()
#' data(sales) + bar * sum + x(region) + y(revenue) + color(product) + nest()
#' }
#' @export
nest <- function() {
  structure(list(type = "coord_nest"), class = "gog_atom")
}

#' Flatten the sphere onto the page.
#'
#' The cartographic coordinate space. `x` is **longitude** and `y` is
#' **latitude**, both in degrees, and the projection decides where each place
#' lands. Nothing else about the sentence changes: `point` is a place, `path` is a
#' route through places in the table's order, `text` is a name at a place, and
#' `rule` spans the axis it does not name, which on a map is a meridian or a
#' parallel.
#'
#' Both positions are spent on the place, so a mark that measures along an axis
#' has none left. Carry a quantity on a channel instead. `size(<column>)` gives
#' the proportional-symbol map and `color(<column>)` shades each place, which is
#' what cartography does once the two axes are gone.
#'
#' A sphere cannot be laid flat without giving something up, and area and angle
#' cannot both survive. `preserve` names which one does:
#'
#' * `"area"` -- every region gets ink in proportion to its true size. The
#'   default, because a map is usually read by area: a projection that inflates
#'   Greenland says something false about the number inside it. Uses the Equal
#'   Earth projection, which reaches both poles.
#' * `"angle"` -- every small shape keeps its true form, and area is what pays.
#'   Uses Mercator, so Greenland arrives the size of Africa while being fourteen
#'   times smaller. Mercator sends the poles infinitely far away, so it stops at
#'   85.05 degrees and says how many rows it could not reach.
#'
#' The panel is shaped by the projection rather than by the page, so
#' `theme(ratio = )` is refused here: an equal-area map stretched to fit a box is
#' no longer equal-area, though it still looks like a map.
#'
#' @param preserve What the flattening keeps: `"area"` or `"angle"`.
#' @examples
#' \dontrun{
#' data(quakes_fiji) + point + x(east) + y(north) + size(magnitude) + map()
#' data(borders) + path + x(lon) + y(lat) + group(country) + map()
#' data(borders) + path + x(lon) + y(lat) + group(country) +
#'   map(preserve = "angle")
#' }
#' @export
map <- function(preserve = "area") {
  # Validated at the line the caller wrote, rather than at the wire. The engine
  # checks it too — a rule implemented in one binding is a rule the other three
  # get wrong — but a reader is owed the error where they typed it.
  if (!is.character(preserve) || length(preserve) != 1L ||
      !preserve %in% c("area", "angle")) {
    stop("gog: `map(preserve = )` takes \"area\" or \"angle\". ",
         "\"area\" keeps every region's true size, which is what a map read by ",
         "area needs; \"angle\" keeps every small shape's true form and pays for ",
         "it in area. A sphere cannot do both.", call. = FALSE)
  }
  structure(list(type = "coord_map", preserve = preserve), class = "gog_atom")
}

# ---------------------------------------------------------------------------
# Layer-scoped channel atoms — bind to the nearest preceding mark
# ---------------------------------------------------------------------------

#' Map fill/stroke color to a column.
#'
#' @param limits  For a continuous column, the two ends of the color scale,
#'   as `c(low, high)`. `NA` on either end leaves that end fitted to the data.
#' @param field  Column to bind (bare name).  Text picks colors from the
#'   palette; a number runs along a sequential ramp.
#' @inheritParams x
#' @export
color <- function(field, scale = NULL, base = NULL, limits = NULL) {
  structure(list(type = "color", field = column_name(substitute(field), "color", settable = TRUE),
                 scale = check_scale(scale), base = check_base(base),
                 limits = check_limits(limits)),
            class = "gog_atom")
}

#' The British spelling of \code{color()}, refused with direction.
#'
#' gog writes American English throughout and accepts no second spelling, which
#' is Law 2 applied to the vocabulary itself: two ways to write one word is a
#' silent letter, and the reader pays for it.  ggplot2 accepts \code{colour} and
#' \code{color} both, so a reader arriving from there types \code{colour} and,
#' unexported, would meet R's "could not find function" — a message that names
#' no fix.  Exported for the same reason JavaScript still exports \code{facet}
#' (spec §13): a word a reader will plausibly type earns a refusal that says
#' what to write instead.
#'
#' @param ... Ignored.  The call is always refused.
#' @export
colour <- function(...) {
  stop("gog: there is no `colour()` channel. gog spells it `color(<column>)`: ",
       "American English is the grammar's only spelling, and unlike ggplot2 ",
       "there is no British alternative.", call. = FALSE)
}

#' Group line/path marks by a column (connects points within the same group).
#' Use color(field) when you also want color distinction — it implies grouping.
#' @param field  Column to group by, as a bare name.
#' @return A `gog_atom` added to a plot with `+`.
#' @export
group <- function(field) {
  structure(list(type = "group", field = column_name(substitute(field), "group")),
            class = "gog_atom")
}

#' Map point radius to a numeric column.
#'
#' @inheritParams x
#' @param limits  The two ends of the size scale, as `c(low, high)`. `NA` on
#'   either end leaves that end fitted to the data.
#' @export
size <- function(field, scale = NULL, base = NULL, limits = NULL) {
  structure(list(type = "size", field = column_name(substitute(field), "size", settable = TRUE),
                 scale = check_scale(scale), base = check_base(base),
                 limits = check_limits(limits)),
            class = "gog_atom")
}

#' Map glyph shape to a categorical column.
#'
#' No \code{scale} argument: \code{shape} answers "which one?", and there is no
#' distance between circle and square for a scale to run along.
#' @param field  Column to bind to the glyph shape, as a bare name.
#' @return A `gog_atom` added to a plot with `+`.
#' @export
shape <- function(field) {
  structure(list(type = "shape", field = column_name(substitute(field), "shape", settable = TRUE)),
            class = "gog_atom")
}

#' Map paint texture to a categorical column --- \code{shape}'s twin.
#'
#' \code{pattern} tells categories apart by \emph{texture} rather than hue: on a
#' fill (\code{bar}/\code{box}/\code{area}/\code{ribbon}) each category draws as a
#' hatch, on a stroke (\code{line}/\code{step}/\code{interval}) as a dash.  It is
#' the color-free way to separate series, so it survives grayscale printing and
#' color-blindness; pairing it with \code{color} on the same column is the
#' redundant encoding that is the accessibility best practice.  No \code{scale}
#' argument: like \code{shape} it answers "which one?", not "how much?".
#'
#' Distinct from the \code{style(pattern = )} \emph{setting}, which fixes one
#' texture for the whole layer; \code{pattern()} \emph{maps} a column to several.
#' @param field  Column to bind to the texture, as a bare name.
#' @return A `gog_atom` added to a plot with `+`.
#' @export
pattern <- function(field) {
  structure(list(type = "pattern", field = column_name(substitute(field), "pattern", settable = TRUE)),
            class = "gog_atom")
}

#' Map opacity to a numeric column.
#'
#' @inheritParams x
#' @param limits  The two ends of the scale, as `c(low, high)`. `NA` on either
#'   end leaves that end fitted to the data.
#' @param tick_count  How many ticks the legend should aim for. A target, not
#'   a promise: the chosen values are rounded to readable numbers.
#' @export
opacity <- function(field, scale = NULL, base = NULL, limits = NULL, tick_count = NULL) {
  structure(list(type = "opacity", field = column_name(substitute(field), "opacity", settable = TRUE),
                 scale = check_scale(scale), base = check_base(base),
                 limits = check_limits(limits)),
            class = "gog_atom")
}

#' Draw a column's values as text labels — the \code{text} mark's content.
#'
#' \code{label} supplies the string a \code{text} mark draws, the way \code{x}
#' and \code{y} supply its position: \code{text + x(a) + y(b) + label(name)}.  A
#' string column is drawn as-is, a numeric one is formatted.  It is \code{text}'s
#' required channel — no other mark accepts it, so a labeled scatter is the
#' superposition \code{point + text}.  No \code{scale}: a label is content, like
#' \code{shape}, not a magnitude to run a scale along.
#' @param field  Column whose values are drawn as the text, as a bare name.
#' @return A `gog_atom` added to a plot with `+`.
#' @export
label <- function(field) {
  structure(list(type = "label", field = column_name(substitute(field), "label")),
            class = "gog_atom")
}

#' Cut the plot into frames and play them — the time dimension.
#'
#' \code{play} is \code{facet} read in time.  Both split the rows by a column's
#' distinct values; \code{| facet(continent)} lays the pieces out across the page
#' and \code{play(year)} lays them out in sequence, so
#' \code{point + x(gdp) + y(life) + play(year)} is the time-lapse of the same
#' data the faceted grid shows all at once.
#'
#' Every scale, the color map and every legend are fitted across the whole
#' sequence, never per frame — a scale that re-fitted would move the axis under
#' the data and make a still point look like a moving one.  A layer that does not
#' bind \code{play} is drawn in every frame, so a reference line stands still
#' behind the marks that move.
#'
#' Unlike \code{facet}, a \strong{number} is welcome: panels compete for page
#' area, where a hundred of them are unreadable at any size, but frames compete
#' for time, where a hundred is a longer loop rather than a smaller picture.
#'
#' \code{speed} is how many times faster than normal the frames run --- 2 is
#' twice as fast, 0.5 half.  Where a static image is made from the plot (the PDF
#' of this book, or any SVG converter) the first frame is what it shows.
#' @param field  Column whose distinct values become the frames, as a bare name.
#' @param speed  A multiple of the normal pace: `2` is twice as fast.
#' @return A `gog_atom` added to a plot with `+`.
#' @export
play <- function(field, speed = NULL) {
  structure(list(type = "play", field = column_name(substitute(field), "play"),
                 speed = check_speed(speed)),
            class = "gog_atom")
}

# What `at = ` was given, and which of the two readings it is.
#
# One argument rather than two, because the *value* answers the question the way
# a column answers it everywhere else in this grammar: numbers are a range,
# names are a set of slots. `at = c("Asia")` beside `at = c(1200, 45000)` would
# be two argument names for one idea, which is the silent letter Law 2 refuses.
check_brush_at <- function(at) {
  if (is.null(at)) return(NULL)
  if (is.character(at) || is.factor(at)) {
    at <- as.character(at)
    if (length(at) < 1L || anyNA(at)) {
      stop("gog: `at = ` on a column of categories is the names to select, ",
           "e.g. `brush(continent, at = c(\"Asia\", \"Europe\"))`.", call. = FALSE)
    }
    return(list(levels = at))
  }
  if (!is.numeric(at) || length(at) != 2L || !all(is.finite(at))) {
    stop("gog: `at = ` is where the selection opens: two numbers on a column ",
         "that measures, e.g. `brush(gdp, at = c(1200, 45000))`, or the names ",
         "to select on a column of categories.", call. = FALSE)
  }
  list(at = as.numeric(at))
}

#' Let the reader select rows, and push back the rest.
#'
#' \code{brush} puts a bound on one column's values.  Rows inside it keep the
#' plot's colors; rows outside it are dimmed, so a selection is read against
#' what it was taken from.  Where the page can run the engine, dragging moves the
#' bound; on paper it stays where the sentence put it.
#'
#' \strong{A brush highlights.  It never removes rows.}  Removing rows before
#' the statistics run is what \code{limits} does, on the binding, and it counts
#' what it dropped.  The two are the same shape and different operations: change
#' a domain and a histogram re-bins the survivors, brush it and the same bars
#' stay, with the selected part standing out.
#'
#' One column per \code{brush}.  Write two for a rectangle:
#' \code{brush(gdp, at = c(1200, 45000)) + brush(life, at = c(55, 78))}.
#'
#' A mark can be brushed when one row is one shape: \code{point}, \code{text},
#' \code{rule} and \code{zone}.  A \code{line} draws one shape through many rows,
#' so there is no single row to select, and gog says so rather than guessing.
#' A summarized layer is drawn whole, because a selection of twelve of a bar's
#' forty rows has no honest picture.
#' @param field  Column the bound is read on, as a bare name.
#' @param at  Where the selection opens: two numbers on a column that measures,
#'   or the names to select on a column of categories.  Left out, nothing is
#'   selected and the plot draws exactly as it would with no `brush` at all.
#' @return A `gog_atom` added to a plot with `+`.
#' @export
brush <- function(field, at = NULL) {
  # Bare `brush` — `+` calls an uncalled atom with its defaults, the same way
  # `bar * bin` calls `bin`. No column means the positions this plot binds, so
  # the reader draws a region instead of moving a bound the author chose.
  field <- if (missing(field)) "" else deparse(substitute(field))
  structure(c(list(type = "brush", field = field), check_brush_at(at)),
            class = "gog_atom")
}

# ---------------------------------------------------------------------------
# Constant settings — set, not mapped
# ---------------------------------------------------------------------------

# The British spelling of a setting, and what gog spells it instead.  One entry
# per gog word that has a British form; there are three, and `colour()` the
# channel is the fourth word in the grammar with one.
BRITISH_SETTINGS <- c(colour = "color", border_colour = "border_color",
                      centre = "center")

# Refuse a name `style()` does not have.  Split out so the British spelling and
# the ordinary typo part on the *message* and not on the check: one names the
# word to write, the other lists what exists.
reject_setting <- function(name) {
  if (!nzchar(name)) {
    stop("gog: `style()` takes named settings, e.g. `style(color = \"tomato\")`.",
         call. = FALSE)
  }
  if (name %in% names(BRITISH_SETTINGS)) {
    stop("gog: `style(", name, " = )` is not a setting. gog spells it `",
         BRITISH_SETTINGS[[name]], "`: American English is the grammar's only ",
         "spelling, and unlike ggplot2 there is no British alternative.",
         call. = FALSE)
  }
  stop("gog: `style(", name, " = )` is not a setting. gog sets: ",
       paste(sort(setdiff(names(formals(style)), "...")), collapse = ", "), ".",
       call. = FALSE)
}

#' Set constant visual properties on a layer.
#'
#' Channels \emph{map}: \code{color(species)} asks the reader "which species?"
#' and earns a legend to answer it.  \code{style()} \emph{sets}: it fixes a
#' visual property at one value for the whole layer, maps no column, consumes
#' no scale, and produces no legend — there is nothing to decode.
#'
#' A constant is therefore not a channel, by the same reasoning that keeps
#' rotation out of the channel set: a channel maps a data column to a visual
#' feature, and a constant maps nothing.
#'
#' @param color   CSS color name (\code{"steelblue"}) or hex (\code{"#4e79a7"}).
#' @param opacity Number in 0–1.  Set literally — unlike the \code{opacity}
#'   channel there is no data range to rescale from.
#' @param size    Pixels: point radius (default 4.5), or line stroke width
#'   (default 2).
#' @param shape   One of \code{"circle"}, \code{"square"}, \code{"triangle"},
#'   \code{"diamond"}, \code{"cross"}.
#'
#' @examples
#' \dontrun{
#' data(df) + x(a) + y(b) + point + style(color = "tomato", opacity = 0.3)
#' data(df) + x(a) + y(b) + line + style(color = "gray70", size = 1)
#' }
#' @param border_color  Outline color for a closed glyph.
#' @param border_size   Outline width for a closed glyph.
#' @param caps    `TRUE` (default) draws the end caps on an interval; `FALSE`
#'   leaves it bare.
#' @param center  `TRUE` (default) draws a box plot's median line.
#' @param nudge   Offset a text label from its point, in pixels.
#' @param pattern A stroke's dash pattern, or a fill's hatch tile.
#' @param arrow   Draw the stroke as an arrow.
#' @param reach   How far a density curve runs past the data.
#' @param ...     Further settings; an unknown one is refused by name rather
#'   than ignored.
#' @export
style <- function(color = NULL, opacity = NULL, size = NULL, shape = NULL,
                  border_color = NULL, border_size = NULL, caps = NULL,
                  center = NULL, nudge = NULL, pattern = NULL, arrow = NULL,
                  reach = NULL, ...) {
  # `...` exists only to be refused.  Without it R answers an unknown setting
  # with "unused argument (colour = ...)", which names neither the fix nor even
  # the package — the one message in the four bindings that taught nothing.
  dots <- list(...)
  if (length(dots)) {
    nms <- names(dots)
    if (is.null(nms)) nms <- rep("", length(dots))
    reject_setting(nms[[1L]])
  }

  props <- list(color = color, opacity = opacity, size = size, shape = shape,
                border_color = border_color, border_size = border_size, caps = caps,
                center = center, nudge = nudge, pattern = pattern, arrow = arrow,
                reach = reach)
  props <- props[!vapply(props, is.null, logical(1))]

  if (length(props) == 0L) {
    stop("gog: `style()` sets nothing. Name at least one property, ",
         "e.g. `style(color = \"tomato\")`.", call. = FALSE)
  }

  # Type-check here rather than in Rust: the R caller gets the error at the
  # line that wrote it, and a length-2 vector never reaches the wire as an
  # array where a scalar is expected. `border_color` is the outline color of a
  # filled mark (the fill is `color`); `border_size` its width.
  string_eg <- c(color = "\"tomato\"", shape = "\"square\"", border_color = "\"black\"")
  for (nm in c("color", "shape", "border_color")) {
    v <- props[[nm]]
    if (!is.null(v) && (!is.character(v) || length(v) != 1L)) {
      stop("gog: `style(", nm, " = )` needs a single string, e.g. ",
           "`style(", nm, " = ", string_eg[[nm]], ")`.", call. = FALSE)
    }
  }
  number_eg <- c(opacity = "0.3", size = "6", border_size = "1.5")
  for (nm in c("opacity", "size", "border_size")) {
    v <- props[[nm]]
    if (!is.null(v) && (!is.numeric(v) || length(v) != 1L)) {
      stop("gog: `style(", nm, " = )` needs a single number, e.g. ",
           "`style(", nm, " = ", number_eg[[nm]], ")`.", call. = FALSE)
    }
    if (!is.null(v)) props[[nm]] <- as.numeric(v)
  }

  # `caps` is a flag, not a color or a size: TRUE draws an interval's end caps
  # (an error bar), FALSE a bare linerange.
  if (!is.null(props$caps) && (!is.logical(props$caps) || length(props$caps) != 1L ||
                               is.na(props$caps))) {
    stop("gog: `style(caps = )` needs TRUE or FALSE \u2014 `caps = FALSE` draws a bare ",
         "linerange, `caps = TRUE` (the default) an error bar.", call. = FALSE)
  }

  # `center` is a flag too: TRUE draws a confidence interval's center dot (a
  # pointrange), FALSE hides it (a bare error bar).
  if (!is.null(props$center) && (!is.logical(props$center) || length(props$center) != 1L ||
                                 is.na(props$center))) {
    stop("gog: `style(center = )` needs TRUE or FALSE \u2014 `center = FALSE` hides a ",
         "confidence interval's center dot, `center = TRUE` (the default) draws it.", call. = FALSE)
  }

  # `nudge` moves a text label off its point so a superposed dot shows through:
  # one of four plain directions.
  if (!is.null(props$nudge) &&
      (!is.character(props$nudge) || length(props$nudge) != 1L ||
       !props$nudge %in% c("up", "down", "left", "right"))) {
    stop("gog: `style(nudge = )` needs one of \"up\", \"down\", \"left\", \"right\" ",
         "\u2014 which way a text label sits from its point.", call. = FALSE)
  }

  # `pattern` is the texture of a mark's paint, realized per geometry (spec §4):
  # on a stroke (`line`/`step`/`interval`) the dash (`dashed`/`dotted`), on a fill
  # (`bar`/`box`/`area`/`ribbon`) a hatch (`hatch`/`crosshatch`/`grid`/`dots`);
  # `solid` is the shared no-texture default. Here we only check the value is one of
  # the union; the engine, which knows the mark, refuses a dash on a fill (or a
  # hatch on a stroke) with direction.
  pattern_values <- c("solid", "dashed", "dotted", "hatch", "crosshatch", "grid", "dots")
  if (!is.null(props$pattern) &&
      (!is.character(props$pattern) || length(props$pattern) != 1L ||
       !props$pattern %in% pattern_values)) {
    stop("gog: `style(pattern = )` needs a stroke's dash ",
         "(\"solid\", \"dashed\", \"dotted\") or a fill's texture ",
         "(\"hatch\", \"crosshatch\", \"grid\", \"dots\").", call. = FALSE)
  }

  # `arrow` puts a head on a `path`'s end -- the one mark with a direction to
  # point in, since a `line` sorts its vertices and its last point is wherever
  # the domain ends.  A value rather than a flag, so a double-headed arrow says
  # so in one word.
  if (!is.null(props$arrow) &&
      (!is.character(props$arrow) || length(props$arrow) != 1L ||
       !props$arrow %in% c("end", "start", "both"))) {
    stop("gog: `style(arrow = )` needs \"end\", \"start\", or \"both\" ",
         "\u2014 which end of a `path` carries the head.", call. = FALSE)
  }

  # `reach` is how far a `rule` crosses the axis it does not name: all the way
  # (a reference line) or a short tick at that axis's start (a rug).  There is no
  # distance argument -- the tick length comes from the panel, the way a nudge's
  # comes from the font size.
  if (!is.null(props$reach) &&
      (!is.character(props$reach) || length(props$reach) != 1L ||
       !props$reach %in% c("panel", "edge"))) {
    stop("gog: `style(reach = )` needs \"panel\" (the default \u2014 a `rule` all the ",
         "way across, a reference line) or \"edge\" (a short tick at the start of ",
         "that axis, a rug).", call. = FALSE)
  }

  structure(list(type = "style", props = props), class = "gog_atom")
}

# ---------------------------------------------------------------------------
# Plot-level annotation atoms
# ---------------------------------------------------------------------------

#' Order the categorical axis by a column.
#'
#' Every atom that takes a column is a noun naming a property, and its argument
#' is the column that drives it — \code{color(species)}, \code{size(population)},
#' \code{order(gold)}.  This one was called \code{sort_by} until it became the
#' last survivor of a \code{_by} convention that \code{color_by} → \code{color}
#' had already retired: the suffix marked the argument as a key, which says
#' nothing when every atom in the group takes a key.
#'
#' Masks \code{base::order}, as \code{data}, \code{title}, and \code{palette}
#' mask theirs.  Use \code{base::order()} for the base function.
#'
#' @param field  Column to order by (bare name). Use a numeric column (e.g. y)
#'   to order by value, which overrides a factor's levels. Use the category
#'   column itself to ask for that column's own order: alphabetical for plain
#'   text, and the declared levels for a factor.
#' @param desc   Logical. \code{TRUE} = descending (largest first / Z→A).
#'
#' @examples
#' \dontrun{
#' data(medals) + bar + x(gold) + y(country) + order(gold, desc = TRUE)
#' }
#' @export
order <- function(field, desc = FALSE) {
  # `order` is one of the eight gog names that mask a base *function* rather than
  # a base object, and it is the one measured to fail worst. A reader doing
  # ordinary host arithmetic beside a gog sentence writes `df[order(key), ]`, gets
  # this atom, and dies at `invalid subscript type 'list'` inside `[.data.frame` —
  # a message naming neither `order` nor gog, several frames from the line that
  # wrote it. This atom names a *column*, so an operand that is not a bare name is
  # that mistake, refused here where the fix can still be said. `order(desc =
  # TRUE)` keeps working: the field is optional, and that spelling is how a
  # categorical axis is reversed.
  if (!missing(field) && !is.name(substitute(field))) {
    expr <- substitute(field)
    bad  <- paste(deparse(expr), collapse = " ")
    minus <- is.call(expr) && identical(expr[[1L]], as.name("-"))
    stop("gog: `order(", bad, ")` is not a column name. gog's `order()` takes a ",
         "bare column, as in `order(population, desc = TRUE)`. To sort a vector, ",
         "the base function is still there as `base::order(", bad, ")`.",
         if (minus) paste0(" For descending order write `desc = TRUE`; a minus ",
                           "sign reverses a vector, not an axis.") else "",
         call. = FALSE)
  }
  structure(
    list(type        = "order",
         order_field = column_name(substitute(field), "order"),
         descending  = isTRUE(desc)),
    class = "gog_atom"
  )
}

#' Name the column that splits the plot into panels — small multiples.
#'
#' \code{facet()} joins the plot with the facet operators, not \code{+}:
#'
#' \preformatted{
#' plot | facet(cyl)               one panel per category, side by side
#' plot / facet(drv)               one panel per category, stacked
#' plot / facet(drv) | facet(cyl)  crossed into a grid (rows x columns)
#' }
#'
#' The panels share one scale — that is what makes them comparable — and any
#' transform in the plot runs within each panel's rows. The facet column must
#' be a category column: a number is a position along an axis, not a name for
#' a panel.
#'
#' \code{wrap} folds a long line of panels into a rectangle: twelve countries
#' side by side are twelve slivers, and \code{wrap = 4} makes them a 4 x 3 grid.
#' The number is \strong{how many panels before the line turns}, and which way
#' the line runs is the operator's to say, not \code{wrap}'s:
#'
#' \preformatted{
#' plot | facet(country, wrap = 4)  levels run across, 4 to a row
#' plot / facet(country, wrap = 4)  levels run down, 4 to a column
#' }
#'
#' That is one number where ggplot2 has \code{nrow} and \code{ncol}, and it
#' cannot contradict itself.  Wrapping a *crossed* facet is refused: a crossing
#' already fills a rectangle.
#'
#' @param field  Column to facet by (bare name).
#' @param wrap   Panels to draw before the line of them turns. \code{NULL}
#'   (the default) leaves the panels in one line.
#' @export
facet <- function(field, wrap = NULL) {
  if (!is.null(wrap) &&
      !(is.numeric(wrap) && length(wrap) == 1 && !is.na(wrap) &&
        wrap == as.integer(wrap))) {
    stop("gog: `facet(wrap = )` takes the number of panels to draw before the ",
         "line of them turns \u2014 one whole number, e.g. `wrap = 4`.", call. = FALSE)
  }
  structure(list(type = "facet", field = column_name(substitute(field), "facet"),
                 wrap = if (is.null(wrap)) NULL else as.integer(wrap)),
            class = "gog_atom")
}

#' Set the categorical color palette.
#'
#' @param pal Either a palette name ("gog", "okabe") or a character vector
#'   of hex colors. The palette is used in first-appearance order; if there
#'   are more categories than palette entries, additional colors are generated
#'   automatically via the HSL color wheel so no two categories share a color.
#' @export
palette <- function(pal) {
  # A no-argument call is `grDevices::palette()`, which reads the current base
  # palette back. gog's always names one, so the bare call can only be the masked
  # function, and R's own "argument is missing" says nothing about either.
  if (missing(pal)) {
    stop("gog: `palette()` names the palette to use, as in `palette(\"soft\")` or ",
         "`palette(c(\"#1b9e77\", \"#d95f02\"))`. To read base R's own palette, ",
         "that is `grDevices::palette()`, the function this atom's name masks.",
         call. = FALSE)
  }
  value <- if (is.character(pal) && length(pal) == 1) {
    list(named = pal)
  } else {
    list(custom = as.list(as.character(pal)))
  }
  structure(list(type = "palette", value = value), class = "gog_atom")
}

#' Set the plot's furniture — the page rather than the ink.
#'
#' Everything here maps no column, so each is a *setting*; but none of it
#' belongs to a mark either, which is why it is not `style()`. A layer has no
#' gridlines and a plot has no fill, so the two property sets are disjoint and
#' telling them apart by where they were written would make a sub-expression
#' mean different things in different places (Law 6). Spec §7 is the ruling.
#'
#' A named preset comes first and anything named adjusts it, because a preset
#' you cannot adjust sends you straight back to asking for knobs.
#'
#' @param preset One of `"gog"` (the default look), `"minimal"` (no gridlines) or
#'   `"bw"` (a white panel inside a full rectangle — the journal look, and note
#'   that what goes black and white is the furniture, never the data).
#' @param grid Which gridlines are drawn: `"both"`, `"x"`, `"y"` or `"none"`,
#'   named by the *axis* whose ticks they mark — which is what lets the setting
#'   survive `polar()`, where the y axis's gridlines are rings.
#' @param ratio The panel's width divided by its height. `1` is a square.
#' @param tick_angle Degrees to turn the x tick labels through, between -90 and
#'   90. `45` is the usual answer to category names that overlap.
#' @param font_size How many pixels a tick label is — and, through it, the size
#'   of every other piece of text the plot draws. One number rather than three,
#'   because the axis names and the title are a fixed step above it: `11` (the
#'   default) gives 11, 13 and 16, and `16` gives 16, 19 and 23. It is a
#'   measurement, not a multiplier, so `font_size = 1.5` is refused. It names no
#'   typeface — the engine measures text with its own width table and has none to
#'   choose.
#' @param background The panel's fill — any color the rest of the grammar
#'   accepts, including `"transparent"`.
#' @param strip The facet strip's fill: the band above a panel that names the
#'   level it holds. Same colors as `background`. `theme("bw")` sets it white,
#'   because a gray band reproduces poorly in print, which is the one place that
#'   preset is for.
#' @param strip_text The ink of the strip's label. Leave it out and gog picks
#'   whichever of its two defaults reads on the band, so
#'   `theme(strip = "black")` already gives you white type. Name it when the ink
#'   is a real choice rather than a legibility one, such as a navy strip with
#'   gold type.
#' @param frame How the panel is bounded: `"full"` (a rectangle, which is what
#'   `theme("bw")` sets and what a journal usually asks for), `"axes"` (the
#'   default, bottom and left only) or `"none"`.
#' @param width,height How many pixels the plot asks for. On its own that is the
#'   image; composed onto a page with `|` or `/` it is the plot's *cell*, and
#'   the plots that ask for nothing split what is left — which is how a marginal
#'   histogram says it is thin. One meaning in both places (Law 6). Not to be
#'   confused with `ratio`, which shapes the *panel* inside whatever room the
#'   plot was given and never resizes the image.
#' @export
theme <- function(preset = NULL, grid = NULL, ratio = NULL, tick_angle = NULL,
                  font_size = NULL, background = NULL, strip = NULL,
                  strip_text = NULL, frame = NULL, width = NULL, height = NULL) {
  if (!is.null(preset) && !(is.character(preset) && length(preset) == 1)) {
    stop("gog: `theme()` takes a preset name first \u2014 `theme(\"minimal\")` \u2014 and ",
         "everything else by name: `theme(grid = \"none\")`.", call. = FALSE)
  }
  if (is.null(preset) && is.null(grid) && is.null(ratio) && is.null(tick_angle) &&
      is.null(font_size) && is.null(background) && is.null(strip) &&
      is.null(strip_text) && is.null(frame) && is.null(width) && is.null(height)) {
    stop("gog: `theme()` sets nothing. Name a preset or a property, e.g. ",
         "`theme(\"minimal\")` or `theme(grid = \"none\", ratio = 1)`.", call. = FALSE)
  }
  # The values are checked in the engine too (`check_theme`), which is what makes
  # the rule the grammar's rather than this binding's. Checking here as well is
  # what puts the error on the line that wrote it.
  if (!is.null(grid) && !(is.character(grid) && length(grid) == 1 &&
                          grid %in% c("both", "x", "y", "none"))) {
    stop("gog: `theme(grid = )` is one of \"both\", \"x\", \"y\" or \"none\".",
         call. = FALSE)
  }
  if (!is.null(ratio) && !(is.numeric(ratio) && length(ratio) == 1 &&
                           is.finite(ratio) && ratio > 0)) {
    stop("gog: `theme(ratio = )` is the panel's width divided by its height, so it ",
         "needs one positive number. `ratio = 1` is a square.", call. = FALSE)
  }
  if (!is.null(tick_angle) && !(is.numeric(tick_angle) && length(tick_angle) == 1 &&
                                is.finite(tick_angle) && abs(tick_angle) <= 90)) {
    stop("gog: `theme(tick_angle = )` turns the x tick labels between -90 and 90 ",
         "degrees. `tick_angle = 45` is the usual answer to names that overlap.",
         call. = FALSE)
  }
  if (!is.null(font_size) && !(is.numeric(font_size) && length(font_size) == 1 &&
                               is.finite(font_size) && font_size >= 4)) {
    stop("gog: `theme(font_size = )` is how many pixels a tick label is, not a ",
         "multiplier, so it needs one number of at least 4. The default is 11, ",
         "and the axis names and the title are derived from it.", call. = FALSE)
  }
  if (!is.null(frame) && !(is.character(frame) && length(frame) == 1 &&
                           frame %in% c("full", "axes", "none"))) {
    stop("gog: `theme(frame = )` is one of \"full\" (a rectangle round the panel), ",
         "\"axes\" (bottom and left only) or \"none\".", call. = FALSE)
  }
  if (!is.null(background) && !(is.character(background) && length(background) == 1)) {
    stop("gog: `theme(background = )` needs a single color, e.g. ",
         "`theme(background = \"white\")` or `\"transparent\"`.", call. = FALSE)
  }
  if (!is.null(strip) && !(is.character(strip) && length(strip) == 1)) {
    stop("gog: `theme(strip = )` needs a single color for the band above each ",
         "panel, e.g. `theme(strip = \"white\")`.", call. = FALSE)
  }
  if (!is.null(strip_text) && !(is.character(strip_text) && length(strip_text) == 1)) {
    stop("gog: `theme(strip_text = )` needs a single color for the strip's label. ",
         "Leave it out and gog picks the one that reads on the band.", call. = FALSE)
  }
  # One loop for both, because they are one property asked twice — see the
  # engine's `check_theme`, which states the same rule for every binding.
  for (side in c("width", "height")) {
    v <- get(side)
    if (!is.null(v) && !(is.numeric(v) && length(v) == 1 && is.finite(v) && v >= 40)) {
      stop("gog: `theme(", side, " = )` is how many pixels the plot asks for, so it ",
           "needs one number of at least 40. On its own it sizes the image; ",
           "composed with `|` or `/` it sizes the plot's cell on the page.",
           call. = FALSE)
    }
  }
  structure(list(type = "theme", preset = preset, grid = grid,
                 ratio = ratio, tick_angle = tick_angle, font_size = font_size,
                 background = background, strip = strip,
                 strip_text = strip_text, frame = frame,
                 width = width, height = height),
            class = "gog_atom")
}

#' Set the plot title.
#' @param text  The label, as a character string.
#' @export
title <- function(text) {
  # Only the bare call is catchable here, and it is worth catching because R's own
  # "argument is missing" names neither `title()`. Two other spellings are not:
  # `title(main = "...")` is rejected by argument matching before this body runs
  # (R says `unused argument`, which at least names the argument), and
  # `title("...")` is well formed in both readings, so nothing can tell the plot's
  # title from a line written onto a base R plot. Catching those would mean a
  # `...` in the signature, which would swallow every mistyped argument name to
  # buy one message, so they are documented instead. `box()` is the same ruling.
  if (missing(text)) {
    stop("gog: `title()` takes the title as one string, as in `title(\"Life ",
         "expectancy\")`. To write on a base R plot, that is ",
         "`graphics::title(main = )`, the function this atom's name masks.",
         call. = FALSE)
  }
  structure(list(type = "title", value = text_value(text, "title")),
            class = "gog_atom")
}

#' Override the x-axis label.
#' @param text  The label, as a character string.
#' @export
x_label <- function(text) {
  structure(list(type = "x_label", value = text_value(text, "x_label")),
            class = "gog_atom")
}

#' Override the y-axis label.
#' @param text  The label, as a character string.
#' @export
y_label <- function(text) {
  structure(list(type = "y_label", value = text_value(text, "y_label")),
            class = "gog_atom")
}

#' Override the z-axis label.
#'
#' The third position takes a label override for the same reason it takes a
#' scale and a tick: the positions are a family of three, and two of them
#' having one is the per-channel exception the orthogonality law catches.
#' Read only in 3-D, where the axis names sit on the projected cube's edges.
#' @param text  The label, as a character string.
#' @export
z_label <- function(text) {
  structure(list(type = "z_label", value = text_value(text, "z_label")),
            class = "gog_atom")
}
