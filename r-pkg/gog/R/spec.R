# spec.R — gog_spec construction and the + operator
#
# A gog_spec holds the in-progress plot specification plus the actual data
# frames.  The + operator accumulates atoms left to right.

# ---------------------------------------------------------------------------
# * operator — derive (bind a mark to a transform, creating a combined atom)
#
# mark * transform  →  a pre-built layer atom carrying both
# layer * transform →  adds another transform to an existing layer atom
#
# R evaluates * before +, so in:
#   data(df) + bar * bin + x(height) + y(count)
# the sub-expression  bar * bin  resolves first → a "layer" gog_atom, which
# is then composed into the spec by +.gog_spec.
# ---------------------------------------------------------------------------

# A bin transform carries its count/width/tiling onto the layer — the engine reads
# them from `layer$bin`, not from inside the transform list. Only non-NULL params
# are attached, so a bare `bar * bin` adds nothing and stays on Sturges' rule with
# a rectangular mesh.
carry_bin_params <- function(layer, tr) {
  if (identical(tr$transform, "bin") &&
      (!is.null(tr$bins) || !is.null(tr$width) || !is.null(tr$tiling))) {
    layer$bin <- Filter(Negate(is.null),
                        list(bins = tr$bins, width = tr$width, tiling = tr$tiling))
  }
  layer
}

# A density transform carries its adjust/bandwidth/levels/compare onto the layer —
# the engine reads them from `layer$density`, mirroring `carry_bin_params`. Only
# non-NULL params attach, so a bare `line * density` adds nothing and stays on
# Silverman's automatic bandwidth, a bare `path * density` on the default number of
# contours, and a bare violin on comparing shapes.
carry_density_params <- function(layer, tr) {
  if (identical(tr$transform, "density") &&
      (!is.null(tr$adjust) || !is.null(tr$bandwidth) || !is.null(tr$levels) ||
       !is.null(tr$compare) || !is.null(tr$reach))) {
    layer$density <- Filter(Negate(is.null),
                            list(adjust = tr$adjust, bandwidth = tr$bandwidth,
                                 levels = tr$levels, compare = tr$compare,
                                 reach = tr$reach))
  }
  layer
}

# A confidence transform carries its level onto the layer — the engine reads it
# from `layer$confidence`. Only a non-NULL level attaches, so a bare
# `interval * confidence` stays on the default 0.95.
carry_confidence_params <- function(layer, tr) {
  if (identical(tr$transform, "confidence") && !is.null(tr$level)) {
    layer$confidence <- list(level = tr$level)
  }
  layer
}

# A jitter modifier carries its amount onto the layer — the engine reads it from
# `layer$jitter`, mirroring `carry_density_params`. Only a non-NULL amount
# attaches, so a bare `point * jitter` stays on the slot-derived default spread.
carry_jitter_params <- function(layer, tr) {
  if (identical(tr$transform, "jitter") && !is.null(tr$amount)) {
    layer$jitter <- list(amount = tr$amount)
  }
  layer
}

# A stack modifier carries its `share` flag and its `baseline` onto the layer —
# the engine reads both from `layer$stack` (a StackSpec), mirroring
# `carry_jitter_params`. Only a non-NULL field attaches, so a bare
# `bar * count * stack` piles the values themselves, stands them on zero, and
# never mentions either field on the wire.
carry_stack_params <- function(layer, tr) {
  if (!identical(tr$transform, "stack")) return(layer)
  st <- list()
  if (!is.null(tr$share)) st$share <- tr$share
  if (!is.null(tr$baseline)) st$baseline <- tr$baseline
  if (length(st)) layer$stack <- st
  layer
}

# A bounds transform carries its two pre-computed column names onto the layer —
# the engine reads them from `layer$bounds` (a BoundsSpec). Unlike the other carry
# helpers the fields are always present (`bounds` requires both), so no NULL guard.
carry_bounds_params <- function(layer, tr) {
  if (identical(tr$transform, "bounds")) {
    # A NULL half simply does not reach the wire, which is what lets one atom
    # serve a band (lower/upper) and a zone (either pair, or both).
    layer$bounds <- Filter(Negate(is.null), list(
      lower = tr$lower, upper = tr$upper, start = tr$start, end = tr$end))
  }
  layer
}

# A partition carries its level columns onto the layer, the way `bounds` carries
# its four. `I()` keeps a single level a JSON array rather than a bare string:
# `levels` is a sequence in the IR whatever its length, and jsonlite unboxes a
# length-1 vector without it.
carry_partition_params <- function(layer, tr) {
  if (identical(tr$transform, "partition")) {
    layer$partition <- list(levels = I(as.character(tr$levels)))
    # `cross` rides the same list. Sent only when TRUE, so a nested partition's
    # wire form is byte-identical to what it was before this existed — which is
    # what keeps the corpus from re-recording every sunburst.
    if (isTRUE(tr$cross)) layer$partition$cross <- TRUE
  }
  layer
}

#' Derive a compound layer atom from a mark and a transform.
#'
#' @param e1 The mark, or a layer already carrying one transform.
#' @param e2 The transform being joined to it.
#' @return A layer `gog_atom` carrying the mark and its transforms.
#' @export
`*.gog_atom` <- function(e1, e2) {
  # Capture the operand expressions before forcing them, so a masking error can
  # name the offending symbol.
  s1 <- substitute(e1); s2 <- substitute(e2)

  # A transform used bare (`bar * bin`) arrives as the function itself — `bin`
  # is a function because it takes a parameter. Call it with its defaults so
  # `bar * bin` and `bar * bin(30)` reach the same code path.
  if (is.function(e1)) e1 <- e1()
  if (is.function(e2)) e2 <- e2()

  # A gog name masked by another attached package is the trap here: lubridate's
  # `interval()` (which `library(tidyverse)` loads) shadows the `interval` mark,
  # and the `is.function` call above then *invokes* it, yielding an S4 `Interval`
  # object whose `$type` fails cryptically. Catch a non-gog operand and say what
  # to do rather than failing deep inside on a missing slot.
  if (!inherits(e1, "gog_atom") || !inherits(e2, "gog_atom")) {
    bad <- deparse(if (!inherits(e1, "gog_atom")) s1 else s2)
    stop("gog: `", bad, "` is not a gog atom \u2014 another attached package likely ",
         "masks it (lubridate's `interval()`, loaded by `library(tidyverse)`, is ",
         "the usual culprit). Load gog after it so gog's names win, or qualify it ",
         "as `gog::", bad, "`.", call. = FALSE)
  }

  # mark * transform
  if (e1$type == "mark" && e2$type == "transform") {
    layer <- structure(
      list(type       = "layer",
           mark       = e1$mark,
           transforms = list(e2$transform),
           encodings  = list()),
      class = "gog_atom"
    )
    layer <- carry_bin_params(layer, e2)
    layer <- carry_density_params(layer, e2)
    layer <- carry_confidence_params(layer, e2)
    layer <- carry_jitter_params(layer, e2)
    layer <- carry_stack_params(layer, e2)
    layer <- carry_bounds_params(layer, e2)
    layer <- carry_partition_params(layer, e2)
    return(layer)
  }

  # layer * transform  (chaining: bar * bin * smooth)
  if (e1$type == "layer" && e2$type == "transform") {
    e1$transforms <- c(e1$transforms, list(e2$transform))
    e1 <- carry_bin_params(e1, e2)
    e1 <- carry_density_params(e1, e2)
    e1 <- carry_confidence_params(e1, e2)
    e1 <- carry_jitter_params(e1, e2)
    e1 <- carry_stack_params(e1, e2)
    e1 <- carry_bounds_params(e1, e2)
    e1 <- carry_partition_params(e1, e2)
    return(e1)
  }

  stop("gog: `*` not defined for ", e1$type, " * ", e2$type,
       ".\nUse `*` to combine a mark with a transform, e.g. bar * bin.")
}


# Did the caller's expression lose the table's name on the way in?
#
# The name comes from the expression written at the call site, and Law 4 makes it
# matter: "nearest table wins" resolves *by name*, so a table whose name was lost
# cannot be referred to again, and two that lost theirs collide.
#
# Almost every expression survives. A *call* is kept verbatim, so
# `data(data.frame(life = 50))` becomes the table `data.frame(life = 50)` — ugly,
# but unique, which is what lets inline literal tables compose (`marks/rule.qmd`
# stacks four of them in one plot). Law 8: guide taste softly, never forbid the
# ugly-but-legal.
#
# Exactly one expression is a real loss, and it is why this function exists:
#
#   df |> data()   fine. The native pipe is a *parser* transformation — `lhs |>
#                  f()` becomes `f(lhs)` before evaluation begins — so
#                  `substitute()` sees `df`, exactly as if it were typed.
#   df %>% data()  lost. magrittr's pipe is an ordinary function, so the argument
#                  arrives as its placeholder `.`. That is not ugly-but-unique:
#                  it is the *same* for every table piped that way, so two of
#                  them silently become one.
is_lost_name <- function(nm) identical(nm, ".")

#' Start a plot with a data frame.
#'
#' Every plot begins by naming its table. The columns are then written as bare
#' names in the channels that follow, and the nearest table wins where a plot has
#' more than one.
#'
#' Masks \code{utils::data()}, R's dataset loader. Use \code{utils::data()} to
#' load a built-in dataset; this one binds a table you already have. Both
#' readings of \code{data(mtcars)} are well formed, so that one case cannot be
#' told apart and is the reason the collision is documented rather than caught.
#'
#' @section Usage:
#' \preformatted{data(df, name = NULL)}
#'
#' The Usage section is written out here rather than generated, because
#' `R CMD check` reads a generated `\usage\{data(...)\}` as base R's dataset
#' loader and then reports `df` as a missing dataset. Suppressing the generated
#' one is the documented way out; the signature is not lost, only moved.
#'
#' @usage NULL
#'
#' @param df  An R data frame.
#' @param name  Optional table name (auto-derived from the variable name if
#'   omitted).
#' @return A `gog_spec` -- the start of a sentence, extended with `+`.
#' @examples
#' p <- data(mtcars) + point + x(wt) + y(mpg)
#'
#' # Naming the table explicitly, which matters when a plot has more than one.
#' q <- data(mtcars, name = "cars") + point + x(wt) + y(mpg)
#'
#' @export
data <- function(df, name = NULL) {
  # `data` masks `utils::data()`, R's dataset loader, and of the eight gog names
  # that mask a base *function* this is the only one whose failure is silent:
  # `data(mtcars)` is a legal sentence in both readings, so a caller who meant to
  # load a dataset gets a `gog_spec` and no complaint. That collision cannot be
  # resolved from in here (both intents are well-formed) and is documented
  # instead. What *is* catchable is the shape, and each case names the loader.
  arg <- paste(deparse(substitute(df)), collapse = " ")
  if (missing(df)) {
    stop("gog: `data()` needs the table the plot is about, as in `data(gapminder)`. ",
         "To load one of R's built-in datasets, that is `utils::data()`, which this ",
         "package's `data()` masks.", call. = FALSE)
  }
  # Forced here rather than several lines down, so "there is no such object" can
  # say which of the two `data()`s the caller probably wanted. A package dataset
  # that has to be loaded before use is exactly the case that reaches this.
  missing_object <- tryCatch({ force(df); NULL }, error = function(e) conditionMessage(e))
  if (!is.null(missing_object)) {
    stop("gog: `data(", arg, ")` cannot be built: ", missing_object,
         ". gog's `data()` binds a table that already exists in your session. To ",
         "*load* a dataset from a package first, that is `utils::data(", arg,
         ")`, the function this package's `data()` masks.", call. = FALSE)
  }
  if (!is.data.frame(df) && !(is.list(df) && length(df) > 0L && !is.null(names(df)))) {
    stop("gog: `data(", arg, ")` is not a table, but a ", class(df)[1L], ". A plot ",
         "begins with a data frame, or a named list of equal-length columns. If ",
         "you meant to load a built-in dataset, that is `utils::data()`, which ",
         "this package's `data()` masks.", call. = FALSE)
  }
  if (is.null(name)) {
    # `deparse` returns one element per line, so a long expression would
    # otherwise make `name` a vector and `setNames` below fail obscurely.
    name <- paste(deparse(substitute(df)), collapse = " ")
    name <- gsub('"', "", name, fixed = TRUE)
    # The name is gone rather than merely ugly. By §12's omission rule this is
    # the *ambiguous* case — only the caller knows what the table should be
    # called — so it is an Assumption, said out loud with the direction, never a
    # silent default. The caller wrote a pipe and has no reason to suspect it ate
    # the name, so the message names the cause, and the fix is a different pipe
    # as much as a different call.
    if (is_lost_name(name)) {
      warning("gog: magrittr's `%>%` replaced this table with its placeholder ",
              "`.` before `data()` could read the name, so the table is called ",
              "`data`. A layer resolves its bare columns against the nearest ",
              "table *by name*, so two tables piped this way collide. Either ",
              "name it \u2014 `data(name = \"...\")` \u2014 or use R's native pipe, ",
              "which keeps the name: `df |> data()` reads as `data(df)`.",
              call. = FALSE)
      name <- "data"
    }
  }
  spec <- list(
    data     = name,
    layers   = list(),
    coord    = "flat",
    title    = NULL,
    # `AxisSpec` is the axis's furniture, which is only its name: `tick_count`
    # moved to the channel binding 2026-07-26, beside `scale` and `limits`,
    # because how many ticks an axis gets is a property of the scale (spec §10).
    x_axis   = list(label = NULL),
    y_axis   = list(label = NULL),
    z_axis   = list(label = NULL),
    x        = NULL,
    y        = NULL,
    z        = NULL,
    channels = list()   # plot-scoped channels — those written before any mark
  )
  new_gog_spec(spec, name, df)
}

# The empty sentence, given the name of the table it is about. One skeleton,
# shared by every atom that can open a plot — `data()` and `query()`. Two copies
# of this list is how a field gets added to one data source and not the other.
new_spec <- function(name) {
  list(
    data     = name,
    layers   = list(),
    coord    = "flat",
    title    = NULL,
    x_axis   = list(label = NULL),
    y_axis   = list(label = NULL),
    z_axis   = list(label = NULL),
    x        = NULL,
    y        = NULL,
    z        = NULL,
    channels = list()
  )
}

new_gog_spec <- function(spec, name, frame) {
  structure(
    list(
      spec          = spec,
      data_frames   = stats::setNames(list(frame), name),
      current_layer = NULL,
      pending_data  = NULL
    ),
    class = "gog_spec"
  )
}

#' A table that lives in a database
#'
#' `query()` stands exactly where [data()] stands, and **nothing after it
#' changes** — the same operators, channels, bare column names and transforms:
#'
#' ```r
#' data(orders)                             + bar + x(status)
#' query(con, "SELECT * FROM orders")       + bar + x(status)
#' ```
#'
#' The SQL is confined to this one argument and never enters the grammar, which
#' is the point: `x(status)` is still a bare column resolved by the same mask,
#' not a fragment of another language.
#'
#' The connection is the caller's own — gog opens none. It is a **DBI**
#' connection, R's database standard, so RSQLite, RPostgres, RMariaDB, odbc and
#' bigrquery all reach this. DBI is Suggests rather than Imports: a user who
#' never writes SQL should not install a database stack to draw a plot.
#'
#' The query is **not run here**. It runs once, at render.
#'
#' @param con A DBI connection.
#' @param sql A single `SELECT`, as a string.
#' @param name What to call the table. Defaults to `"query"`; a second query in
#'   one sentence needs its own name, since a layer resolves its bare columns
#'   against the nearest table *by name*.
#' @return A `gog_spec`.
#' @export
query <- function(con, sql, name = NULL) {
  if (missing(con)) {
    stop("gog: `query()` needs a connection and a SELECT — ",
         "`query(con, \"SELECT ...\")`.", call. = FALSE)
  }
  # Checked before the missing-`sql` branch so that `query("SELECT ...")` — the
  # mistake `data()` invites, that atom taking one argument — is told the fix
  # rather than "argument \"sql\" is missing, with no default".
  if (is.character(con)) {
    stop("gog: `query()` takes the connection first, then the SELECT — ",
         "`query(con, \"SELECT ...\")`. A query on its own cannot say which ",
         "database it runs against, which is why the connection is written out ",
         "loud. If the rows are already in hand, that is `data(df)`.", call. = FALSE)
  }
  if (missing(sql)) {
    stop("gog: `query()` needs the SELECT as well as the connection — ",
         "`query(con, \"SELECT ...\")`.", call. = FALSE)
  }
  if (!is.character(sql) || length(sql) != 1L) {
    stop("gog: `query()` takes a SELECT as one string — ",
         "`query(con, \"SELECT ...\")`. Got ", class(sql)[1L],
         " of length ", length(sql), ".", call. = FALSE)
  }
  if (is.null(name)) name <- "query"

  new_gog_spec(new_spec(name), name, gog_query(con, sql))
}

# The unresolved table. Deliberately not executed when it is written: a query
# that ran at that moment would foreclose pushing the transform down to the
# database, because the planner has to see the whole sentence before it can know
# what to ask for. `resolve_query()` runs it once, at render.
gog_query <- function(con, sql) {
  structure(list(con = con, sql = sql), class = "gog_query")
}

resolve_query <- function(q, table) {
  if (!inherits(q, "gog_query")) return(q)
  if (!requireNamespace("DBI", quietly = TRUE)) {
    stop("gog: `query()` needs the DBI package, which is not installed — ",
         "`install.packages(\"DBI\")`, plus the driver for your database ",
         "(RSQLite, RPostgres, odbc). DBI is Suggests rather than a hard ",
         "dependency, so drawing a plot from a data frame never asks for it.",
         call. = FALSE)
  }
  rows <- tryCatch(
    DBI::dbGetQuery(q$con, q$sql),
    error = function(e) {
      stop("gog: the query for `", table, "` failed: ", conditionMessage(e),
           call. = FALSE)
    }
  )
  if (!is.data.frame(rows)) {
    stop("gog: the query for `", table, "` did not return a table. `query()` ",
         "takes a SELECT — a statement that produces rows.", call. = FALSE)
  }
  rows
}

# ---------------------------------------------------------------------------
# + operator
# ---------------------------------------------------------------------------

#' Compose plot atoms with \code{+}.
#'
#' @param lhs The plot so far, or the first atom of a sentence.
#' @param rhs The atom, layer or table being added to it.
#' @return A `gog_spec` carrying both sides.
#' @export
`+.gog_spec` <- function(lhs, rhs) {
  # A mark that takes a knob is a *function* (like `box`), and used bare
  # (`... + box`) it arrives here as the function itself. Call it with its
  # defaults so `+ box` and `+ box(whiskers = "range")` reach the same path —
  # the mirror of what `*.gog_atom` does for a bare transform. Only `box` is a
  # function today, so this is inert for every other atom.
  if (is.function(lhs)) lhs <- lhs()
  if (is.function(rhs)) rhs <- rhs()

  # A bare data frame in the chain could not normally even reach this
  # function: base R owns `Ops.data.frame`, so `df + point` runs base's
  # arithmetic and `spec + df` is a dispatch stalemate ("Incompatible
  # methods") that falls back to the internal `+`. Either way the user got
  # "non-numeric argument to binary operator" — R's error, with no
  # direction. From R 4.3, `chooseOpsMethod()` (defined below) claims those
  # calls for gog, so the mistake lands here and the message can say what
  # to write. `substitute()` still sees the caller's expression inside an
  # Ops method, which is what lets the advice name the actual frame.
  frame_name <- function(expr) {
    nm <- paste(deparse(expr), collapse = " ")
    if (grepl("^[a-zA-Z.][a-zA-Z0-9._]*$", nm)) nm else "df"
  }
  if (is.data.frame(lhs)) {
    tail <- if (inherits(rhs, "gog_atom") && !is.null(rhs$mark)) {
      paste0(rhs$mark, " + ...")
    } else {
      "..."
    }
    stop("gog: a plot starts with `data()`, which names the table \u2014 ",
         "columns are bare names and the nearest named table wins, so the ",
         "name is load-bearing. Write `data(", frame_name(substitute(lhs)),
         ") + ", tail, "`.", call. = FALSE)
  }

  # `point + x(gdp)` with no data at all: both operands are atoms, and this
  # function is also `+.gog_atom`, so the missing subject can be said.
  if (inherits(lhs, "gog_atom")) {
    stop("gog: these atoms have no plot to join \u2014 the sentence starts with ",
         "the data: `data(df) + point + x(gdp) + ...`.", call. = FALSE)
  }

  # mid-expression data(): merge data frames, mark pending table for next mark
  if (inherits(rhs, "gog_spec")) {
    new_name        <- names(rhs$data_frames)[1]
    # Two *different* tables under one name is Law 4 with its floor removed:
    # "nearest table wins" has nothing to choose between, the first one answers
    # every lookup, and the second one's columns come back reported as
    # misspellings — an error that blames the reader for the binding's loss. The
    # same name carrying the same frame is a harmless restatement, so only a
    # genuine clash refuses.
    if (new_name %in% names(lhs$data_frames) &&
        !identical(lhs$data_frames[[new_name]], rhs$data_frames[[1]])) {
      stop("gog: two different tables are both called `", new_name,
           "` \u2014 a layer resolves its bare columns against the nearest table ",
           "by name, so one of these can never be reached. Give them ",
           "distinct names: `data(name = \"...\")`.", call. = FALSE)
    }
    lhs$data_frames <- c(lhs$data_frames, rhs$data_frames)
    lhs$pending_data <- new_name
    return(lhs)
  }

  # A bare data frame mid-expression: `data(df) + point + df2 + line`.
  if (is.data.frame(rhs)) {
    stop("gog: a data frame joins the plot through `data()` \u2014 write `+ data(",
         frame_name(substitute(rhs)), ")`. The name is what a later layer ",
         "resolves its columns against (nearest table wins), and a bare ",
         "frame has none.", call. = FALSE)
  }

  if (!inherits(rhs, "gog_atom")) {
    stop("Right-hand side of + must be a gog_atom or gog_spec, got: ",
         paste(class(rhs), collapse = "/"))
  }

  switch(rhs$type,

    mark = {
      # Push current open layer to the spec's layer list
      if (!is.null(lhs$current_layer)) {
        lhs$spec$layers <- c(lhs$spec$layers, list(lhs$current_layer))
      }
      lhs$current_layer <- list(
        mark      = rhs$mark,
        encodings = list(),
        transforms = list(),
        data      = lhs$pending_data
      )
      # A `box` mark may carry its one knob (the whisker rule); a NULL adds
      # nothing, so every other mark's layer stays untouched.
      lhs$current_layer$box <- rhs$box
      lhs$pending_data <- NULL
    },

    # Pre-built layer atom from the * operator (e.g. bar * bin)
    layer = {
      if (!is.null(lhs$current_layer)) {
        lhs$spec$layers <- c(lhs$spec$layers, list(lhs$current_layer))
      }
      lhs$current_layer <- list(
        mark       = rhs$mark,
        encodings  = if (length(rhs$encodings) == 0) list() else rhs$encodings,
        transforms = rhs$transforms,
        data       = lhs$pending_data
      )
      # Carry bin/density/confidence/jitter/stack parameters if the layer atom has
      # them; a NULL assignment simply adds nothing, so a plain layer stays
      # untouched.
      lhs$current_layer$bin <- rhs$bin
      lhs$current_layer$density <- rhs$density
      lhs$current_layer$confidence <- rhs$confidence
      lhs$current_layer$jitter <- rhs$jitter
      lhs$current_layer$stack <- rhs$stack
      lhs$current_layer$bounds <- rhs$bounds
      lhs$current_layer$partition <- rhs$partition
      lhs$pending_data <- NULL
    },

    coord_x = { lhs <- set_position(lhs, "x", rhs) },
    coord_y = { lhs <- set_position(lhs, "y", rhs) },
    coord_z = { lhs <- set_position(lhs, "z", rhs) },

    # The viewing angle rides on the coordinate space: `{"space":{turn,tilt}}`
    # matches the Rust `CoordSpace::Space(SpaceView)`, while a plain 2-D plot keeps
    # sending `coord = "flat"`.
    coord_space = { lhs$spec$coord <- list(space = list(turn = rhs$turn, tilt = rhs$tilt)) },

    # Polar carries its one view parameter the same way `space` carries its two:
    # `{"polar":{"start":0}}` matches `CoordSpace::Polar(PolarView)`.
    coord_polar = { lhs$spec$coord <- list(polar = list(start = rhs$start)) },

    # Nest carries no view parameter at all, so it crosses as the bare string
    # `"nest"` — `CoordSpace::Nest` is a unit variant, like `globe` and `map`.
    # There is no angle to send because there is nothing underneath to view from
    # an angle: a packing is not a map of the plane.
    coord_nest = { lhs$spec$coord <- "nest" },

    color   = { lhs <- set_channel(lhs, "color",   rhs$field, rhs$scale, rhs$base, rhs$limits) },
    group   = { lhs <- set_channel(lhs, "group",   rhs$field) },
    size    = { lhs <- set_channel(lhs, "size",    rhs$field, rhs$scale, rhs$base, rhs$limits) },
    shape   = { lhs <- set_channel(lhs, "shape",   rhs$field) },
    opacity = { lhs <- set_channel(lhs, "opacity", rhs$field, rhs$scale, rhs$base, rhs$limits) },
    label   = { lhs <- set_channel(lhs, "label",   rhs$field) },
    pattern = { lhs <- set_channel(lhs, "pattern", rhs$field) },
    play    = { lhs <- set_channel(lhs, "play",    rhs$field, speed = rhs$speed) },

    style   = { lhs <- set_style(lhs, rhs$props) },

    palette = { lhs$spec$palette <- rhs$value },

    # Merged rather than replaced, so two `theme()` calls in one sentence
    # accumulate the way two `style()` calls on one mark do. Only the properties
    # actually named are written, which is what keeps "the caller said nothing"
    # distinct from "the caller asked for the default" (`ThemeSpec`, spec §7).
    theme = {
      if (is.null(lhs$spec$theme)) lhs$spec$theme <- list()
      for (key in c("preset", "grid", "ratio", "tick_angle", "font_size",
                    "background", "strip", "strip_text", "frame", "width", "height")) {
        if (!is.null(rhs[[key]])) lhs$spec$theme[[key]] <- rhs[[key]]
      }
    },

    title   = { lhs$spec$title          <- rhs$value },
    x_label = { lhs$spec$x_axis$label   <- rhs$value },
    y_label = { lhs$spec$y_axis$label   <- rhs$value },
    z_label = { lhs$spec$z_axis$label   <- rhs$value },

    order = {
      lhs$spec$order <- list(field = rhs$order_field, descending = rhs$descending)
    },

    facet = stop(
      "gog: `facet()` joins with `|` (panels side by side) or `/` (panels ",
      "stacked), not `+`. Write `plot | facet(", rhs$field, ")` or `plot / facet(",
      rhs$field, ")`.", call. = FALSE
    ),

    # `... + y(b) / facet(g)`: R gave `/` the atom before it (see facet_join).
    # Apply the carried atom first, then the facet — left to right, as written.
    atom_then_facet = {
      lhs <- lhs + rhs$atom
      if (is.null(lhs$spec$facet)) lhs$spec$facet <- list(col = NULL, row = NULL)
      lhs$spec$facet[[rhs$slot]] <- rhs$facet
      if (!is.null(rhs$wrap)) lhs$spec$facet$wrap <- rhs$wrap
    },

    warning("gog: unknown atom type '", rhs$type, "' \u2014 ignored")
  )

  lhs
}

#' @export
`+.gog_atom` <- `+.gog_spec`

#' @export
`+.gog_page` <- function(lhs, rhs) {
  # A page is plots arranged; an atom belongs to one of them. Adding a title to
  # the page as a whole is real and not built — designed, not implemented — and
  # saying so beats R's "non-numeric argument to binary operator".
  what <- if (inherits(rhs, "gog_atom")) paste0("`", rhs$type, "()`") else "that"
  stop("gog: ", what, " belongs to a plot, and the left side is a page of them. ",
       "Write it into the plot it describes, before composing: ",
       "`(plot + theme(...)) | other_plot`.", call. = FALSE)
}

# From R 4.3, when the two operands of a binary operator carry *different* S3
# methods, `chooseOpsMethod()` is consulted before R gives up with an
# "Incompatible methods" warning and the internal operator. gog always
# volunteers: an expression mixing a gog object with anything else is gog's
# mistake to explain, and the internal `+` could only ever say "non-numeric
# argument". On R older than 4.3 these are simply never consulted and the
# base behavior stands.
#' @export
chooseOpsMethod.gog_spec <- function(x, y, mx, my, cl, reverse) TRUE

#' @export
chooseOpsMethod.gog_atom <- chooseOpsMethod.gog_spec

# ---------------------------------------------------------------------------
# | and / operators — facet (small multiples)
#
#   plot | facet(cyl)                one panel per category, side by side
#   plot / facet(drv)                one panel per category, stacked
#   plot | facet(cyl) / facet(drv)   crossed into a grid
#
# R evaluates `/` before `|`, so in the last line `facet(cyl) / facet(drv)`
# resolves first — to a *pair* atom — and `|` then assigns the pair's first
# column to its own slot and the second to the other. Read left to right, each
# operator applies to the facet written after it, which is what the eye expects.
#
# All four methods are the same two functions on purpose. When both operands
# of an operator carry a class, S3 uses the method only if the two classes
# resolve to the *identical* function — different functions make R warn
# "Incompatible methods" and fall back to the internal operator, which cannot
# handle lists. Sharing the function object is what keeps `plot | facet(g)`
# dispatching cleanly.
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# Composition — separate plots arranged on one page
#
#   plot_a | plot_b        side by side
#   plot_a / plot_b        one above the other
#   top / (main | right)   nested: the marginal plot
#
# Faceting is one plot split by a variable and sharing everything; composition
# is several plots on one page, each keeping its own coordinate space (spec
# §11). The two wear the same operators and are told apart by the operand
# types, which is what the facet design left room for.
#
# What relates the composed plots is one rule, and the engine owns it: the same
# column on the same axis in two of them is one axis — one scale, one panel
# extent, drawn once (`render::page`). Nothing about it is decided here.
#
# **`/` binds tighter than `|` in R**, so `a | b / c` reads as `a | (b / c)`.
# Parenthesize when the reading matters; the marginal plot above does.
# ---------------------------------------------------------------------------

# A plot or a page — the two things composition takes on either side.
is_figure <- function(x) inherits(x, "gog_spec") || inherits(x, "gog_page")

# The wire form of one operand: a finalized spec, or a page's own node.
figure_wire <- function(x) {
  if (inherits(x, "gog_page")) x$page else finalize_spec(x)$spec
}

# The cells this operand contributes to a page running `arrange`.
#
# A page already running that way is *flattened* into it, so `a | b | c` is one
# row of three rather than a row of (a row of two, and one) — the reading the
# eye gives it. A page running the other way stays a cell of its own, which is
# what makes `top / (main | right)` two rows, the second holding two plots.
figure_cells <- function(x, arrange) {
  if (inherits(x, "gog_page") && identical(x$page$arrange, arrange)) {
    return(x$page$cells)
  }
  list(figure_wire(x))
}

# Two figures' tables, under Law 4's rule: one name, one table.
merge_frames <- function(lhs, rhs) {
  frames <- lhs$data_frames
  for (name in names(rhs$data_frames)) {
    if (name %in% names(frames) &&
        !identical(frames[[name]], rhs$data_frames[[name]])) {
      stop("gog: two different tables on one page are both called `", name,
           "` \u2014 a layer resolves its bare columns against the nearest table by ",
           "name, so one of these can never be reached. Give them distinct ",
           "names: `data(name = \"...\")`.", call. = FALSE)
    }
    frames[[name]] <- rhs$data_frames[[name]]
  }
  frames
}

page_compose <- function(lhs, rhs, arrange, op) {
  structure(
    list(
      page = list(arrange = arrange,
                  cells   = c(figure_cells(lhs, arrange), figure_cells(rhs, arrange))),
      data_frames = merge_frames(lhs, rhs)
    ),
    class = "gog_page"
  )
}

#' Facet a plot into panel columns, or set a plot beside another.
#'
#' @param e1,e2 The plot on the left, and the facet or plot joined to it.
#' @return A faceted `gog_spec`, or a `gog_page` when two plots are composed.
#' @name facet-operators
#' @rdname facet-operators
#' @export
`|.gog_spec` <- function(e1, e2) facet_join(e1, e2, "col", "|")

#' @description `|` facets into panel columns and sets plots side by side;
#'   `/` facets into panel rows and sets them one above the other.
#' @rdname facet-operators
#' @export
`/.gog_spec` <- function(e1, e2) facet_join(e1, e2, "row", "/")

#' @export
`|.gog_atom` <- `|.gog_spec`

#' @export
`/.gog_atom` <- `/.gog_spec`

#' @export
`|.gog_page` <- `|.gog_spec`

#' @export
`/.gog_page` <- `/.gog_spec`

#' @export
chooseOpsMethod.gog_page <- chooseOpsMethod.gog_spec

facet_join <- function(lhs, rhs, slot, op) {
  other <- if (slot == "col") "row" else "col"

  # Two plots: composition, not faceting. The operators tell the two apart by
  # what is on their right — a facet split takes an atom, a page takes another
  # plot — which is the door the design left open when `plot | plot` still
  # refused (spec §11).
  if (is_figure(lhs) && is_figure(rhs)) {
    return(page_compose(lhs, rhs, if (slot == "col") "beside" else "below", op))
  }
  # A page can only be composed further. `(a | b) | facet(g)` would be faceting
  # a page, which is a split of something that is not one plot.
  if (inherits(lhs, "gog_page")) {
    stop("gog: `", op, "` faceted a page of plots, and a facet splits *one* plot ",
         "by a column. Facet the plots before composing them: ",
         "`(plot ", op, " facet(g)) ", op, " other_plot`.", call. = FALSE)
  }

  # The operator reached an atom instead of the plot. Two legitimate ways in:
  #
  #   facet(a) / facet(b)   — an inner pair, waiting for its plot; which slot
  #                           each column lands in is decided by the *outer*
  #                           operator when the pair reaches it.
  #   y(b) / facet(a)       — R evaluates `/` before `+`, so in
  #                           `data(df) + point + x(a) + y(b) / facet(g)` the
  #                           operator grabs the atom written just before it,
  #                           not the plot. Carry both forward: `+` applies the
  #                           atom and then the facet — exactly what was
  #                           written, read left to right.
  if (inherits(lhs, "gog_atom")) {
    if (inherits(rhs, "gog_atom") && rhs$type == "facet") {
      if (lhs$type == "facet") {
        return(structure(
          list(type = "facet_pair", first = lhs$field, second = rhs$field,
               wrap = if (is.null(lhs$wrap)) rhs$wrap else lhs$wrap),
          class = "gog_atom"
        ))
      }
      if (lhs$type == "facet_pair") {
        stop("gog: a plot crosses at most two facet columns \u2014 one for the ",
             "panel rows, one for the columns.", call. = FALSE)
      }
      return(structure(
        list(type = "atom_then_facet", atom = lhs, facet = rhs$field,
             slot = slot, wrap = rhs$wrap),
        class = "gog_atom"
      ))
    }
    stop("gog: `", op, "` facets a *plot* \u2014 build the plot first, then facet ",
         "it: `data(df) + point + x(a) + y(b) ", op, " facet(g)`.",
         call. = FALSE)
  }

  # `gm_ae | facet(g)` — a bare data frame is not a plot; the sentence it
  # opens is missing. (Reachable because chooseOpsMethod hands gog the call.)
  if (!inherits(lhs, "gog_spec")) {
    stop("gog: `", op, "` facets a gog plot, and the left side is not one. ",
         "Start the sentence with `data()`: `data(df) + point + x(a) + y(b) ",
         op, " facet(g)`.", call. = FALSE)
  }
  if (!inherits(rhs, "gog_atom") ||
      !(rhs$type %in% c("facet", "facet_pair"))) {
    stop("gog: the right side of `", op, "` must be `facet(<column>)`.",
         call. = FALSE)
  }

  if (is.null(lhs$spec$facet)) lhs$spec$facet <- list(col = NULL, row = NULL)
  if (rhs$type == "facet") {
    lhs$spec$facet[[slot]] <- rhs$field
    # The count rides with the column it was written on. Which *way* the line
    # runs is already settled by the operator that brought us here, so nothing
    # about the slot is recorded twice.
    if (!is.null(rhs$wrap)) lhs$spec$facet$wrap <- rhs$wrap
  } else {
    # `plot | facet(a) / facet(b)`: the operator's own slot takes the first
    # column written, the other slot the second — left to right, as read.
    lhs$spec$facet[[slot]]  <- rhs$first
    lhs$spec$facet[[other]] <- rhs$second
    # A crossing with a `wrap` on either column is refused by the engine, with
    # the reason. Carried rather than dropped: silently ignoring a binding is
    # what spec §12 forbids, and the refusal is more use than a shrug.
    if (!is.null(rhs$wrap)) lhs$spec$facet$wrap <- rhs$wrap
  }
  lhs
}

# ---------------------------------------------------------------------------
# print — auto-render in RStudio viewer / browser
# ---------------------------------------------------------------------------

#' @export
print.gog_spec <- function(x, ...) {
  render_and_display(x)
  invisible(x)
}

#' @export
print.gog_page <- print.gog_spec

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# A binding is a column plus, optionally, how its numbers become positions.
# `scale = NULL` goes over the wire as JSON null, which is what the Rust
# `Option<ScaleType>` already expected — an unscaled binding is unchanged.
# `limits` is the domain the channel runs over when the data is not the
# authority (spec §10). Two numbers with `NA` for an end the data should decide;
# `na = "null"` in `toJSON` turns that into the engine's `[0, null]` without a
# special case here. `I()` keeps it an array at length two — the same guard the
# data columns need, for the same reason.
channel_def <- function(field, scale = NULL, base = NULL, limits = NULL,
                        tick_count = NULL, speed = NULL, free = NULL) {
  list(field = field, scale = scale, base = base,
       limits = if (is.null(limits)) NULL else I(limits),
       tick_count = tick_count, speed = speed, free = free)
}

# A position binding, scoped by position like every other channel.
#
#   x(gdp) + point + line          -> both layers read `gdp`
#   point + data(notes) + text + x(at)
#                                  -> the text layer reads `at`, the point `gdp`
#
# This is one axis with two column names, never two axes: the scale, the ticks
# and the coordinate space are the plot's throughout, and only which column of
# *this layer's* table supplies the values is local. A layer asking for its own
# scale is the secondary axis spec §18 refuses, and `check_layer_position`
# refuses it here too.
#
# `x`/`y`/`z` were plot-scoped unconditionally until 2026-07-25, which read as a
# rule about axes and was really a rule about names — it made a second `data()`
# unable to say where its rows go, and the annotation sentence in spec §8 could
# not run.
set_position <- function(gog, ch, rhs) {
  cd <- channel_def(rhs$field, rhs$scale, rhs$base, rhs$limits, rhs$tick_count,
                    free = rhs$free)
  if (is.null(gog$current_layer)) {
    gog$spec[[ch]] <- cd                    # written before any mark → the plot's
  } else {
    gog$current_layer$encodings[[ch]] <- cd # written after a mark → that layer's
  }
  gog
}

# Position decides scope, and binding is always forward.
#
#   color(g) + line + point   -> plot-scoped: every layer that can take it
#   line + color(g) + point   -> the line only
#
# This replaces a broadcast that also reached *backwards* over layers already
# committed. That was two bugs in one: it made `point + color(g) + line` and
# `line + point + color(g)` mean different things, and it left no way to scope
# a channel that only one mark accepts — `line + color(country) + point +
# size(population)` put `size` on the line, which has none, and refused to
# render. Reaching forward from the plot level covers the useful case without
# either problem, and matches how x/y/z have always worked.
set_channel <- function(gog, ch, field, scale = NULL, base = NULL, limits = NULL,
                        speed = NULL) {
  cd <- channel_def(field, scale, base, limits, speed = speed)
  if (is.null(gog$current_layer)) {
    gog$spec$channels[[ch]] <- cd   # written before any mark → plot-scoped
  } else {
    gog$current_layer$encodings[[ch]] <- cd
  }
  gog
}

# Apply constant settings to the nearest preceding mark — and only that one.
#
# Deliberately NOT broadcast, unlike set_channel(). The backward reach above is
# already a known divergence from spec §7b ("channels bind to the nearest
# preceding mark") and makes `point + color(g) + line` mean something different
# from `line + point + color(g)`. A new atom should not inherit an asymmetry
# that is queued for removal — and styling is per-layer decoration by nature:
# `line + point + style(color = "gray")` graying the line as a side effect of
# styling the points would be a surprise, not a convenience.
set_style <- function(gog, props) {
  if (is.null(gog$current_layer)) {
    stop("gog: `style()` has no mark to style. Put it after a mark, ",
         "e.g. `point + style(color = \"tomato\")`.", call. = FALSE)
  }
  if (is.null(gog$current_layer$style)) gog$current_layer$style <- list()
  for (nm in names(props)) gog$current_layer$style[[nm]] <- props[[nm]]
  gog
}

# Push the last open layer into spec$layers and return the sealed spec.
finalize_spec <- function(gog) {
  if (!is.null(gog$current_layer)) {
    gog$spec$layers   <- c(gog$spec$layers, list(gog$current_layer))
    gog$current_layer <- NULL
  }
  gog
}
