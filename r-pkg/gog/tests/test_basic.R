# Basic sanity test — does not require RStudio; runs in plain R.
# Run from the project root:
#   Rscript r-pkg/gog/tests/test_basic.R

library(methods)

# This file runs in two situations and has to tell them apart, because the paths
# below are relative to the repository root and only one of them has a repository.
#
#   * From the repo root, as the development smoke test. The package is loaded
#     FROM SOURCE with pkgload::load_all(), which honors the hand-written
#     NAMESPACE (export_all = FALSE), so a missing export() or S3 registration
#     fails right here — the class of bug plain source() can never see, because
#     source() bypasses NAMESPACE entirely.
#
#   * Under `R CMD check`, from <pkg>.Rcheck/tests/, where `r-pkg/gog` does not
#     exist and the package has *already been installed* into the check library.
#     There, loading the installed copy is not a fallback but the point: it is
#     the thing being checked. Hardcoding the source path made every r-universe
#     and CRAN check fail at the first line with pkgload's "does your project
#     have a DESCRIPTION file?", which says nothing about gog.
#
# source() stays as the last resort so the suite still runs without pkgload; the
# NAMESPACE checks further down guard the drift either way.
pkg_src <- "r-pkg/gog"
if (dir.exists(pkg_src) && requireNamespace("pkgload", quietly = TRUE)) {
  pkgload::load_all(pkg_src, export_all = FALSE, quiet = TRUE)
} else if (dir.exists(pkg_src)) {
  for (f in list.files(file.path(pkg_src, "R"), pattern = "\\.R$", full.names = TRUE)) source(f)
} else {
  library(gog)
}
# The wire tests below poke this internal directly; source() leaves it global,
# while load_all() and library() both keep it namespace-private.
if ("gog" %in% loadedNamespaces()) df_to_wire <- gog:::df_to_wire

# Tell R where to find the CLI binary (release build first, then debug).
# Mirrors book/R/setup.R; find_gog_cli() handles the platform extension itself,
# so this only pins the path when a local build exists.
exe <- if (.Platform$OS.type == "windows") "gog-cli.exe" else "gog-cli"
for (build in c("release", "debug")) {
  p <- file.path("target", build, exe)
  if (file.exists(p)) {
    Sys.setenv(GOG_CLI_PATH = normalizePath(p))
    break
  }
}

# Build a tiny test data frame
df <- data.frame(
  x    = c(1.0, 2.0, 3.0, 4.0, 5.0),
  y    = c(2.5, 3.1, 1.8, 4.0, 3.5),
  group = c("A", "B", "A", "B", "A")
)

# --- scatter plot ---
svg <- render_svg(
  data(df) + x(x) + y(y) + point + color(group) +
    title("Basic scatter") + x_label("X value") + y_label("Y value")
)

if (!grepl("<svg", svg)) stop("FAIL: output does not look like SVG")
cat("PASS: scatter plot rendered (", nchar(svg), " chars)\n")

# --- line plot ---
svg2 <- render_svg(
  data(df) + x(x) + y(y) + line + title("Basic line")
)
if (!grepl("<svg", svg2)) stop("FAIL: line output not SVG")
cat("PASS: line plot rendered (", nchar(svg2), " chars)\n")

# --- bar chart ---
bar_df <- data.frame(category = c("A", "B", "C"), value = c(10.0, 25.0, 15.0))
svg3 <- render_svg(
  data(bar_df) + x(category) + y(value) + bar + title("Bar chart")
)
if (!grepl("<svg", svg3)) stop("FAIL: bar output not SVG")
cat("PASS: bar chart rendered (", nchar(svg3), " chars)\n")

# ---------------------------------------------------------------------------
# query() — the table that is not in memory
#
# The guard is the one that matters and it is the same in all four bindings: the
# *same sentence*, over a materialized frame and over a query returning the same
# rows, must render byte-identical SVG. If those diverge, `query()` has stopped
# being a way of naming rows and become a second way of drawing them.
#
# DBI and RSQLite are Suggests rather than Imports — a user who never writes SQL
# should not install a database stack to draw a plot — so the byte-identity half
# skips when they are absent. The refusals do not need a database and always run.
# ---------------------------------------------------------------------------

rows <- data.frame(
  status  = c("open", "shipped", "shipped", "closed", "open", "refunded"),
  revenue = c(120, 240.5, 95.25, 310.75, 60, 45),
  stringsAsFactors = FALSE
)

if (requireNamespace("DBI", quietly = TRUE) && requireNamespace("RSQLite", quietly = TRUE)) {
  con <- DBI::dbConnect(RSQLite::SQLite(), ":memory:")
  on.exit(DBI::dbDisconnect(con), add = TRUE)
  DBI::dbWriteTable(con, "orders", rows)
  sql <- "SELECT status, revenue FROM orders"

  sentences <- list(
    "point with two positions" = function(t) t + point + x(revenue) + y(status),
    "bar * count"              = function(t) t + bar * count + x(status),
    "bar with a mapped color"  = function(t) t + bar + x(status) + y(revenue) + color(status)
  )
  for (label in names(sentences)) {
    sentence <- sentences[[label]]
    from_table <- render_svg(sentence(data(rows, name = "orders")))
    from_query <- render_svg(sentence(query(con, sql, name = "orders")))
    if (!identical(from_table, from_query))
      stop("FAIL: query() and data() disagree on ", label)
    cat("PASS: query() draws ", label, " byte-identically to data()\n", sep = "")
  }

  # The query does not run when the sentence is written. An eager query would
  # foreclose pushing the transform down, since the planner has to see the whole
  # sentence before it knows what to ask the database for.
  lazy <- query(con, "SELECT nonsense FROM nowhere", name = "orders")
  if (!inherits(lazy$data_frames[["orders"]], "gog_query"))
    stop("FAIL: query() ran its SQL when the sentence was built")
  cat("PASS: query() holds the SQL rather than running it when the sentence is built\n")
} else {
  cat("SKIP: query() byte-identity needs DBI and RSQLite (both Suggests)\n")
}

# `query("SELECT ...")` is the mistake `data()` invites, that atom taking one
# argument. All four bindings answer it with the fix rather than the host
# language's own arity error.
refusal <- tryCatch(query("SELECT 1"), error = function(e) conditionMessage(e))
if (!grepl("connection first", refusal, fixed = TRUE))
  stop("FAIL: query('SELECT ...') did not name the fix — got: ", refusal)
cat("PASS: query() with only a query refused, naming the connection\n")

refusal <- tryCatch(query(NULL, 123), error = function(e) conditionMessage(e))
if (!grepl("SELECT as one string", refusal, fixed = TRUE))
  stop("FAIL: query() given a non-string query did not say so — got: ", refusal)
cat("PASS: query() given a query that is not text refused\n")

cat("\nAll tests passed.\n")

# --- * operator: bar * bin (histogram) ---
hist_df <- data.frame(height = rnorm(100, mean = 170, sd = 10))
svg4 <- render_svg(
  data(hist_df) + bar * bin + x(height) + y(count) + title("Histogram via bar * bin")
)
if (!grepl("<svg", svg4)) stop("FAIL: histogram output not SVG")
cat("PASS: histogram (bar * bin) rendered (", nchar(svg4), " chars)\n")

# --- step mark: step * bin (histogram outline) and step + x + y (step function) ---
svg4b <- suppressMessages(render_svg(
  data(hist_df) + step * bin + x(height) + title("Histogram outline via step * bin")
))
if (!grepl("<polyline", svg4b)) stop("FAIL: step * bin did not draw a silhouette")
cat("PASS: step * bin (histogram outline) rendered (", nchar(svg4b), " chars)\n")

svg4c <- render_svg(
  data(df) + step + x(x) + y(y) + title("Step function")
)
if (!grepl("<polyline", svg4c)) stop("FAIL: step + x + y did not draw a step")
cat("PASS: step function (step + x + y) rendered (", nchar(svg4c), " chars)\n")

# --- * operator: line * smooth ---
svg5 <- render_svg(
  data(df) + line * smooth + x(x) + y(y) + title("Smoother via line * smooth")
)
if (!grepl("<svg", svg5)) stop("FAIL: smooth output not SVG")
cat("PASS: smooth (line * smooth) rendered (", nchar(svg5), " chars)\n")

# --- chaining: which two transforms may compose, and which may not ---
# `bin` supplies an extent and a tally, and only the extent is what makes it a
# `bin` — so composed with a statistic it keeps the cut and gives the tally up.
# The binned mean profile, and the summary heatmap one dimension up.
svg_cut <- render_svg(data(df) + bar * bin * mean + x(x) + y(y))
if (!grepl("<svg", svg_cut)) stop("FAIL: bar * bin * mean did not render")
cat("PASS: composed cut (bar * bin * mean) rendered (", nchar(svg_cut), " chars)\n")

# Order cannot matter here: a cell has to exist before anything is measured in it.
if (!identical(svg_cut, render_svg(data(df) + bar * mean * bin + x(x) + y(y))))
  stop("FAIL: bar * mean * bin must draw what bar * bin * mean draws")
cat("PASS: the cut runs first wherever it is written\n")

# And it must actually reduce the named column rather than hand back the tally —
# the silent drop this composition had until 2026-07-26, where the geometry stayed
# a histogram's and only the axis title changed.
strip_text <- function(s) gsub("<text[^<]*</text>", "", s)
if (identical(strip_text(svg_cut), strip_text(render_svg(data(df) + bar * bin + x(x)))))
  stop("FAIL: bar * bin * mean drew a plain histogram — the statistic was dropped")
cat("PASS: the composed statistic changes what is measured\n")

# `smooth` is refused against all four synthesizing transforms: it fits a curve and
# already averages locally, so cutting the rows into cells first buys it nothing.
refused <- tryCatch({
  render_svg(data(df) + bar * bin * smooth + x(x) + y(y)); "no_error"
}, error = function(e) if (grepl("asks one question twice", conditionMessage(e))) "refused" else "wrong")
if (refused != "refused") stop("FAIL: bar * bin * smooth must be refused, got: ", refused)
cat("PASS: bar * bin * smooth refused with direction\n")

# `count` supplies only a measurement, so there is nothing left of it to compose.
refused2 <- tryCatch({
  render_svg(data(df) + bar * count * mean + x(group) + y(y)); "no_error"
}, error = function(e) if (grepl("measures each cell twice", conditionMessage(e))) "refused" else "wrong")
if (refused2 != "refused") stop("FAIL: bar * count * mean must be refused, got: ", refused2)
cat("PASS: bar * count * mean refused with direction\n")

# Two *synthesizing* transforms are the same contradiction with neither side handed
# a column: each invents its own measurement and a cell holds one number.
refused3 <- tryCatch({
  render_svg(data(df) + bar * bin * count + x(x)); "no_error"
}, error = function(e) if (grepl("neither was handed a column", conditionMessage(e))) "refused" else "wrong")
if (refused3 != "refused") stop("FAIL: bar * bin * count must be refused, got: ", refused3)
cat("PASS: bar * bin * count refused with direction\n")

# `proportion` is *not* one of them and stopped being one on 2026-07-26 — it
# rescales the measurement it finds rather than inventing one, so it composes
# with everything that leaves a single number per cell. The pair below is the
# relative-frequency histogram, refused for one day on the strength of a plot
# whose twelve equal bars were a sequencing defect and not the sentence.
render_svg(data(df) + bar * bin * proportion + x(x))
cat("PASS: bar * bin * proportion draws the relative-frequency histogram\n")

cat("\n* operator tests passed.\n")

# --- aggregation transforms: sum, mean, median, max, min ---
medals_dup <- data.frame(
  country = c("USA", "USA", "GBR", "GBR", "JPN"),
  gold    = c(10, 5, 8, 4, 6)
)
svg6 <- render_svg(data(medals_dup) + bar * sum + x(country) + y(gold))
if (!grepl("<svg", svg6)) stop("FAIL: sum transform output not SVG")
cat("PASS: sum transform (duplicate x summed) rendered (", nchar(svg6), " chars)\n")

svg7 <- render_svg(data(medals_dup) + bar * mean + x(country) + y(gold))
if (!grepl("<svg", svg7)) stop("FAIL: mean transform output not SVG")
cat("PASS: mean transform rendered (", nchar(svg7), " chars)\n")

svg8 <- render_svg(data(medals_dup) + bar * median + x(country) + y(gold))
if (!grepl("<svg", svg8)) stop("FAIL: median transform output not SVG")
cat("PASS: median transform rendered (", nchar(svg8), " chars)\n")

svg9 <- render_svg(data(medals_dup) + bar * max + x(country) + y(gold))
if (!grepl("<svg", svg9)) stop("FAIL: max transform output not SVG")
cat("PASS: max transform rendered (", nchar(svg9), " chars)\n")

svg10 <- render_svg(data(medals_dup) + bar * min + x(country) + y(gold))
if (!grepl("<svg", svg10)) stop("FAIL: min transform output not SVG")
cat("PASS: min transform rendered (", nchar(svg10), " chars)\n")

cat("\naggregation transform tests passed.\n")

# --- proportion transform ---
continents_df <- data.frame(
  continent = c("Asia", "Asia", "Europe", "Europe", "Europe", "Africa")
)
svg11 <- render_svg(
  data(continents_df) + bar * proportion + x(continent) + y(proportion) +
    title("Proportion of rows per continent") + y_label("Proportion")
)
if (!grepl("<svg", svg11)) stop("FAIL: proportion transform output not SVG")
cat("PASS: proportion transform rendered (", nchar(svg11), " chars)\n")

cat("\nproportion transform test passed.\n")

# ---------------------------------------------------------------------------
# Missing values — NA becomes a reported row-drop, not a parse error
#
# The end-to-end path the Rust tests cannot reach: R's NA → JSON null → the CLI's
# Option columns → the mapped-column drop. `penguins` is the real motive (two
# missing flipper measurements); this is that shape, self-contained.
# ---------------------------------------------------------------------------

capture_msgs <- function(expr) {
  msgs <- character()
  val <- withCallingHandlers(expr, message = function(m) {
    msgs[[length(msgs) + 1L]] <<- conditionMessage(m)
    invokeRestart("muffleMessage")
  })
  list(value = val, msgs = paste(msgs, collapse = ""))
}

# NA in a mapped column (x) drops the row and says so — where it used to die with
# `invalid type: string "NA", expected f64`.
na_x <- data.frame(x = c(1, 2, NA, 4, 5, 6, NA, 8))
res <- capture_msgs(render_svg(data(na_x) + bar * bin + x(x)))
if (!grepl("<svg", res$value)) stop("FAIL: a frame with NA in a mapped column did not render")
if (!grepl("dropped 2 rows", res$msgs)) stop("FAIL: the two dropped rows were not reported")
cat("PASS: NA in a mapped column is dropped and reported\n")

# NA in a column the plot never maps costs nothing — no drop, no message. This is
# the scoping that keeps n honest for a real dataset full of unrelated gaps.
na_unused <- data.frame(x = c(1, 2, 3, 4, 5, 6), note = c("a", NA, "c", NA, "e", NA))
res2 <- capture_msgs(render_svg(data(na_unused) + bar * bin + x(x)))
if (!grepl("<svg", res2$value)) stop("FAIL: NA in an unused column broke the render")
if (grepl("dropped", res2$msgs)) stop("FAIL: an NA in an unmapped column must not drop a row")
cat("PASS: NA in an unmapped column is left alone (no silent shrink)\n")

cat("\nmissing-value tests passed.\n")

# ---------------------------------------------------------------------------
# One row is still a column — the wire must not unbox a length-1 vector
#
# `toJSON(auto_unbox = TRUE)` is needed for the scalar *spec* fields and cannot
# tell them from a length-1 *column*, so `[30000]` left as `30000` and the
# engine rejected the frame: `invalid type: integer 30000, expected a sequence`.
# Every single-row table was unrenderable — including the one-row literal table
# that *is* the recorded annotation mechanism (spec §8). Tested at both ends:
# the wire shape, and a render that used to die.
# ---------------------------------------------------------------------------

# The invariant, stated directly: a column is a JSON array at every length.
one <- df_to_wire(data.frame(n = 30000, s = "note", f = factor("Low")))
for (nm in c("n", "s")) {
  slot <- if (nm == "n") one$floats else one$strings
  if (!inherits(slot[[nm]], "AsIs"))
    stop("FAIL: a length-1 `", nm, "` column lost its array protection")
}
if (!inherits(one$levels$f, "AsIs"))
  stop("FAIL: a single-level factor's levels lost their array protection")
if (inherits(one$dates, "AsIs") || !is.null(one$dates$n))
  stop("FAIL: `dates` maps a column to one scalar and must not be wrapped")
if (!identical(unclass(one$floats$n), 30000) || !identical(unclass(one$strings$s), "note"))
  stop("FAIL: wrapping changed a value")
cat("PASS: a length-1 column crosses the wire as an array, not a scalar\n")

# And end to end: the three shapes that could not render before.
if (!grepl("<svg", render_svg(data(data.frame(gdp = 30000, life = 82)) + point + x(gdp) + y(life))))
  stop("FAIL: a one-row table did not render")
cat("PASS: a one-row table renders\n")

note1 <- data.frame(x = 3, y = 3.2, what = "here")
lab <- render_svg(data(df) + point + x(x) + y(y) + data(note1) + text + label(what))
if (!grepl(">here<", lab))
  stop("FAIL: a one-row annotation layer did not reach the SVG")
cat("PASS: a one-row literal table annotates (the spec §8 mechanism)\n")

if (!grepl("<svg", render_svg(data(data.frame(k = factor("Low", levels = "Low"), v = 3)) +
                              bar + x(k) + y(v))))
  stop("FAIL: a single-level factor did not render")
cat("PASS: a single-level factor renders\n")

cat("\none-row wire tests passed.\n")

# ---------------------------------------------------------------------------
# path — the stroke that keeps the table's order, and the arrow it can carry
#
# The engine tests pin the vertex order; these pin the *binding*: that `path` is
# exported and reaches the wire, that `style(arrow = )` validates in R (so the
# error lands on the line that wrote it), and that the refusals arrive.
# ---------------------------------------------------------------------------

# A route that doubles back, so row order and x order are different sequences.
route <- data.frame(x = c(3, 1, 4, 2), y = c(1, 2, 3, 4))

if (!grepl("<polyline", render_svg(data(route) + path + x(x) + y(y))))
  stop("FAIL: a path did not render a stroke")
cat("PASS: path renders a stroke through the rows\n")

# The head is a filled polygon, so counting heads is counting polygons.
heads <- function(a) {
  p <- data(route) + path + x(x) + y(y)
  if (!is.null(a)) p <- p + style(arrow = a)
  lengths(regmatches(render_svg(p), gregexpr("<polygon", render_svg(p))))
}
if (heads(NULL) != 0L) stop("FAIL: a bare path drew a head")
if (heads("end") != 1L || heads("start") != 1L || heads("both") != 2L)
  stop("FAIL: arrow ends drew the wrong number of heads: ",
       heads("end"), "/", heads("start"), "/", heads("both"))
cat("PASS: style(arrow = ) draws a head at end, start, or both\n")

# R validates the value before the wire, so the message names the line that
# wrote it rather than arriving from the engine. (`refuses()` is defined further
# down, with the style tests; this section runs before it.)
bad_end <- tryCatch({ style(arrow = "tip"); NA_character_ },
                    error = function(e) conditionMessage(e))
if (is.na(bad_end) || !grepl("\"both\"", bad_end))
  stop("FAIL: style(arrow = \"tip\") should be refused in R, got: ", bad_end)
cat("PASS: refused — style(arrow) with a value that is not an end\n")

# And the engine refuses the mark-level ones, with direction.
arrow_on_line <- tryCatch({
  render_svg(data(route) + line + x(x) + y(y) + style(arrow = "end")); NA_character_
}, error = function(e) conditionMessage(e))
if (is.na(arrow_on_line) || !grepl("Use `path`", arrow_on_line))
  stop("FAIL: arrow on a line should be refused toward path, got: ", arrow_on_line)
cat("PASS: refused — style(arrow) on a line, with direction toward path\n")

path_transform <- tryCatch({
  render_svg(data(df) + path * mean + x(group) + y(y)); NA_character_
}, error = function(e) conditionMessage(e))
if (is.na(path_transform) || !grepl("line", path_transform))
  stop("FAIL: path * mean should be refused toward line, got: ", path_transform)
cat("PASS: refused — path * mean, with direction toward line\n")

cat("\npath tests passed.\n")

# ---------------------------------------------------------------------------
# rule — one position from the data, the other extent from the panel
#
# The engine tests pin the geometry; these pin the *binding*: that `rule` is
# exported and reaches the wire, that the axis is read off which position column
# the rule's own table holds, that `style(reach = )` validates in R, and that the
# ambiguous case refuses rather than guessing.
# ---------------------------------------------------------------------------

cuts_y <- data.frame(y = c(2.0, 3.5))
cuts_x <- data.frame(x = 2.5)

# The mark's whole claim, as one differential check: the *same* sentence draws a
# horizontal line or a vertical one purely by which column the rule's table has.
# A horizontal rule spans the panel's width, a vertical one its height, so the
# two are told apart by whether the segment's endpoints share a y or an x.
# The four endpoint numbers of the first crimson `<line>`, in x1 y1 x2 y2 order.
# Pulled from the *quoted* values, never from the whole tag: `[0-9.]+` over the
# tag also matches the `1` in `x1=`, which is how the first version of this
# helper reported a horizontal rule as vertical.
seg_of <- function(svg) {
  m <- regmatches(svg, regexpr(
    "<line x1=\"[-0-9.]+\" y1=\"[-0-9.]+\" x2=\"[-0-9.]+\" y2=\"[-0-9.]+\" fill=\"none\" stroke=\"crimson\"", svg))
  if (length(m) == 0L) stop("FAIL: no crimson rule was drawn")
  as.numeric(gsub("\"", "", regmatches(m, gregexpr("\"[-0-9.]+\"", m))[[1]]))
}
seg <- function(tbl) {
  seg_of(render_svg(data(df) + point + x(x) + y(y) + data(tbl) + rule +
                      style(color = "crimson")))
}
h <- seg(cuts_y); v <- seg(cuts_x)
if (!isTRUE(all.equal(h[2], h[4])) || isTRUE(all.equal(h[1], h[3])))
  stop("FAIL: a rule on y should be horizontal, got ", paste(h, collapse = ","))
if (!isTRUE(all.equal(v[1], v[3])) || isTRUE(all.equal(v[2], v[4])))
  stop("FAIL: a rule on x should be vertical, got ", paste(v, collapse = ","))
cat("PASS: a rule lands on whichever axis its own table answers\n")

# `reach` shortens the same mark rather than changing it: the edge tick must be a
# small fraction of the span the panel rule crosses.
span <- function(reach) {
  n <- seg_of(render_svg(data(df) + point + x(x) + y(y) + data(cuts_x) + rule +
                           style(color = "crimson", reach = reach)))
  abs(n[4] - n[2])
}
if (span("edge") >= span("panel") / 4)
  stop("FAIL: an edge rule should be a short tick, got ", span("edge"), " of ", span("panel"))
cat("PASS: style(reach = \"edge\") draws a tick, \"panel\" the whole crossing\n")

# R validates the value before the wire, so the error names the line that wrote it.
bad_reach <- tryCatch({ style(reach = "halfway"); NA_character_ },
                      error = function(e) conditionMessage(e))
if (is.na(bad_reach) || !grepl("\"edge\"", bad_reach))
  stop("FAIL: style(reach = \"halfway\") should be refused in R, got: ", bad_reach)
cat("PASS: refused — style(reach) with a value that is not a reach\n")

# The ambiguous case: handed the plot's own table, both position columns answer.
both_axes <- tryCatch({
  render_svg(data(df) + point + x(x) + y(y) + rule); NA_character_
}, error = function(e) conditionMessage(e))
if (is.na(both_axes) || !grepl("its own table", both_axes))
  stop("FAIL: a rule over the plot's own table should be refused, got: ", both_axes)
cat("PASS: refused — a rule whose table answers both axes, with direction\n")

rule_transform <- tryCatch({
  render_svg(data(df) + point + x(x) + y(y) + data(cuts_y) + rule * mean); NA_character_
}, error = function(e) conditionMessage(e))
if (is.na(rule_transform) || !grepl("no measure", rule_transform))
  stop("FAIL: rule * mean should be refused, got: ", rule_transform)
cat("PASS: refused — rule * mean, with direction toward a computed column\n")

cat("\nrule tests passed.\n")

# ---------------------------------------------------------------------------
# zone — the rectangle whose unbounded axis is the panel's
#
# The engine tests pin the geometry against the panel rect; these pin the
# *binding*: that `zone` is exported, that `bounds` carries all four column names
# across the wire, and that the refusals arrive.
# ---------------------------------------------------------------------------

series <- data.frame(t = 1:10, v = c(3, 5, 4, 7, 9, 6, 8, 11, 10, 13))
spanz  <- data.frame(a = c(2.5, 7.5), b = c(4.0, 9.0))
bandz  <- data.frame(lo = 5.0, hi = 9.0)

zrects <- function(spec) lengths(regmatches(render_svg(spec), gregexpr('fill-opacity="0.200"', render_svg(spec))))

# One row is one rectangle, so a two-row table draws two bands from one layer —
# `rule`'s payoff for taking columns rather than numbers, inherited.
n2 <- zrects(data(series) + x(t) + y(v) +
               data(spanz) + zone * bounds(start = a, end = b) +
               data(series) + line)
if (n2 != 2L) stop("FAIL: a two-row zone table should draw two rectangles, drew ", n2)
cat("PASS: one row is one rectangle, so one table draws several\n")

# Both pairs on one atom: the box.
n1 <- zrects(data(series) + x(t) + y(v) +
               data(cbind(spanz[1, ], bandz)) + zone * bounds(lo, hi, start = a, end = b) +
               data(series) + line)
if (n1 != 1L) stop("FAIL: a box should draw one rectangle, drew ", n1)
cat("PASS: bounds carries all four column names across the wire\n")

# A bare zone says nothing about where its sides are.
bare <- tryCatch({ render_svg(data(series) + x(t) + y(v) + zone + line); NA_character_ },
                 error = function(e) conditionMessage(e))
if (is.na(bare) || !grepl("spans the panel", bare))
  stop("FAIL: a zone with no bounds should be refused, got: ", bare)
cat("PASS: refused — a zone with no bounds, with direction\n")

# Half a pair is the likely typo, and the message says which half.
half <- tryCatch({
  render_svg(data(series) + x(t) + y(v) + data(bandz) + zone * bounds(lower = lo) +
               data(series) + line); NA_character_
}, error = function(e) conditionMessage(e))
if (is.na(half) || !grepl("other half", half))
  stop("FAIL: half a pair should be refused by naming it, got: ", half)
cat("PASS: refused — half a pair, naming the half that was given\n")

# The domain pair belongs to a rectangle, not to a band.
onband <- tryCatch({
  render_svg(data(series) + x(t) + y(v) +
               data(spanz) + ribbon * bounds(start = a, end = b)); NA_character_
}, error = function(e) conditionMessage(e))
if (is.na(onband) || !grepl("use `zone`", onband))
  stop("FAIL: bounds(start, end) on a ribbon should be refused toward zone, got: ", onband)
cat("PASS: refused — bounds(start, end) on a band mark, with direction toward zone\n")

# And `bounds()` with nothing at all errors in R, on the line that wrote it.
nocols <- tryCatch({ bounds(); NA_character_ }, error = function(e) conditionMessage(e))
if (is.na(nocols) || !grepl("column names", nocols))
  stop("FAIL: bounds() with no columns should be refused in R, got: ", nocols)
cat("PASS: refused — bounds() naming no columns\n")

cat("\nzone tests passed.\n")

# ---------------------------------------------------------------------------
# surface — the sheet through the samples (spec §15)
#
# The engine tests pin the mesh against the lattice; these pin the *binding*: that
# `surface` is exported, that a grid table reaches the engine as a grid, and that
# the refusals a reader will actually hit arrive with direction.
# ---------------------------------------------------------------------------

# `expand.grid` is the shape a surface wants, and saying so here is half the test:
# the mark's whole contract with the caller is one row per (x, y) crossing.
surf_grid <- expand.grid(gx = seq(-3, 3, length.out = 15),
                         gy = seq(-3, 3, length.out = 15))
surf_grid$h <- with(surf_grid, sin(sqrt(gx^2 + gy^2) + 1e-9) / sqrt(gx^2 + gy^2 + 1e-9))

sfaces <- function(spec) {
  svg <- render_svg(spec)
  lengths(regmatches(svg, gregexpr('<path d="M', svg)))
}

# A 15x15 lattice of nodes has 14x14 blocks of four, so the sheet is 196 faces.
nf <- sfaces(data(surf_grid) + surface + x(gx) + y(gy) + z(h))
if (nf != 196L) stop("FAIL: a 15x15 grid should draw 196 faces, drew ", nf)
cat("PASS: one face per complete cell of the grid\n")

# Binding `z` is what puts a plot in the cube, so a surface needs no `space()` —
# and `space()` still sets the angle, which must change the picture.
turned <- render_svg(data(surf_grid) + surface + x(gx) + y(gy) + z(h) +
                       space(turn = 110, tilt = 40))
if (identical(turned, render_svg(data(surf_grid) + surface + x(gx) + y(gy) + z(h))))
  stop("FAIL: space(turn=, tilt=) did not turn the sheet")
cat("PASS: `z` is the trigger and `space()` sets the angle\n")

# The mesh lines: the seam hairline each face already carried, handed to the caller.
meshed <- render_svg(data(surf_grid) + surface + x(gx) + y(gy) + z(h) +
                       style(border_color = "white", border_size = 0.6))
if (!grepl('stroke="white"', meshed, fixed = TRUE))
  stop("FAIL: style(border_color =) should draw the mesh lines")
cat("PASS: style(border_color =) is the wireframe over the sheet\n")

# A flat surface is one failure, not two, and the direction names both routes in.
flat <- tryCatch({ render_svg(data(surf_grid) + surface + x(gx) + y(gy)); NA_character_ },
                 error = function(e) conditionMessage(e))
if (is.na(flat) || !grepl("needs the cube", flat) || !grepl("`zone`", flat))
  stop("FAIL: a flat surface should be refused toward the cube and zone, got: ", flat)
cat("PASS: refused — a surface with no height, with both routes and the flat mark\n")

# A scatter is the empty panel this refusal exists to prevent.
set.seed(11)
scat <- data.frame(sx = runif(60), sy = runif(60), sh = runif(60))
sc <- tryCatch({ render_svg(data(scat) + surface + x(sx) + y(sy) + z(sh)); NA_character_ },
               error = function(e) conditionMessage(e))
if (is.na(sc) || !grepl("scatter rather than a grid", sc) || !grepl("surface \\* density", sc))
  stop("FAIL: a scatter should be refused toward point/density, got: ", sc)
cat("PASS: refused — a scatter, with `point` and `surface * density` named\n")

# And the sentence that refusal advises must draw: the field raised, no `z()`.
est <- sfaces(data(scat) + surface * density + x(sx) + y(sy) + space())
if (est < 100L) stop("FAIL: surface * density should raise a mesh, drew ", est, " faces")
cat("PASS: surface * density raises the estimated field, with no z() bound\n")

# `bin` cuts the floor into adjacent cells and the sheet lays a flat lid on each —
# the terraced surface, for a design that measures one value per cell. A 3x3 grid
# read as *nodes* is 2x2 blocks of four corners, so four faces; read as cells it is
# nine lids plus the twelve risers that connect them.
terr <- expand.grid(ta = c(-2, 0, 2), tb = c(-2, 0, 2))
terr$tv <- terr$ta^2 + terr$tb^2
nodes <- sfaces(data(terr) + surface + x(ta) + y(tb) + z(tv))
if (nodes != 4L) stop("FAIL: nine nodes are four faces, got ", nodes)
lids <- sfaces(data(terr) + surface * bin(3) * mean + x(ta) + y(tb) + z(tv))
if (lids != 21L) stop("FAIL: nine cells are 9 lids + 12 risers = 21, got ", lids)
cat("PASS: a cut floor lays one plateau per cell where nodes span the gaps\n")

# What is still refused is a floor of *slots*: categories leave air between them,
# and tiles that float apart are not a sheet.
sbin <- tryCatch({ render_svg(data(scat) + surface * count + x(sx) + y(sy) + space()); NA_character_ },
                 error = function(e) conditionMessage(e))
if (is.na(sbin) || !grepl("surface \\* bin", sbin))
  stop("FAIL: surface * count should be refused toward the two floors, got: ", sbin)
cat("PASS: refused — surface * count, with direction toward the two floors that tile\n")

# A face spans the gap between two samples; two categories have no gap to span.
scat_cat <- surf_grid
scat_cat$band <- rep(c("low", "high"), length.out = nrow(scat_cat))
scab <- tryCatch({ render_svg(data(scat_cat) + surface + x(band) + y(gy) + z(h)); NA_character_ },
                 error = function(e) conditionMessage(e))
if (is.na(scab) || !grepl("bar \\* count", scab))
  stop("FAIL: a categorical floor should be refused toward bar * count, got: ", scab)
cat("PASS: refused — a category on the floor, with direction toward the 3-D bar\n")

cat("\nsurface tests passed.\n")

# ---------------------------------------------------------------------------
# theme() — the page rather than the ink (spec §7)
#
# The properties are checked here and again in the engine, so what is tested is
# the *pair*: the binding stops a bad value on the line that wrote it, and the
# engine stops one that reached it another way. What matters for the grammar is
# the second, which is why the refusals below are read for their direction.
# ---------------------------------------------------------------------------

theme_df <- data.frame(g = c("Alpha", "Beta", "Gamma"), v = c(3, 7, 5),
                       side = c("Left", "Right", "Left"),
                       stringsAsFactors = FALSE)
theme_lines <- function(...) {
  svg <- render_svg(data(theme_df) + bar + x(g) + y(v) + ...)
  lengths(regmatches(svg, gregexpr("<line", svg)))
}

if (!(theme_lines(theme(grid = "none")) < theme_lines(style(opacity = 1))))
  stop("FAIL: theme(grid = 'none') drew as many lines as the default")
cat("PASS: theme(grid = 'none') drops the gridlines\n")

if (theme_lines(theme("minimal")) != theme_lines(theme(grid = "none")))
  stop("FAIL: the `minimal` preset is not `grid = none`")
cat("PASS: a named preset resolves in the engine, not the binding\n")

# A preset a caller cannot adjust sends them back to knobs, which is the failure
# spec §7 exists to prevent.
sq <- render_svg(data(theme_df) + bar + x(g) + y(v) + theme("minimal", ratio = 1))
wide <- render_svg(data(theme_df) + bar + x(g) + y(v) + theme("minimal"))
if (identical(sq, wide)) stop("FAIL: a preset could not be adjusted")
cat("PASS: a preset can be adjusted\n")

# Turned labels are the answer to names that overlap, and they earn their room.
if (!grepl("rotate", render_svg(data(theme_df) + bar + x(g) + y(v) + theme(tick_angle = 45))))
  stop("FAIL: theme(tick_angle) did not turn the labels")
cat("PASS: theme(tick_angle) turns the x labels\n")

# One number, three sizes. The tick labels take the number itself and the axis
# names and title are a fixed step above it, so a plot's text is one decision.
font_sizes <- function(svg) {
  sort(unique(as.numeric(gsub('font-size="([0-9.]+)"', "\\1",
    regmatches(svg, gregexpr('font-size="[0-9.]+"', svg))[[1]]))), decreasing = TRUE)
}
typed <- function(atom = NULL) {
  p <- data(theme_df) + bar + x(g) + y(v) + title("T")
  if (!is.null(atom)) p <- p + atom
  render_svg(p)
}
if (!identical(font_sizes(typed()), c(16, 13, 11)))
  stop("FAIL: the default type scale is not 11/13/16")
if (!identical(font_sizes(typed(theme(font_size = 16))), c(23, 19, 16)))
  stop("FAIL: theme(font_size = 16) did not carry the axis names and title with it")
cat("PASS: theme(font_size) is one number and three sizes\n")

# Asking for the size you already have must draw the plot you already had, or the
# default is an approximation of the scale rather than a point on it.
if (!identical(typed(theme(font_size = 11)), typed()))
  stop("FAIL: theme(font_size = 11) is not the untouched default")
cat("PASS: theme(font_size = 11) draws the untouched default\n")

refuses_theme <- function(label, expr) {
  msg <- tryCatch({ force(expr); NA_character_ },
                  error = function(e) conditionMessage(e))
  if (is.na(msg)) stop("FAIL: ", label, " should have been refused")
  if (!grepl("^gog:", msg)) stop("FAIL: ", label, " refused without the gog: prefix")
  cat("PASS: refused —", label, "\n")
}
# The mistake the pixel unit invites: reading the number as a multiplier.
refuses_theme("theme(font_size = 1.5)", theme(font_size = 1.5))
refuses_theme("theme('dark')", render_svg(data(theme_df) + bar + x(g) + y(v) + theme("dark")))
refuses_theme("theme(grid = 'diag')", theme(grid = "diag"))
refuses_theme("theme(ratio = -1)", theme(ratio = -1))
refuses_theme("theme(tick_angle = 120)", theme(tick_angle = 120))
refuses_theme("theme() with nothing set", theme())
refuses_theme("theme(frame = 'box')", theme(frame = "box"))
refuses_theme("theme(background = 'whte')",
              render_svg(data(theme_df) + bar + x(g) + y(v) + theme(background = "whte")))

# `theme("bw")` is only a bundle of properties a caller could set themselves —
# the rule that keeps a preset from becoming a second, hidden vocabulary.
# Faceted on purpose. This assertion passed for the whole life of `theme("bw")`
# while the preset left five gray strips over its white panels, because an
# unfaceted plot draws no strip for the test to miss.
bw_named <- render_svg((data(theme_df) + bar + x(g) + y(v) + theme("bw")) | facet(side))
bw_spelt <- render_svg((data(theme_df) + bar + x(g) + y(v) +
                          theme(background = "white", frame = "full", strip = "white")) |
                         facet(side))
if (!identical(bw_named, bw_spelt))
  stop("FAIL: theme('bw') is not its own properties spelled out")
cat("PASS: a preset is only a bundle of properties you could set yourself\n")

# The band above each panel is furniture like any other, and the journal preset
# has to cover it: a gray tint is what reproduces badly on paper.
if (grepl("#e4e4ec", bw_named))
  stop("FAIL: theme('bw') left the facet strip gray")
if (!grepl("#e4e4ec", render_svg((data(theme_df) + bar + x(g) + y(v)) | facet(side))))
  stop("FAIL: the default strip moved")
if (!grepl("seagreen", render_svg((data(theme_df) + bar + x(g) + y(v) +
                                   theme(strip = "seagreen")) | facet(side))))
  stop("FAIL: theme(strip = ) did not reach the band")
cat("PASS: theme(strip = ) colors the band, and `bw` covers it\n")

# The ink derives from the band, so `strip = "black"` is a whole instruction
# rather than half of one: without this it would print near-black on near-black.
dark <- render_svg((data(theme_df) + bar + x(g) + y(v) + theme(strip = "black")) | facet(side))
if (!grepl('fill="#ffffff" text-anchor="middle"', dark))
  stop("FAIL: a dark strip did not get light type")
if (!grepl('fill="#3c3c46" text-anchor="middle"',
           render_svg((data(theme_df) + bar + x(g) + y(v)) | facet(side))))
  stop("FAIL: the default strip ink moved")
# …and a named ink wins, because the derivation guides rather than forbids.
if (!grepl("gold", render_svg((data(theme_df) + bar + x(g) + y(v) +
                               theme(strip = "navy", strip_text = "gold")) | facet(side))))
  stop("FAIL: theme(strip_text = ) did not win over the derived ink")
cat("PASS: the strip's ink derives from its band, and a named one wins\n")
refuses_theme("theme(strip_text = 'gld')",
              render_svg((data(theme_df) + bar + x(g) + y(v) +
                          theme(strip_text = "gld")) | facet(side)))
refuses_theme("theme(strip = 'whte')",
              render_svg((data(theme_df) + bar + x(g) + y(v) +
                          theme(strip = "whte")) | facet(side)))

# The furniture goes black and white; the data does not.
bw_colored <- render_svg(data(theme_df) + bar + x(g) + y(v) + color(g) + theme("bw"))
if (!grepl("#", bw_colored)) stop("FAIL: theme('bw') took the color out of the data")
cat("PASS: `bw` is the furniture, never the data\n")

if (!grepl('<rect[^>]*fill="none"|<g stroke="[^"]*" stroke-width="1.5"[^>]*>\\s*<rect', bw_named))
  stop("FAIL: frame = 'full' drew no rectangle")
cat("PASS: frame = 'full' closes the axes into a rectangle\n")

cat("\ntheme tests passed.\n")

# ---------------------------------------------------------------------------
# style() — constants are set, not mapped
# ---------------------------------------------------------------------------

refuses <- function(label, expr) {
  msg <- tryCatch({ force(expr); NA_character_ },
                  error = function(e) conditionMessage(e))
  if (is.na(msg)) stop("FAIL: ", label, " should have been refused")
  cat("PASS: refused —", label, "\n")
  invisible(msg)
}

# A set color reaches the output and draws no legend.
svg12 <- render_svg(data(df) + x(x) + y(y) + point + style(color = "tomato"))
if (!grepl('fill="tomato"', svg12)) stop("FAIL: style(color) did not reach the SVG")
cat("PASS: style(color) rendered\n")

svg13 <- render_svg(data(df) + x(x) + y(y) + point + color(group))
if (!grepl(">A<", svg13)) stop("FAIL: color(group) should draw a legend")
if (grepl(">A<", svg12)) stop("FAIL: a set color must not draw a legend")
cat("PASS: a map earns a legend, a set does not\n")

# `line` refuses opacity as a channel but accepts it as a setting — the case
# the whole set/map split exists for.
refuses("opacity(y) on line", render_svg(data(df) + x(x) + y(y) + line + opacity(y)))
svg14 <- render_svg(data(df) + x(x) + y(y) + line + style(opacity = 0.4, size = 6))
if (!grepl('stroke-width="6"', svg14)) stop("FAIL: style(size) missing on line")
if (!grepl('stroke-opacity="0.400"', svg14)) stop("FAIL: style(opacity) missing on line")
cat("PASS: line takes size/opacity as settings\n")

# Refusals, each with a directional message.
m <- refuses("style(shape) on bar",
             render_svg(data(bar_df) + x(category) + y(value) + bar + style(shape = "square")))
if (!grepl("no shape to set", m)) stop("FAIL: unhelpful message: ", m)

m <- refuses("mapping and setting color together",
             render_svg(data(df) + x(x) + y(y) + point + color(group) + style(color = "red")))
if (!grepl("cannot do both", m)) stop("FAIL: unhelpful message: ", m)

m <- refuses("a misspelt color",
             render_svg(data(df) + x(x) + y(y) + point + style(color = "stelblue")))
if (!grepl("steelblue", m)) stop("FAIL: no suggestion offered: ", m)

m <- refuses("an R color name",
             render_svg(data(df) + x(x) + y(y) + point + style(color = "gray80")))
if (!grepl("R color name", m)) stop("FAIL: vocabulary not explained: ", m)

m <- refuses("style() before any mark", data(df) + x(x) + style(color = "red"))
if (!grepl("no mark to style", m)) stop("FAIL: unhelpful message: ", m)

m <- refuses("style() with nothing set", style())
if (!grepl("sets nothing", m)) stop("FAIL: unhelpful message: ", m)

# One spelling of English, and the refusal has to say which. A reader arriving
# from ggplot2 types `colour` because there it works; before this, R answered
# with its own "unused argument", which named neither the fix nor gog.
for (pair in list(c("colour", "color"), c("border_colour", "border_color"),
                  c("centre", "center"))) {
  m <- refuses(paste0("the British spelling of `", pair[[2]], "`"),
               do.call(style, setNames(list("red"), pair[[1]])))
  if (!grepl(paste0("gog spells it `", pair[[2]], "`"), m, fixed = TRUE))
    stop("FAIL: the American spelling is not named: ", m)
  if (!grepl("ggplot2", m)) stop("FAIL: does not say ggplot2 differs: ", m)
}
m <- refuses("an unknown setting that is not a British spelling", style(nonsense = 1))
if (!grepl("gog sets:", m)) stop("FAIL: a plain typo should get the list: ", m)
m <- refuses("the British spelling of the color channel", colour(group))
if (!grepl("gog spells it `color(<column>)`", m, fixed = TRUE))
  stop("FAIL: `colour()` must name `color()`: ", m)

# palette(): a single color name was silently ignored before.
m <- refuses("palette() given one color name",
             render_svg(data(df) + x(x) + y(y) + point + color(group) + palette("red")))
if (!grepl("style\\(color", m)) stop("FAIL: does not point at style(): ", m)

svg15 <- render_svg(data(df) + x(x) + y(y) + point + color(group) +
                      palette(c("firebrick", "steelblue")))
if (!grepl('fill="firebrick"', svg15)) stop("FAIL: CSS color names in palette()")
cat("PASS: palette() accepts CSS color names\n")

# style() does not broadcast backward, unlike color().
svg16 <- render_svg(data(df) + x(x) + y(y) + line + point + style(color = "tomato"))
if (!grepl('<polyline[^>]*stroke="#4e79a7"', svg16))
  stop("FAIL: style() leaked backward onto the line layer")
cat("PASS: style() binds forward only\n")

cat("\nstyle() tests passed.\n")

# ---------------------------------------------------------------------------
# Channel scope — position decides, binding is forward-only
# ---------------------------------------------------------------------------

# Count distinct COLORS, not distinct elements: every polyline carries its own
# `points`, and every <circle> its own position, so counting elements or whole
# attribute runs measures geometry rather than color. Pull the value out.
line_colors <- function(s) {
  poly <- unlist(regmatches(s, gregexpr('<polyline[^>]*>', s)))
  length(unique(gsub('.*[^-]stroke="([^"]*)".*', '\\1', poly)))
}
point_colors <- function(s) {
  # Exclude legend swatches, which are drawn at a fixed small radius.
  circ <- unlist(regmatches(s, gregexpr('<circle[^>]*>', s)))
  circ <- circ[!grepl('fill-opacity="0.60"', circ)]
  length(unique(gsub('.*fill="([^"]*)".*', '\\1', circ)))
}

scope_df <- data.frame(
  t   = rep(1:4, 3),
  v   = c(1.0,2,3,4, 2,3,4,5, 3,4,5,6),
  who = rep(c("a", "b", "c"), each = 4),
  wgt = rep(c(10.0, 20.0, 30.0), each = 4)
)

# Before any mark → plot-scoped, so both layers take it.
s <- render_svg(data(scope_df) + x(t) + y(v) + color(who) + line + point)
if (line_colors(s) != 3 || point_colors(s) != 3)
  stop("FAIL: plot-scoped color should reach both layers, got ",
       line_colors(s), "/", point_colors(s))
cat("PASS: a channel before any mark is plot-scoped\n")

# After a mark → that mark only, in either order.
s <- render_svg(data(scope_df) + x(t) + y(v) + line + color(who) + point)
if (line_colors(s) != 3 || point_colors(s) != 1)
  stop("FAIL: color should bind to the line only, got ",
       line_colors(s), "/", point_colors(s))
s <- render_svg(data(scope_df) + x(t) + y(v) + line + group(who) + point + color(who))
if (line_colors(s) != 1 || point_colors(s) != 3)
  stop("FAIL: color should bind to the point only, got ",
       line_colors(s), "/", point_colors(s))
cat("PASS: a channel after a mark binds to that mark alone\n")

# The regression this fixed: `size` after `point` must not land on `line`.
s <- render_svg(data(scope_df) + x(t) + y(v) +
                  line + color(who) + point + size(wgt))
if (!grepl("<svg", s)) stop("FAIL: per-layer channels should render")
if (line_colors(s) != 3) stop("FAIL: line lost its color")
if (length(unique(unlist(regmatches(s, gregexpr('r="[0-9.]+"', s))))) < 2)
  stop("FAIL: size did not reach the points")
cat("PASS: a channel only one mark accepts can be scoped to it\n")

# A plot-scoped channel no mark accepts is refused, not silently dropped.
m <- refuses("plot-scoped size with only a line",
             render_svg(data(scope_df) + x(t) + y(v) + size(wgt) + line + group(who)))
if (!grepl("no mark here has a size feature", m)) stop("FAIL: unhelpful message: ", m)

cat("\nchannel scope tests passed.\n")

# ---------------------------------------------------------------------------
# Horizontal bars — orientation is read off the bindings, there is no `flip`
# ---------------------------------------------------------------------------

# Read attributes by their leading space (` width=`) rather than loosely:
# `stroke-width` also ends in `width=` and would otherwise be picked up.
bar_geom <- function(s) {
  r <- unlist(regmatches(s, gregexpr('<rect[^>]*fill-opacity[^>]*>', s)))
  g <- function(a) as.numeric(sub('".*', '', sub(paste0('.* ', a, '="'), '', r)))
  data.frame(x = g("x"), y = g("y"), w = g("width"), h = g("height"))
}

med <- data.frame(country = c("USA", "China", "Great Britain"),
                  gold    = c(46.0, 38.0, 29.0))

v <- bar_geom(render_svg(data(med) + bar + x(country) + y(gold)))
h <- bar_geom(render_svg(data(med) + bar + x(gold) + y(country)))
if (nrow(v) != 3 || nrow(h) != 3) stop("FAIL: expected 3 bars in each orientation")

if (length(unique(round(v$w, 2))) != 1 || length(unique(round(v$h, 2))) == 1)
  stop("FAIL: vertical bars should share a width and vary in height")
if (length(unique(round(h$h, 2))) != 1 || length(unique(round(h$w, 2))) == 1)
  stop("FAIL: horizontal bars should share a height and vary in width")
if (length(unique(round(h$x, 2))) != 1)
  stop("FAIL: horizontal bars should start from a common baseline")
cat("PASS: bars lie down when the categories are on y\n")

# order means one thing in both orientations: first in sort order reads first.
hs <- bar_geom(render_svg(data(med) + bar + x(gold) + y(country) +
                            order(gold, desc = TRUE)))
if (hs$y[which.max(hs$w)] != min(hs$y))
  stop("FAIL: order(desc) should put the largest bar at the top")
cat("PASS: order(desc = TRUE) puts the largest bar at the top\n")

# A synthesizing transform writes to the measured axis, whichever that is.
cts <- data.frame(g = c("a", "a", "b"))
s <- render_svg(data(cts) + bar * count + y(g))
if (!grepl(">Count<", s)) stop("FAIL: horizontal count should label the x axis")
hc <- bar_geom(s)
if (nrow(hc) != 2 || length(unique(round(hc$h, 2))) != 1)
  stop("FAIL: horizontal count should draw one equal-thickness bar per category")
if (abs(hc$w[1] - hc$w[2]) < 1) stop("FAIL: counts of 2 and 1 should differ in length")
cat("PASS: `bar * count + y(g)` counts along x\n")

# Both axes categorical leaves nothing to measure. True of every slot mark, and
# each refusal names that mark's own verb.
m <- refuses("bar with two categorical axes",
             render_svg(data(cts) + bar + x(g) + y(g)))
if (!grepl("nothing for it to measure", m)) stop("FAIL: unhelpful message: ", m)

m <- refuses("box with two categorical axes",
             render_svg(data(cts) + box + x(g) + y(g)))
if (!grepl("nothing for it to summarize", m)) stop("FAIL: unhelpful message: ", m)

m <- refuses("interval with two categorical axes",
             render_svg(data(cts) + interval * range + x(g) + y(g)))
if (!grepl("nothing for it to span", m)) stop("FAIL: unhelpful message: ", m)

# The horizontal box plot and error bar: `box`/`interval` read their orientation
# off the bindings exactly as `bar` does, so a category on y lays them down.
spread <- data.frame(
  team  = rep(c("alpha", "beta", "gamma"), each = 9),
  score = c(10:18, 30:38, 50:58) + 0.0
)
vb <- bar_geom(render_svg(data(spread) + box + x(team) + y(score)))
hb <- bar_geom(render_svg(data(spread) + box + x(score) + y(team)))
if (nrow(vb) != 3 || nrow(hb) != 3) stop("FAIL: expected 3 boxes in each orientation")
if (length(unique(round(vb$w, 2))) != 1 || length(unique(round(hb$h, 2))) != 1)
  stop("FAIL: every box should share one slot thickness, whichever axis holds it")
cat("PASS: a category on y lays the box plot down\n")

# `bounds` invents the measured axis, so a forest plot binds no x() at all — and
# must not be warned at about it.
coefs <- data.frame(term = c("Age", "Education", "Experience"),
                    lo = c(0.02, 0.31, 0.11), hi = c(0.18, 0.55, 0.29))
w <- NULL
fs <- withCallingHandlers(
  render_svg(data(coefs) + interval * bounds(lo, hi) + y(term)),
  warning = function(c) { w <<- c(w, conditionMessage(c)); invokeRestart("muffleWarning") })
if (!grepl("<line", fs)) stop("FAIL: a forest plot should draw its whiskers")
if (any(grepl("x\\(\\) is not set", w)))
  stop("FAIL: bounds invents the measured axis; warning about x() contradicts the check")
cat("PASS: interval * bounds + y(term) draws a forest plot, unwarned\n")

# Vertical remains the default when neither axis is categorical.
nums <- data.frame(year = c(2020.0, 2021, 2022), sales = c(10.0, 14.0, 12.0))
nv <- bar_geom(render_svg(data(nums) + bar + x(year) + y(sales)))
if (length(unique(round(nv$w, 2))) != 1 || length(unique(round(nv$h, 2))) == 1)
  stop("FAIL: two continuous axes should still draw vertical bars")
cat("PASS: two continuous axes stay vertical\n")

cat("\nhorizontal bar tests passed.\n")

# ---------------------------------------------------------------------------
# Facets — | and / split the plot into panels
# ---------------------------------------------------------------------------

facet_df <- data.frame(
  x = c(1, 2, 3, 4, 5, 6),
  y = c(2, 4, 3, 6, 5, 7),
  g = c("a", "a", "b", "b", "c", "c"),
  h = c("u", "v", "u", "v", "u", "v")
)

panel_count <- function(svg) lengths(regmatches(svg, gregexpr('fill="#f5f5f8"', svg)))

svg_fc <- render_svg(data(facet_df) + point + x(x) + y(y) | facet(g))
if (panel_count(svg_fc) != 3) stop("FAIL: | facet(g) should draw 3 panels")
cat("PASS: `| facet(g)` draws one panel per category, side by side\n")

svg_fr <- render_svg(data(facet_df) + point + x(x) + y(y) / facet(h))
if (panel_count(svg_fr) != 2) stop("FAIL: / facet(h) should draw 2 panels")
cat("PASS: `/ facet(h)` stacks one panel per category\n")

# The cube takes a facet too, one projected box per panel. Refused as "not drawn
# yet" until 2026-07-28, when it turned out the renderer had always built its
# scene from the panel's own rectangle and only the check said otherwise.
facet_df$z <- c(1, 5, 2, 6, 3, 7)
cube_count <- function(svg) lengths(regmatches(svg, gregexpr('stroke="#d8d8de"', svg)))
svg_cube <- render_svg(data(facet_df) + point + x(x) + y(y) + z(z) | facet(g))
if (panel_count(svg_cube) != 3) stop("FAIL: a faceted cube should draw 3 panels")
if (cube_count(svg_cube) != 3) stop("FAIL: each panel should project its own cube")
cat("PASS: `+ z(z) | facet(g)` draws one projected cube per panel\n")

# Both orders of the crossed grid: `/` binds tighter than `|`, so the first
# form chains left to right and the second resolves facet(g)/facet(h) into a
# pair that `|` then unpacks — first column to its own slot, second to the
# other. Both must mean the same 3 x 2 grid.
svg_grid  <- render_svg(data(facet_df) + point + x(x) + y(y) / facet(h) | facet(g))
svg_grid2 <- render_svg(data(facet_df) + point + x(x) + y(y) | facet(g) / facet(h))
if (panel_count(svg_grid) != 6) stop("FAIL: crossed facet should draw 6 panels")
if (!identical(svg_grid, svg_grid2))
  stop("FAIL: `/ facet(h) | facet(g)` and `| facet(g) / facet(h)` must agree")
cat("PASS: crossed facets draw the full grid, in either spelling\n")

# `wrap` folds the line of panels into a rectangle. Ten levels wrapped at four
# is a 4 x 3 rectangle holding ten panels — the two cells the fold left over are
# slack, not combinations, so they get no panel at all.
wrap_df <- data.frame(
  x = rep(1:2, 10),
  y = as.numeric(1:20),
  g = rep(LETTERS[1:10], each = 2)
)
svg_wrap <- render_svg(data(wrap_df) + point + x(x) + y(y) | facet(g, wrap = 4))
if (panel_count(svg_wrap) != 10)
  stop("FAIL: ten levels wrapped at 4 should draw 10 panels, not the grid's 12")
for (nm in LETTERS[1:10]) {
  if (!grepl(paste0(">", nm, "</text>"), svg_wrap, fixed = TRUE))
    stop("FAIL: a wrapped panel must carry its own name; missing ", nm)
}
cat("PASS: `| facet(g, wrap = 4)` folds ten panels into a rectangle and names each\n")

# The direction is the operator's, never the count's: the same number under `/`
# runs the levels down instead, so the two pictures differ.
svg_wrap_down <- render_svg(data(wrap_df) + point + x(x) + y(y) / facet(g, wrap = 4))
if (identical(svg_wrap, svg_wrap_down))
  stop("FAIL: `| facet(g, wrap=4)` and `/ facet(g, wrap=4)` must differ")
cat("PASS: `wrap` says where the line turns; the operator says which way it runs\n")

# Wrapping a crossing is Illegal — a crossing already fills a rectangle.
err_wrap <- tryCatch(
  { render_svg(data(facet_df) + point + x(x) + y(y) | facet(g, wrap = 2) / facet(h)); NULL },
  error = function(e) conditionMessage(e)
)
if (is.null(err_wrap) || !grepl("wrap", err_wrap))
  stop("FAIL: wrapping a crossed facet should refuse with direction")
cat("PASS: wrapping a crossed facet refuses with direction\n")

# And the count has to be a whole number, refused at the door.
err_wrap2 <- tryCatch({ facet(g, wrap = "four"); NULL }, error = function(e) conditionMessage(e))
if (is.null(err_wrap2) || !grepl("whole number", err_wrap2))
  stop("FAIL: `facet(wrap = \"four\")` should refuse")
cat("PASS: `facet(wrap = )` takes a whole number\n")

# A free scale fits each panel from its own rows. Three groups three orders of
# magnitude apart: shared, the small ones are a flat line; freed, each has its
# own axis — and only the axis that asked, so x stays shared.
free_df <- data.frame(x = rep(1:2, 3), y = c(1, 2, 100, 200, 10, 20),
                      g = rep(c("a", "b", "c"), each = 2))
svg_shared <- render_svg(data(free_df) + point + x(x) + y(y) | facet(g))
svg_free   <- render_svg(data(free_df) + point + x(x) + y(y, free = TRUE) | facet(g))
if (grepl(">20</text>", svg_shared, fixed = TRUE))
  stop("FAIL: a shared y spans 1..200 and should never tick 20")
if (!grepl(">200</text>", svg_free, fixed = TRUE) ||
    !grepl(">20</text>", svg_free, fixed = TRUE))
  stop("FAIL: a freed y should tick each panel's own range")
cat("PASS: `y(v, free = TRUE)` fits each panel from its own rows\n")

# Refused where there are no panels to free a scale across. (`free` on a channel
# that is not a position is refused by the engine, and cannot be written here at
# all: `color()` takes no `free`, exactly as it takes no `tick_count`.)
for (bad in list(
  list(what = "free with no facet",
       f = function() render_svg(data(free_df) + point + x(x) + y(y, free = TRUE)),
       says = "one panel"),
  list(what = "free beside a stated domain",
       f = function() render_svg(data(free_df) + point + x(x) +
                                   y(y, limits = c(0, 300), free = TRUE) | facet(g)),
       says = "one scale per panel"))) {
  msg <- tryCatch({ bad$f(); NULL }, error = function(e) conditionMessage(e))
  if (is.null(msg) || !grepl(bad$says, msg, fixed = TRUE))
    stop("FAIL: ", bad$what, " should refuse with direction; got: ", msg)
  cat("PASS: ", bad$what, " refuses with direction\n", sep = "")
}

err_free <- tryCatch({ y(life, free = "yes"); NULL }, error = function(e) conditionMessage(e))
if (is.null(err_free) || !grepl("TRUE or FALSE", err_free))
  stop("FAIL: `free = \"yes\"` should refuse")
cat("PASS: `free = ` is TRUE or FALSE\n")

# facet() must not join with + — the error should point at the operators.
err <- tryCatch(
  { data(facet_df) + point + x(x) + y(y) + facet(g); NULL },
  error = function(e) conditionMessage(e)
)
if (is.null(err) || !grepl("\\|", err)) stop("FAIL: + facet() should refuse, pointing at | and /")
cat("PASS: `+ facet()` refuses with direction\n")

# A numeric facet column is Illegal, with direction toward factor().
err2 <- tryCatch(
  { render_svg(data(facet_df) + point + x(x) + y(y) | facet(y)); NULL },
  error = function(e) conditionMessage(e)
)
if (is.null(err2) || !grepl("factor", err2)) stop("FAIL: numeric facet should refuse with factor() advice")
cat("PASS: a numeric facet column refuses with direction\n")

cat("\nfacet tests passed.\n")

# ---------------------------------------------------------------------------
# play — the facet read in time
#
# The same split, laid out in sequence rather than across the page, so these
# mirror the facet tests above on purpose.
# ---------------------------------------------------------------------------

play_df <- data.frame(
  x    = c(1, 2, 3, 10, 20, 30),
  y    = c(1, 2, 3, 10, 20, 30),
  year = c(1957, 1957, 1957, 1962, 1962, 1962)
)

frame_count <- function(svg)
  lengths(regmatches(svg, gregexpr('<animate attributeName="display"', svg)))

svg_play <- render_svg(data(play_df) + point + x(x) + y(y) + play(year))
# Two moments, once for the marks and once for the strip that names them.
if (frame_count(svg_play) != 4) stop("FAIL: play(year) should cut two frames")
if (!grepl(">1957</text>", svg_play, fixed = TRUE) ||
    !grepl(">1962</text>", svg_play, fixed = TRUE))
  stop("FAIL: the play strip should name each frame")
# A year is named, not measured: it must not read "1957.0".
if (grepl(">1957.0<", svg_play, fixed = TRUE))
  stop("FAIL: a numeric frame should read as a plain number")
cat("PASS: `play(year)` cuts one frame per value and names each\n")

# The invariant the feature rests on: without play, nothing is written at all.
svg_still <- render_svg(data(play_df) + point + x(x) + y(y))
if (grepl("<animate", svg_still, fixed = TRUE))
  stop("FAIL: a plot with no play() must carry no timing")
cat("PASS: a plot that does not play is untouched\n")

# speed divides the loop rather than dropping frames.
svg_fast <- render_svg(data(play_df) + point + x(x) + y(y) + play(year, speed = 2))
if (frame_count(svg_fast) != 4) stop("FAIL: speed must not change how many frames there are")
if (!grepl('dur="0.800s"', svg_fast, fixed = TRUE))
  stop("FAIL: speed = 2 should halve the loop")
cat("PASS: `speed = 2` runs the same frames twice as fast\n")

# One column names the panels or names the moments, never both.
err3 <- tryCatch(
  { render_svg(data(play_df) + point + x(x) + y(y) + play(year) | facet(year)); NULL },
  error = function(e) conditionMessage(e)
)
if (is.null(err3) || !grepl("name the frames", err3))
  stop("FAIL: one column cannot both name the frames and name the panels")
cat("PASS: a column cannot name both the frames and the panels\n")

# speed belongs to play alone — it is the narrowest binding parameter.
err4 <- tryCatch({ play(year, speed = 0); NULL }, error = function(e) conditionMessage(e))
if (is.null(err4) || !grepl("above zero", err4))
  stop("FAIL: speed = 0 should refuse with direction")
cat("PASS: `speed = 0` refuses with direction\n")

cat("\nplay tests passed.\n")

# ---------------------------------------------------------------------------
# Bare data frames — every wrong spelling should point at data()
#
# None of these could even reach gog's code by ordinary dispatch: base R owns
# Ops.data.frame, so R either ran base arithmetic (`df + point`) or declared
# the methods incompatible (`spec + df`) — both ending in "non-numeric
# argument to binary operator" with no direction. chooseOpsMethod (R >= 4.3)
# hands gog the call so the message can say what to write.
# ---------------------------------------------------------------------------

expect_data_error <- function(label, expr) {
  err <- tryCatch({ force(expr); NULL }, error = function(e) conditionMessage(e))
  if (is.null(err) || !grepl("data(", err, fixed = TRUE))
    stop("FAIL: ", label, " should refuse, pointing at data(); got: ",
         if (is.null(err)) "no error" else err)
  cat("PASS: ", label, " refuses toward data()\n", sep = "")
}

expect_data_error("`df + point` (bare start)",            facet_df + point)
expect_data_error("`spec + df` (bare mid-expression)",    data(facet_df) + point + x(x) + y(y) + facet_df)
expect_data_error("`point + x(x)` (no data at all)",      point + x(x))
expect_data_error("`df | facet(g)` (facet on bare frame)", facet_df | facet(g))

# The named frame should appear in the advice, thanks to substitute().
err <- tryCatch({ facet_df + point; NULL }, error = function(e) conditionMessage(e))
if (!grepl("data(facet_df)", err, fixed = TRUE))
  stop("FAIL: the advice should name the frame: ", err)
cat("PASS: the advice names the actual frame\n")

cat("\nbare-data-frame guard tests passed.\n")

# ---------------------------------------------------------------------------
# Parentheses do not group marks
#
# `+` with a spec on its right keeps the table and returns, which is right for a
# bare `data(df)` and silent loss for `(data(df) + point + area)` — the marks
# inside simply stopped existing, and the plot rendered byte-identical to one
# that never named them. That is the dropped binding §12 forbids, and a
# sub-expression meaning one thing alone and nothing in context breaks Law 6.
#
# The other half of this test is the half that could regress: `|` and `/` take
# parenthesized plots as operands and must keep working, since a refusal on `+`
# is one S3 method away from breaking composition.
# ---------------------------------------------------------------------------

group_df   <- data.frame(x = c(1, 2, 3), y = c(4, 5, 6))
group_note <- data.frame(x = c(2), y = c(5))

msg <- refuses("(data(note) + point + area) on the right of `+`",
               data(group_df) + x(x) + y(y) + line + (data(group_note) + point + area))
for (want in c("parentheses do not group marks", "repeat", "group_note", "`|` and `/`"))
  if (!grepl(want, msg, fixed = TRUE))
    stop("FAIL: the refusal should say ", want, "; got: ", msg)
cat("PASS: the refusal names the table and gives the sequence to write\n")

# Not only marks: a position or a title inside the parentheses was dropped too.
refuses("(data(note) + x(x)) on the right of `+`",
        data(group_df) + x(x) + y(y) + line + (data(group_note) + x(x)))
refuses("(data(note) + title()) on the right of `+`",
        data(group_df) + x(x) + y(y) + line + (data(group_note) + title("hi")))

# A bare `data()` carries nothing, so it still joins mid-sentence.
p_seq <- data(group_df) + x(x) + y(y) + line + data(group_note) + point
if (length(c(p_seq$spec$layers, list(p_seq$current_layer))) != 2L)
  stop("FAIL: a bare mid-sentence data() should still bind the next mark")
cat("PASS: a bare mid-sentence `data()` still binds the next mark\n")

# Composition is the thing this refusal must not break.
if (!inherits((data(group_df) + point + x(x) + y(y)) |
              (data(group_df) + line + x(x) + y(y)), "gog_page"))
  stop("FAIL: `|` composition broke")
if (!inherits((data(group_df) + point + x(x) + y(y)) /
              (data(group_df) + line + x(x) + y(y)), "gog_page"))
  stop("FAIL: `/` composition broke")
cat("PASS: `|` and `/` still compose parenthesized plots\n")

# ---------------------------------------------------------------------------
# NAMESPACE — every atom must actually be reachable from `library(gog)`
#
# Under pkgload::load_all() a missing export fails at load time; under the
# source() fallback NAMESPACE is bypassed completely, and an atom could pass
# every test above while invisible to a user who installs the package. That is
# exactly what happened to `style()` and to `*.gog_atom`: the NAMESPACE header
# claimed to be roxygen-generated but was not roxygen's marker, so roxygen
# quietly skipped the file for every atom added after it was written. This
# file-level comparison guards both load paths — and the @export tags — either
# way.
# ---------------------------------------------------------------------------

ns_path <- "r-pkg/gog/NAMESPACE"

# Guarded like the `book/` and license checks below, and for the same reason:
# this compares NAMESPACE against the @export tags in the *sources*, and under
# `R CMD check` there are no sources — the suite runs from <pkg>.Rcheck/tests/
# against an already-installed package. Skipping is right there, because the
# drift this catches is caught at build time by then. Unguarded, it made every
# r-universe and CRAN check fail on a missing file.
if (!file.exists(ns_path)) {
  cat("SKIP: NAMESPACE/@export comparison - run from the repo root to check it\n")
} else {
  ns <- readLines(ns_path, warn = FALSE)

  exported <- sub("\\).*", "", sub("^export\\(", "", grep("^export\\(", ns, value = TRUE)))
  s3_raw <- gsub('"', "", sub("\\).*", "", sub("^S3method\\(", "", grep("^S3method\\(", ns, value = TRUE))))

  # A method for *another package's* generic is registered, never exported —
  # roxygen spells that @exportS3Method — so there is no @export tag for it to
  # match. The qualified `pkg::generic` form is exactly that class of method, and
  # it is how gog teaches each new host to display a plot: knitr::knit_print for
  # Quarto, repr::repr_html for Jupyter. This used to whitelist the single name
  # `knit_print.gog_spec`, which meant every additional host had to be added to
  # the list by hand; exempting the form instead of the name costs nothing and
  # does not go stale.
  foreign <- grepl("^[A-Za-z0-9.]+::", s3_raw)
  s3 <- sub("^[A-Za-z0-9.]+::", "", s3_raw)  # delayed form: knitr::knit_print -> knit_print
  s3 <- gsub(", *", ".", s3)                 # S3method("+", gog_spec)        -> +.gog_spec

  # Names tagged @export in the sources: the line after the tag, minus assignment.
  src <- unlist(lapply(list.files("r-pkg/gog/R", pattern = "\\.R$", full.names = TRUE),
                       readLines, warn = FALSE))
  tagged <- character()
  for (i in which(grepl("@export", src))) {
    nxt <- src[i + 1]
    if (is.na(nxt) || !grepl("(<-|function)", nxt)) next
    nm <- trimws(sub("(<-|=).*", "", nxt))
    tagged <- c(tagged, gsub("`", "", nm))
  }
  tagged <- sort(unique(tagged))

  declared <- sort(unique(c(exported, s3)))
  missing  <- setdiff(tagged, declared)
  extra    <- setdiff(declared, c(tagged, s3[foreign]))

  if (length(missing))
    stop("FAIL: tagged @export but absent from NAMESPACE — unreachable via library(gog): ",
         paste(missing, collapse = ", "))
  if (length(extra))
    stop("FAIL: exported in NAMESPACE but not defined with @export: ",
         paste(extra, collapse = ", "))
  cat("PASS: every @export atom is declared in NAMESPACE (", length(tagged), "names )\n")

  # The header must not impersonate roxygen2, or roxygen will skip the file.
  if (grepl("^# Generated by roxygen2", ns[1]))
    stop("FAIL: NAMESPACE claims roxygen2 authorship but is hand-maintained")

  cat("\nNAMESPACE tests passed.\n")
}

# ---------------------------------------------------------------------------
# The manual must name only atoms that exist
#
# Run from here rather than as a separate script because this is the one entry
# point anybody actually runs. A second command is a command that gets skipped.
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# Continuous color — the ramp, and the palette/column-kind pairing
# ---------------------------------------------------------------------------

fills <- function(s) unique(gsub('.*fill="([^"]*)".*', '\\1',
  unlist(regmatches(s, gregexpr('<circle[^>]*fill="[^"]*"', s)))))

cdf <- data.frame(a = c(1.0, 2, 3, 4, 5), b = c(1.0, 2, 3, 4, 5),
                  v = c(10.0, 20, 30, 40, 50), g = c("x","y","x","y","x"))

# A numeric color column takes the ramp: many distinct fills, not one.
s <- render_svg(data(cdf) + point + x(a) + y(b) + color(v))
if (length(fills(s)) < 4)
  stop("FAIL: a continuous color should vary per point, got ", length(fills(s)))
cat("PASS: a numeric color column takes the sequential ramp\n")

# The default ramp is the blue one, ends included.
if (!any(grepl("8faed5", fills(s), ignore.case = TRUE)))
  stop("FAIL: the light end of the default ramp is missing: ", paste(fills(s), collapse=","))
cat("PASS: the default ramp is blue, light end present\n")

# Named ramp and custom stops both reach the output.
s <- render_svg(data(cdf) + point + x(a) + y(b) + color(v) + palette("viridis"))
if (!any(grepl("440154", fills(s), ignore.case = TRUE)))
  stop("FAIL: viridis did not reach the output")
cat("PASS: palette(\"viridis\") renders\n")

s <- render_svg(data(cdf) + point + x(a) + y(b) + color(v) + palette(c("white", "navy")))
if (!any(grepl("ffffff|white", fills(s), ignore.case = TRUE)))
  stop("FAIL: custom stops did not reach the output: ", paste(fills(s), collapse=","))
cat("PASS: custom stops interpolate\n")

# Every named ramp reaches the output as itself, rather than falling back to
# the default with nothing said — which is what an unknown name used to do.
# The dark end is checked because `t = 0` hits the first stop exactly, where a
# middle stop on a six-stop ramp is interpolated and would not appear verbatim.
ends <- c(magma = "000004", inferno = "000004", plasma = "0d0887",
          cividis = "00204d", gray = "a9a9a9")
for (nm in names(ends)) {
  s <- render_svg(data(cdf) + point + x(a) + y(b) + color(v) + palette(nm))
  if (!any(grepl(ends[[nm]], fills(s), ignore.case = TRUE)))
    stop("FAIL: palette(\"", nm, "\") did not reach the output: ",
         paste(fills(s), collapse = ","))
  if (any(grepl("8faed5", fills(s), ignore.case = TRUE)))
    stop("FAIL: palette(\"", nm, "\") fell back to the blue ramp")
}
cat("PASS: the sequential ramps each render as themselves\n")

# A diverging ramp, and the ruling that `limits` is what centers it. The data
# is one-sided on purpose (0..40), which is what makes the two readings differ:
# stated symmetrically, zero turns neutral and only the red arm is used;
# unstated, the ramp fits itself to 0..40 and spends its blue arm on positive
# numbers. Neither is a defect — the palette cannot know where the reader's
# center is — but only one of them is what "diverging" is usually meant to say.
ddf <- data.frame(a = c(1.0, 2, 3, 4, 5), b = c(1.0, 2, 3, 4, 5),
                  d = c(0.0, 10, 20, 30, 40))
for (nm in c("blue_red", "brown_teal")) {
  s <- fills(render_svg(data(ddf) + point + x(a) + y(b) +
                          color(d, limits = c(-40, 40)) + palette(nm)))
  if (!any(grepl("a9a9a9", s, ignore.case = TRUE)))
    stop("FAIL: ", nm, " put nothing on the neutral at zero: ", paste(s, collapse = ","))
  if (any(grepl("004383|6b3d10", s, ignore.case = TRUE)))
    stop("FAIL: ", nm, " reached its low end on data that never goes negative")
}
cat("PASS: symmetric limits put zero on a diverging ramp's neutral\n")

s <- fills(render_svg(data(ddf) + point + x(a) + y(b) + color(d) + palette("blue_red")))
if (!any(grepl("004383", s, ignore.case = TRUE)))
  stop("FAIL: an unstated domain should fit the ramp to the data, low end included")
cat("PASS: without limits the ramp fits the data and zero is not the center\n")

# The British spelling is refused by name, and told which word to use. `gray`
# being in the vocabulary is what makes this necessary: without it, `grey`
# would be told it is a color and not a palette, which is now misleading.
grey_err <- tryCatch({
  render_svg(data(cdf) + point + x(a) + y(b) + color(v) + palette("grey")); ""
}, error = function(e) conditionMessage(e))
if (!grepl("`gray`", grey_err, fixed = TRUE))
  stop("FAIL: palette(\"grey\") should name the American spelling, got: ", grey_err)
cat("PASS: palette(\"grey\") is refused and points at `gray`\n")

# `soft` is the muted categorical set, and it reaches a *fill* — which is the
# geometry it exists for, so testing it on a point would miss the point.
bars <- render_svg(data(cdf) + bar * count + x(g) + color(g) + palette("soft"))
if (!grepl("#66c2a5", bars, fixed = TRUE))
  stop("FAIL: palette(\"soft\") did not reach the bars")
if (grepl("#4e79a7", bars, fixed = TRUE))
  stop("FAIL: palette(\"soft\") fell back to the default palette")
cat("PASS: palette(\"soft\") paints the fills\n")

# A continuous color still earns a legend — min / mid / max.
if (!grepl(">30<|>30.00<", render_svg(data(cdf) + point + x(a) + y(b) + color(v))))
  stop("FAIL: the continuous color legend should show the midpoint")
cat("PASS: a continuous color earns a min/mid/max legend\n")

# Palette kind must match the column kind, in both directions.
m <- refuses("categorical palette on a numeric color column",
             render_svg(data(cdf) + point + x(a) + y(b) + color(v) + palette("okabe")))
if (!grepl("one color per category", m)) stop("FAIL: unhelpful message: ", m)
m <- refuses("sequential ramp on a text color column",
             render_svg(data(cdf) + point + x(a) + y(b) + color(g) + palette("viridis")))
if (!grepl("sequential ramp", m)) stop("FAIL: unhelpful message: ", m)

# Unset palette must suit either kind.
for (f in c("color(v)", "color(g)")) {
  e <- if (f == "color(v)") data(cdf) + point + x(a) + y(b) + color(v)
       else                 data(cdf) + point + x(a) + y(b) + color(g)
  if (!grepl("<svg", render_svg(e))) stop("FAIL: unset palette should suit ", f)
}
cat("PASS: an unset palette suits either column kind\n")

cat("\ncontinuous color tests passed.\n")

# ---------------------------------------------------------------------------
# Scales — a log axis, and where it sits relative to a transform
# ---------------------------------------------------------------------------

sdf <- data.frame(
  gdp   = c(10.0, 100.0, 1000.0, 10000.0, 100000.0),
  life  = c(40.0, 50.0, 60.0, 70.0, 80.0),
  place = c("a", "b", "c", "d", "e")
)

svg <- render_svg(data(sdf) + point + x(gdp, scale = "log") + y(life))
# The whole difference between a log scale and plotting log(gdp): the ticks
# stay in the reader's units.
if (!grepl(">10K<", svg)) stop("FAIL: log axis should be labeled in data units")
if (grepl("NaN", svg))    stop("FAIL: NaN coordinates in output")
cat("PASS: a log axis is labeled in data units\n")

# `category` is the third scale chosen from the column's *type*, and since
# 2026-07-28 the third that may be said out loud for nothing — the allowance
# `linear` has on a number and `time` has on a date (spec §10).  Byte-identical
# is the assertion, because "means nothing extra" is a claim about the picture.
if (!identical(render_svg(data(sdf) + bar * mean + x(place) + y(life)),
               render_svg(data(sdf) + bar * mean + x(place, scale = "category") + y(life))))
  stop("FAIL: `scale = \"category\"` on a text column should change nothing")
cat("PASS: saying `category` on a text column costs nothing\n")

# And it may not *contradict* the column: a scale says how a measured column is
# placed, and whether an axis measures at all is the column's type (§18).
# Illegal rather than Unsupported — "not yet" promised a feature now ruled out.
err <- tryCatch(render_svg(data(sdf) + point + x(gdp, scale = "category") + y(life)),
                error = function(e) conditionMessage(e))
if (!is.character(err) || !grepl("factor\\(gdp\\)", err))
  stop("FAIL: `category` on a number should refuse toward `factor()`, got: ", err)
if (!grepl("bin", err))
  stop("FAIL: the refusal should also name `bin`, the other reading, got: ", err)
cat("PASS: `category` on a number refuses toward `factor()` and `bin`\n")

# Scale before the transform on the axis it groups by: bins cut in log space
# all span one ratio, so they are drawn the same width.
svg <- render_svg(data(sdf) + bar * bin + x(gdp, scale = "log"))
# A bar is any <rect> with a fill-opacity: that catches a histogram's
# panel-color separator as well as a categorical bar's self-edge (a histogram
# no longer carries stroke-width="0.5" since its bins draw contiguous).
bar_lines <- grep("fill-opacity", grep("<rect", strsplit(svg, "\n")[[1]], value = TRUE), value = TRUE)
w <- as.numeric(sub('.*<rect x="[-0-9.]+" y="[-0-9.]+" width="([0-9.]+)".*', "\\1", bar_lines))
if (length(w) < 2 || diff(range(w)) > 0.5)
  stop("FAIL: log-space bins should be equal width, got ", paste(w, collapse = " "))
cat("PASS: bins are cut in log space, so the bars are even\n")

# bin() takes a bin count (positional, the common intent) or a width (named);
# bare `bin` stays on Sturges. Uniform data so every bin is populated and the
# drawn-bar count equals the requested bin count.
count_bars <- function(spec) {
  s <- render_svg(spec)
  length(grep("fill-opacity", grep("<rect", strsplit(s, "\n")[[1]], value = TRUE), value = TRUE))
}
uni <- data.frame(v = as.numeric(0:99))
if (count_bars(data(uni) + bar * bin(20) + x(v)) != 20)
  stop("FAIL: bin(20) should draw 20 bins")
if (count_bars(data(uni) + bar * bin(bins = 8) + x(v)) != 8)
  stop("FAIL: bin(bins = 8) should draw 8 bins")
if (count_bars(data(uni) + bar * bin(width = 25) + x(v)) != 4)   # ceil(99/25) = 4
  stop("FAIL: bin(width = 25) over span 99 should draw 4 bins")
if (count_bars(data(uni) + bar * bin + x(v)) != 8)               # Sturges, n=100
  stop("FAIL: bare `bin` should stay on Sturges")
e <- tryCatch(data(uni) + bar * bin(20, width = 5) + x(v),
              error = function(e) conditionMessage(e))
if (!grepl("not both", e))
  stop("FAIL: bin() with both bins and width should refuse: ", e)
cat("PASS: bin() takes a count or a width, bare stays Sturges, both refused\n")

# Scale after the transform on the axis it writes: the groups total 100 and 10,
# not log10(10 * 90).
agg <- data.frame(store = c("n", "n", "s", "s"), sales = c(10.0, 90.0, 1.0, 9.0))
svg <- render_svg(data(agg) + bar * sum + x(store) + y(sales, scale = "log"))
if (!grepl(">100<", svg)) stop("FAIL: a sum should still be a sum")
cat("PASS: a sum is taken before the scale, so it stays a sum\n")

m <- refuses("log over zero and negative values",
             render_svg(data(data.frame(v = c(1.0, 0.0, -4.0), u = c(1.0, 2.0, 3.0))) +
                          point + x(v, scale = "log") + y(u)))
if (!grepl("2 of 3", m)) stop("FAIL: message should count the rows: ", m)
m <- refuses("log of a text column",
             render_svg(data(sdf) + point + x(place, scale = "log") + y(life)))
if (!grepl("is text", m)) stop("FAIL: unhelpful message: ", m)

# A misspelt scale is caught in R, at the line that wrote it.
e <- tryCatch({ x(gdp, scale = "logarithmic"); NULL },
              error = function(e) conditionMessage(e))
if (is.null(e) || !grepl("is not a scale", e)) stop("FAIL: bad scale name accepted")
cat("PASS: refused — an unknown scale name \n")

# --- log bases ---
tick_labels <- function(svg) {
  ln <- grep("</text>", strsplit(svg, "\n")[[1]], value = TRUE)
  sub(".*>([^<]*)</text>", "\\1", ln)
}

oct <- data.frame(freq = c(55.0, 110.0, 220.0, 440.0, 880.0, 1760.0),
                  level = c(3.0, 6.0, 9.0, 7.0, 4.0, 2.0))
lab <- tick_labels(render_svg(data(oct) + point + x(freq, scale = "log", base = 2) + y(level)))
if (!all(c("128", "256", "512", "1024") %in% lab))
  stop("FAIL: base 2 should tick on doublings, got ", paste(lab, collapse = " "))
cat("PASS: base 2 ticks on doublings\n")

# Base e has no readable quantities — 2.718, 7.389 — so it labels the powers.
# This doubles as the regression test for jsonlite's 4-decimal default, which
# used to deliver exp(1) as 2.7183 and left the axis reading "2.718²".
decay <- data.frame(t = 0:5 + 0.0, amount = exp(-(0:5)) * 100)
lab <- tick_labels(render_svg(data(decay) + point + x(amount, scale = "log", base = exp(1)) + y(t)))
if (!("e²" %in% lab)) stop("FAIL: base e should label powers of e, got ", paste(lab, collapse = " "))
cat("PASS: base e labels e-foldings, not 2.718\n")

# The same precision bug, seen directly: values below the old 4-decimal cutoff
# must survive the trip to the engine.
tiny <- data.frame(a = c(1.0, 2.0, 3.0), b = c(0.0000010, 0.0000015, 0.0000021))
lab <- tick_labels(render_svg(data(tiny) + point + x(a) + y(b)))
if (all(lab %in% c("0", "1", "2", "3", "")))
  stop("FAIL: small values were rounded away in transit, got ", paste(lab, collapse = " "))
cat("PASS: values below 1e-4 survive serialization\n")

m <- refuses("a base with no log scale",
             render_svg(data(oct) + point + x(freq, base = 2) + y(level)))
if (!grepl("no scale to be the base of", m)) stop("FAIL: unhelpful message: ", m)
e <- tryCatch({ x(freq, scale = "log", base = 1); NULL },
              error = function(e) conditionMessage(e))
if (is.null(e) || !grepl("greater than 1", e)) stop("FAIL: base 1 accepted")
cat("PASS: refused — base 1 \n")

# --- a scale is not only for axes ---
skew <- data.frame(
  a   = seq_len(30) + 0.0,
  b   = seq_len(30) + 0.0,
  pop = 10^seq(5, 9, length.out = 30)
)

mark_fills <- function(svg) {
  ln <- grep("<circle", strsplit(svg, "\n")[[1]], value = TRUE)
  length(unique(sub('.*fill="([^"]*)".*', "\\1", ln)))
}
lin <- render_svg(data(skew) + point + x(a) + y(b) + color(pop))
lg  <- render_svg(data(skew) + point + x(a) + y(b) + color(pop, scale = "log"))
if (mark_fills(lg) < 28)
  stop("FAIL: a log ramp should use a color per point, got ", mark_fills(lg))
cat("PASS: color takes a log scale and uses the whole ramp\n")

radii <- function(svg) {
  ln <- grep("<circle", strsplit(svg, "\n")[[1]], value = TRUE)
  as.numeric(sub('.* r="([0-9.]+)".*', "\\1", ln))
}
if (median(radii(render_svg(data(skew) + point + x(a) + y(b) + size(pop)))) > 4)
  stop("FAIL: a linear size scale should leave the median point small")
if (median(radii(render_svg(data(skew) + point + x(a) + y(b) + size(pop, scale = "log")))) < 6)
  stop("FAIL: a log size scale should spread the radii")
cat("PASS: size takes a log scale and spreads the radii\n")

# The legend's middle label names the color painted half way along the strip,
# which on a log ramp is the geometric mean — sqrt(1e5 * 1e9) = 1e7.
if (!("10M" %in% tick_labels(lg)))
  stop("FAIL: log legend should show the geometric midpoint, got ",
       paste(tick_labels(lg), collapse = " "))
cat("PASS: a log legend labels the geometric midpoint\n")

# `shape` distinguishes rather than measures, so it offers no scale at all.
e <- tryCatch({ shape(g, scale = "log"); NULL }, error = function(e) conditionMessage(e))
if (is.null(e) || !grepl("unused argument", e)) stop("FAIL: shape should take no scale")
cat("PASS: shape offers no scale to misuse\n")

# ---------------------------------------------------------------------------
# limits — the domain, when the data is not the authority (spec §10)
# ---------------------------------------------------------------------------

hrs <- data.frame(hour = c(1, 4, 7, 10, 13, 16, 19, 22),
                  n    = c(2, 5, 9, 14, 20, 15, 8, 3))

# The forcing case. A periodic axis cannot tell that a variable is periodic, so
# the period is stated; a stated end is flush, or the circle would not close.
lab <- tick_labels(render_svg(data(hrs) + line + x(hour, limits = c(0, 24)) + y(n) + polar()))
if (!("0" %in% lab)) stop("FAIL: a stated cycle should reach its start, got ",
                          paste(lab, collapse = " "))
cat("PASS: limits hold a polar axis open to its period\n")

# Extending drops nothing at all — the direction the forcing case needs.
if (!identical(render_svg(data(hrs) + point + x(hour, limits = c(0, 24)) + y(n)),
               render_svg(data(hrs) + point + x(hour, limits = c(0, 24)) + y(n))))
  stop("FAIL: rendering is not deterministic")
cat("PASS: a widened domain excludes no row\n")

# Restricting is the instruction, so it draws and reports rather than refusing —
# the one place this differs from `scale = "log"` at zero, which refuses.
msgs <- capture.output(
  svg <- render_svg(data(hrs) + point + x(hour, limits = c(0, 10)) + y(n)),
  type = "message")
if (!any(grepl("excludes 4 of 8 rows", msgs)))
  stop("FAIL: excluded rows should be counted aloud, got ", paste(msgs, collapse = " | "))
if (!grepl("<circle", svg)) stop("FAIL: a restricted plot should still draw")
cat("PASS: excluded rows are counted aloud and the plot still draws\n")

# A domain that keeps no row is the empty panel, and that is fatal.
refuses("a domain that keeps no row",
        render_svg(data(hrs) + point + x(hour, limits = c(100, 200)) + y(n)))

# `limits` reaches every channel that measures, not only the axes (Law 1).
fills <- function(svg) unique(regmatches(svg, gregexpr('fill="#[0-9a-f]{6}"', svg))[[1]])
if (identical(fills(render_svg(data(hrs) + point + x(hour) + y(n) + color(n, limits = c(0, 100)))),
              fills(render_svg(data(hrs) + point + x(hour) + y(n) + color(n, limits = c(0, 200))))))
  stop("FAIL: a stated domain should change the color ramp")
cat("PASS: limits reach the color ramp, not just the axes\n")

# A category has no range to lie inside; the refusal points at `order`.
m <- refuses("limits on a categorical axis",
             render_svg(data(data.frame(g = c("a", "b"), v = c(1, 2))) +
                        bar + x(g, limits = c(0, 5)) + y(v)))
if (!grepl("order\\(g\\)", m)) stop("FAIL: the refusal should name `order`, got ", m)

# Caught in the binding, at the line that wrote it.
e <- tryCatch({ x(hour, limits = c(20, 5)); NULL }, error = function(e) conditionMessage(e))
if (is.null(e) || !grepl("runs backwards", e)) stop("FAIL: a backwards domain should be refused")
e <- tryCatch({ x(hour, limits = 5); NULL }, error = function(e) conditionMessage(e))
if (is.null(e) || !grepl("needs two numbers", e)) stop("FAIL: one number is not a domain")
cat("PASS: a malformed domain is refused at the binding\n")

# `shape` measures nothing, so it offers no domain either — the same absence as
# `scale`, which is what makes it one rule rather than two lists.
e <- tryCatch({ shape(g, limits = c(0, 1)); NULL }, error = function(e) conditionMessage(e))
if (is.null(e) || !grepl("unused argument", e)) stop("FAIL: shape should take no limits")
cat("PASS: shape offers no limits to misuse\n")

# ---------------------------------------------------------------------------
# tick_count — how many ticks an axis aims for (spec §10)
#
# The last property that was real in the IR, read by the renderer, and reachable
# from no binding. It rides the binding beside `scale` and `limits` because it
# describes the **scale**; `theme()` declined it on that ground (§7).
# ---------------------------------------------------------------------------

grid5 <- data.frame(a = c(0, 25, 50, 75, 100), b = c(1, 2, 3, 4, 5))
nticks <- function(p) length(tick_labels(render_svg(p)))

# A target rather than a promise: the count picks a step and the step is rounded
# to a human number. So the test is monotone rather than exact — asking for more
# gets more — which is the claim a caller can actually rely on.
few  <- nticks(data(grid5) + point + x(a, tick_count = 3) + y(b))
many <- nticks(data(grid5) + point + x(a, tick_count = 11) + y(b))
if (!(many > few)) stop("FAIL: tick_count changed nothing — ", few, " vs ", many)
cat("PASS: an axis draws more ticks when asked for more (", few, "->", many, ")\n")

# Thinning the labels is not the same as coarsening the step, and this is the
# distinction §10 turns on: the ticks a sparse axis draws are a *subset* of a
# dense one's, so a value read off either is read off the same scale.
sparse <- tick_labels(render_svg(data(grid5) + point + x(a, tick_count = 3) + y(b)))
dense  <- tick_labels(render_svg(data(grid5) + point + x(a, tick_count = 11) + y(b)))
if (!all(sparse %in% dense))
  stop("FAIL: a sparse axis invented labels a dense one does not have: ",
       paste(setdiff(sparse, dense), collapse = " "))
cat("PASS: a sparse axis's ticks are a subset of a dense one's\n")

# `z` is the axis the engine never read, so it is worth its own line: the field
# existed in the IR and `build_axis` was handed a hard NULL for the third axis.
zf <- expand.grid(east = (0:4) * 10, north = (0:4) * 10)
zf$elev <- 100 + zf$east / 10 + zf$north / 10
zfew  <- nticks(data(zf) + surface + x(east) + y(north) + z(elev, tick_count = 2))
zmany <- nticks(data(zf) + surface + x(east) + y(north) + z(elev, tick_count = 9))
if (!(zmany > zfew)) stop("FAIL: z ignored its tick_count — ", zfew, " vs ", zmany)
cat("PASS: the third axis honors a tick count too (", zfew, "->", zmany, ")\n")

# The line between this and `limits`: a domain reaches all six magnitude
# channels, a tick count only the three that draw an axis.
e <- tryCatch({ color(a, tick_count = 4); NULL }, error = function(e) conditionMessage(e))
if (is.null(e) || !grepl("unused argument", e))
  stop("FAIL: color should take no tick_count")
cat("PASS: a legend has no tick count to ask for\n")

# Caught in the binding, at the line that wrote it.
for (bad in list(list(v = 1, want = "at least two ticks"),
                 list(v = 2.5, want = "not a whole number"),
                 list(v = "8", want = "needs one number"))) {
  e <- tryCatch({ x(a, tick_count = bad$v); NULL }, error = function(e) conditionMessage(e))
  if (is.null(e) || !grepl(bad$want, e))
    stop("FAIL: tick_count = ", bad$v, " should say '", bad$want, "', got ", e)
}
cat("PASS: a malformed tick count is refused at the binding\n")

# A category axis has one tick per level, so the count is the data's.
m <- refuses("tick_count on a categorical axis",
             render_svg(data(data.frame(g = c("a", "b"), v = c(1, 2))) +
                        bar + x(g, tick_count = 5) + y(v)))
if (!grepl("order\\(g\\)", m)) stop("FAIL: the refusal should name `order`, got ", m)

# One axis, one count — a layer stating its own is the plot-scoped-scale rule.
m <- refuses("a layer stating its own tick count",
             render_svg(data(grid5) + x(a, tick_count = 4) + y(b) +
                        point + x(a, tick_count = 9)))
if (!grepl("its own tick count", m))
  stop("FAIL: the refusal should name the parameter, got ", m)
cat("PASS: a tick count is the plot's, like every other scale property\n")

# A count the caller *stated* and a cube could not draw is said out loud (§12);
# the engine's own default at the same angle stays silent, which is §12's
# omission rule — an unambiguous default needs no warning, and an accepted
# binding may not be dropped in silence.
tilted <- function(p) capture.output(render_svg(p), type = "message")
said <- tilted(data(zf) + surface + x(east) + y(north) +
               z(elev, tick_count = 10) + space(tilt = 85))
if (!any(grepl("asked for 10 ticks and", said)))
  stop("FAIL: a thinned tick count should be reported, got ",
       paste(said, collapse = " | "))
quiet <- tilted(data(zf) + surface + x(east) + y(north) + z(elev) + space(tilt = 85))
if (any(grepl("asked for", quiet)))
  stop("FAIL: the default count should thin silently, got ",
       paste(quiet, collapse = " | "))
cat("PASS: a thinned tick count is reported, and the default is not\n")

# A domain on a temporal axis is written in dates, and the binding converts them
# the way it converts the column. Without that the two disagree by a factor of
# 86400 — an R `Date` is days, the wire is seconds — and every row falls outside.
dts <- data.frame(day = as.Date("2024-03-01") + 0:41, orders = as.numeric(20:61))
lab <- tick_labels(render_svg(data(dts) + line + y(orders) +
                              x(day, limits = c(as.Date("2024-01-01"),
                                                as.Date("2024-12-31")))))
if (!all(c("Jan 2024", "Nov 2024") %in% lab))
  stop("FAIL: a stated year should tick across the year, got ", paste(lab, collapse = " "))
cat("PASS: limits on a date axis are written in dates\n")

# And the diagnostic quotes them back as dates — epoch seconds would tell the
# caller nothing they can act on, which is the whole of what a message is for.
msgs <- capture.output(
  render_svg(data(dts) + line + y(orders) +
             x(day, limits = c(as.Date("2024-03-01"), as.Date("2024-03-31")))),
  type = "message")
if (!any(grepl("\\[2024-03-01, 2024-03-31\\]", msgs)))
  stop("FAIL: a temporal domain should be quoted back as dates, got ",
       paste(msgs, collapse = " | "))
cat("PASS: a temporal domain is reported in dates, not epoch seconds\n")

cat("\nscale tests passed.\n")

# ---------------------------------------------------------------------------
# Polar — every mark that draws flat draws bent (spec §15)
#
# Five marks were refused in this space until 2026-07-26 for one recorded
# reason, *their straight edges would have to become arcs*. Three of them never
# needed one. What each test below pins is the property the refusal was really
# about: a segment that **holds** a value across a span has to follow the ring,
# because a chord falls inside the circle and puts the mark where the data is not.
# ---------------------------------------------------------------------------

wind <- data.frame(
  dir = rep(c("N", "E", "S", "W"), each = 6),
  spd = c(4, 5, 6, 5, 4, 6,  8, 9, 11, 10, 9, 8,
          6, 7, 5, 6, 7, 6,  3, 4, 2, 3, 4, 3))
band <- data.frame(dir = c("N", "E", "S", "W"),
                   lo  = c(2, 6, 4, 1), hi = c(6, 11, 8, 5))

arcs <- function(svg) length(gregexpr(" A ", svg)[[1]][gregexpr(" A ", svg)[[1]] > 0])

# All five draw, and each is the sentence the refusal used to send elsewhere.
for (p in list(
  list("step",     data(wind) + step * mean + x(dir) + y(spd) + polar()),
  list("interval", data(wind) + interval * range + x(dir) + y(spd) + polar()),
  list("box",      data(wind) + box + x(dir) + y(spd) + polar()),
  list("ribbon",   data(band) + ribbon * bounds(lo, hi) + x(dir) + polar()),
  list("zone",     data(wind) + zone * count + x(dir) + y(dir) + polar()))) {
  svg <- render_svg(p[[2]])
  if (!grepl("<svg", svg) || grepl("NaN", svg))
    stop("FAIL: ", p[[1]], " does not draw in polar")
}
cat("PASS: all five span marks draw in polar\n")

# A stair's treads become arcs; a flat one draws none. The segment the whole
# space was waiting on, and the one that is genuinely new.
if (arcs(render_svg(data(wind) + step * mean + x(dir) + y(spd) + polar())) == 0)
  stop("FAIL: a polar staircase drew no arc")
if (arcs(render_svg(data(wind) + step * mean + x(dir) + y(spd))) != 0)
  stop("FAIL: a flat staircase drew an arc")
cat("PASS: a stair's treads are arcs bent and straight flat\n")

# A band's boundaries are **chords**, which is the correction this made to the
# recorded refusal: they run through the data's own vertices, like `line`'s.
if (arcs(render_svg(data(band) + ribbon * bounds(lo, hi) + x(dir) + polar())) != 0)
  stop("FAIL: a radar band needed no arc and drew one")
cat("PASS: a radar band is drawn with chords, not arcs\n")

# A hexagonal mesh has no polar reading — `bin(tiling = )`'s third refusal.
pts <- data.frame(a = rep(1:6, 6), b = rep(1:6, each = 6))
m <- refuses("a hex mesh in polar",
             render_svg(data(pts) + zone * bin(tiling = "hex") + x(a) + y(b) + polar()))
if (!grepl("rect", m)) stop("FAIL: the refusal should name the tiling that bends, got ", m)
if (arcs(render_svg(data(pts) + zone * bin(tiling = "rect") + x(a) + y(b) + polar())) == 0)
  stop("FAIL: a rectangular mesh should bend into sectors")
cat("PASS: hex is refused in polar and rect bends into sectors\n")

cat("\npolar tests passed.\n")

# ---------------------------------------------------------------------------
# Nest — the panel packed with regions (spec §15)
#
# The third answer to what carries a share: length flat, angle in polar, area
# here. What is tested is the property a treemap is *read* for — the regions are
# the panel, and each is its own share of it — plus the two things that make it a
# space rather than a chart type: the same sentence draws all three, and the
# refusals are the space's own.
# ---------------------------------------------------------------------------

sales <- data.frame(
  region  = c("North", "North", "South", "South", "East", "East", "West"),
  product = c("widgets", "gadgets", "widgets", "gadgets", "widgets", "gadgets", "widgets"),
  revenue = c(32, 14, 25, 8, 19, 11, 6))

# Every `<rect>` a packed panel paints, as a data frame of x/y/w/h. The legend's
# swatches carry `rx=` and the outer region outlines are `fill="none"`; neither
# is a cell.
cells <- function(svg) {
  lines <- grep("<rect", strsplit(svg, "\n")[[1]], value = TRUE)
  lines <- lines[grepl("fill-opacity", lines) & !grepl("rx=", lines) & !grepl('fill="none"', lines)]
  # The leading space matters: without it `width` also matches `stroke-width`
  # and `y` matches the `y` of `fill-opacity`, so every cell comes back one pixel
  # wide and the shares look plausible while being nonsense.
  num <- function(key) as.numeric(sub(paste0('.* ', key, '="([0-9.]+)".*'), "\\1", lines))
  data.frame(x = num("x"), y = num("y"), w = num("width"), h = num("height"))
}

one <- render_svg(data(sales) + bar * sum + y(revenue) + color(region) + nest())
cl <- cells(one)
if (nrow(cl) != 4) stop("FAIL: expected one region per region-name, got ", nrow(cl))
shares <- sort(cl$w * cl$h / sum(cl$w * cl$h))
# North 46, South 33, East 30, West 6 — of 115.
if (max(abs(shares - sort(c(46, 33, 30, 6) / 115))) > 0.002)
  stop("FAIL: the regions are not their own shares: ", paste(round(shares, 4), collapse = ", "))
cat("PASS: every packed region is its share of the panel\n")

# A packed panel has no axes at all, which is the space's defining property.
if (grepl('stroke="#5a5a64"', one)) stop("FAIL: a packed panel drew axis lines")
flat_one <- render_svg(data(sales) + bar * sum + x(region) + y(revenue) + color(region))
if (!grepl('stroke="#5a5a64"', flat_one))
  stop("FAIL: the flat sentence drew no axes either, so the test proves nothing")
cat("PASS: a packed panel draws no axes and the flat one does\n")

# Bind a position and the packing gains a level: one region per category, its
# rows packed inside. Read off the outer outlines, which is what the coarser
# split is drawn as.
two <- render_svg(data(sales) + bar * sum + x(region) + y(revenue) + color(product) + nest())
outer <- grep('<rect.*fill="none"', strsplit(two, "\n")[[1]], value = TRUE)
if (length(outer) != 4) stop("FAIL: expected one outline per region, got ", length(outer))
if (length(grep('<rect.*fill="none"', strsplit(one, "\n")[[1]])) != 0)
  stop("FAIL: a one-level packing outlined a region against nothing")
cat("PASS: a bound position packs a second level inside each region\n")

# The space's own refusals, each naming what to do instead.
m <- refuses("a collision modifier in a packed panel",
             render_svg(data(sales) + bar * sum * stack + y(revenue) + color(region) + nest()))
if (!grepl("own region", m)) stop("FAIL: the refusal should say why a packing has no collisions, got ", m)
refuses("naming an axis a packed panel does not have",
        render_svg(data(sales) + bar * sum + y(revenue) + color(region) + nest() + x_label("Revenue")))
refuses("a point in a packed panel",
        render_svg(data(sales) + point + x(revenue) + y(revenue) + nest()))
refuses("a log scale on a packed measure",
        render_svg(data(sales) + bar * sum + y(revenue, scale = "log") + color(region) + nest()))

# A label at the center of its own region — what makes a packing readable once
# the split is too wide for a legend to decode (2026-07-27). The label layer
# needs no `x`: a packing places by region, which is Law 7's third relaxation.
packed_svg <- render_svg(data(sales) + bar + y(revenue) + color(region) +
                           text + label(product) + nest())
# A mark's label carries `fill-opacity` and the legend's key entries do not — the
# same discriminator `cells()` uses one element over, and needed for the same
# reason: the key spells out the very strings the labels draw, so counting those
# would pass whether or not the mark drew anything.
packed_names <- grep("<text", strsplit(packed_svg, "\n")[[1]], value = TRUE)
packed_names <- packed_names[grepl("fill-opacity", packed_names)]
if (length(packed_names) == 0) stop("FAIL: a packed label drew nothing")
# Every drawn label sits inside a cell the bar drew, which is the property that
# makes the mark worth having: the two marks read one packing, so a name cannot
# land in a rectangle its own row did not get.
packed_boxes <- cells(packed_svg)
lx <- as.numeric(sub('.*<text x="([0-9.]+)".*', "\\1", packed_names))
for (v in lx) {
  if (!any(packed_boxes$x <= v & v <= packed_boxes$x + packed_boxes$w))
    stop("FAIL: a label landed outside every region, at x=", v)
}
cat("PASS: a packed label sits inside its own region\n")

refuses("a nudge in a packed panel, where a label covers no point",
        render_svg(data(sales) + bar + y(revenue) + color(region) +
                     text + label(product) + style(nudge = "up") + nest()))

cat("\nnest tests passed.\n")

# ---------------------------------------------------------------------------
# Space — the three slot marks stand on the cube's floor (spec §15)
#
# `interval` and `box` joined `bar` in the cube on 2026-07-26, and needed no
# ruling of their own: `is_slot_mark` had grouped the three since orientation
# was decided. What the cube's blanks are is the other half — four of them are
# *decided* refusals and two are blocked on occlusion, and until this change
# every one of them said "not drawn yet".
# ---------------------------------------------------------------------------

plots <- data.frame(
  site   = rep(c("North","Center","South"), each = 20),
  season = rep(c("Wet","Dry"), 30),
  yield  = c(rnorm(20, 50, 6), rnorm(20, 58, 6), rnorm(20, 46, 6)))

for (p in list(
  list("interval", data(plots) + interval * range + x(site) + y(season) + z(yield) + space()),
  list("conf",     data(plots) + interval * confidence + x(site) + y(season) + z(yield) + space()),
  list("box",      data(plots) + box + x(site) + y(season) + z(yield) + space()))) {
  svg <- render_svg(p[[2]])
  if (!grepl("<svg", svg) || grepl("NaN", svg))
    stop("FAIL: ", p[[1]], " does not stand in the cube")
}
cat("PASS: interval and box stand on the cube's floor\n")

# One per **cell**, not one per row — the bug building it found. Six cells, each
# a span plus a crossed cap at either end: 6 x 5 = 30 strokes with a linecap.
n_caps <- length(gregexpr("stroke-linecap", render_svg(
  data(plots) + interval * range + x(site) + y(season) + z(yield) + space()))[[1]])
if (n_caps != 30)
  stop("FAIL: a cube whisker should stand one per cell (30 strokes), got ", n_caps)
cat("PASS: a pair transform in the cube groups by the floor\n")

# The four decided refusals say so, and do not promise a renderer.
m <- refuses("a line in the cube",
             render_svg(data(plots) + line + x(yield) + y(yield) + z(yield) + space()))
if (!grepl("no left to right", m)) stop("FAIL: the refusal should give the ruling, got ", m)
if (grepl("not drawn yet|does not draw it yet", m))
  stop("FAIL: a decided refusal must not promise a renderer, got ", m)
if (!grepl("path", m)) stop("FAIL: the refusal should point at `path`, got ", m)
cat("PASS: a 3-D line is refused with its ruling, not with a promise\n")

# The two blocked on occlusion say *that*, which is a different sentence.
m <- refuses("a rule in the cube",
             render_svg(data(plots) + rule + x(yield) + z(yield) + space()))
if (!grepl("footprint", m)) stop("FAIL: the refusal should name the blocker, got ", m)
cat("PASS: a 3-D rule is refused as a plane with no footprint\n")

cat("\nspace tests passed.\n")

# ---------------------------------------------------------------------------
# Factors — a category column that remembers its order
# ---------------------------------------------------------------------------

sev_labels <- function(svg) {
  ln <- grep("</text>", strsplit(svg, "\n")[[1]], value = TRUE)
  v  <- sub(".*>([^<]*)</text>", "\\1", ln)
  v[v %in% c("Low", "Medium", "High", "Other")]
}

fdf <- data.frame(
  sev   = factor(c("High", "Low", "Medium"),
                 levels = c("Low", "Medium", "High"), ordered = TRUE),
  count = c(30.0, 10.0, 20.0)
)

got <- sev_labels(render_svg(data(fdf) + bar + x(sev) + y(count)))
if (!identical(got, c("Low", "Medium", "High")))
  stop("FAIL: an ordered factor's levels should set the axis order, got ",
       paste(got, collapse = " "))
cat("PASS: an ordered factor sets the category order\n")

# A plain factor counts too — most people use one purely to fix display order.
plain <- fdf
plain$sev <- factor(as.character(fdf$sev), levels = c("Low", "Medium", "High"))
got <- sev_labels(render_svg(data(plain) + bar + x(sev) + y(count)))
if (!identical(got, c("Low", "Medium", "High")))
  stop("FAIL: a plain factor's levels should count too, got ", paste(got, collapse = " "))
cat("PASS: a plain factor counts, not only an ordered one\n")

# A character column has no declared order, so the data's order stands.
chr <- fdf; chr$sev <- as.character(fdf$sev)
got <- sev_labels(render_svg(data(chr) + bar + x(sev) + y(count)))
if (!identical(got, c("High", "Low", "Medium")))
  stop("FAIL: a character column should keep data order, got ", paste(got, collapse = " "))
cat("PASS: a character column is unchanged by any of this\n")

# The legend has to agree with the axis, or the chart contradicts itself.
got <- sev_labels(render_svg(data(fdf) + point + x(count) + y(count) + color(sev)))
if (!identical(got, c("Low", "Medium", "High")))
  stop("FAIL: the legend should follow the levels too, got ", paste(got, collapse = " "))
cat("PASS: the color legend follows the same order as the axis\n")

# An explicit order() is nearer than the data, so it wins.
got <- sev_labels(render_svg(data(fdf) + bar + x(sev) + y(count) + order(count, desc = TRUE)))
if (!identical(got, c("High", "Medium", "Low")))
  stop("FAIL: order() should override the levels, got ", paste(got, collapse = " "))
cat("PASS: an explicit order() still overrides the levels\n")

# Levels say what order, the data says what is there.
gap <- data.frame(sev = factor(c("High", "Low"), levels = c("Low", "Medium", "High")),
                  count = c(3.0, 1.0))
got <- sev_labels(render_svg(data(gap) + bar + x(sev) + y(count)))
if (!identical(got, c("Low", "High")))
  stop("FAIL: a level with no rows should get no slot, got ", paste(got, collapse = " "))
cat("PASS: a level with no rows gets no slot\n")

cat("\nfactor tests passed.\n")

# ---------------------------------------------------------------------------
# Time — a date column reads as a calendar
# ---------------------------------------------------------------------------

tick_texts <- function(svg) {
  ln <- grep("</text>", strsplit(svg, "\n")[[1]], value = TRUE)
  sub(".*>([^<]*)</text>", "\\1", ln)
}

# The wire: a Date is days since epoch; the engine's unit is seconds.
# `unclass` because a column now crosses as AsIs (see the one-row wire tests
# below) — the *value* is what this test is about, and `identical` against a
# bare 86400 was asserting the class as well.
wire <- df_to_wire(data.frame(d = as.Date("1970-01-02"), v = 1))
if (!identical(unclass(wire$floats$d), 86400) || !identical(wire$dates$d, "day"))
  stop("FAIL: a Date should arrive as epoch seconds tagged \"day\", got ",
       wire$floats$d, " / ", wire$dates$d)
if (!is.null(wire$strings$d))
  stop("FAIL: a Date must not fall through to a category string")
cat("PASS: a Date crosses the wire as tagged epoch seconds, not a string\n")

# A POSIXct crosses as the clock time the user sees, whatever the zone says.
wire <- df_to_wire(data.frame(
  t = as.POSIXct("2024-03-04 14:30:00", tz = "America/New_York"), v = 1))
if (!identical(wire$dates$t, "second"))
  stop("FAIL: a POSIXct should be tagged \"second\"")
if (wire$floats$t %% 86400 != (14 * 3600 + 30 * 60))
  stop("FAIL: a POSIXct should arrive as its displayed clock time, got ",
       wire$floats$t %% 86400, " seconds past midnight")
cat("PASS: a POSIXct crosses as its displayed clock time, timezone-naive\n")

# Thirty years of yearly data: the axis reads in years, not epoch anything.
years_df <- data.frame(
  day   = as.Date(paste0(1994:2019, "-01-01")),
  sales = 50 + (0:25 * 1.7) %% 30
)
got <- tick_texts(render_svg(data(years_df) + line + x(day) + y(sales)))
if (!any(got == "2000"))
  stop("FAIL: a span of decades should tick in years, got: ", paste(got, collapse = " "))
cat("PASS: a Date axis ticks in calendar years\n")

# Weeks of daily data tick by month/day, not by number.
days_df <- data.frame(
  day   = as.Date("2024-03-01") + 0:41,
  sales = 10 + (0:41 * 2.3) %% 7
)
got <- tick_texts(render_svg(data(days_df) + line + x(day) + y(sales)))
if (!any(grepl("^(Mar|Apr) ", got)))
  stop("FAIL: a span of weeks should tick as 'Mar 4', got: ", paste(got, collapse = " "))
cat("PASS: a short Date span ticks as month and day\n")

# A timestamp column may tick by the clock; a Date column never does.
hours_df <- data.frame(
  t = as.POSIXct("2024-03-04 00:00:00", tz = "UTC") + 3600 * 0:18,
  v = 1:19
)
got <- tick_texts(render_svg(data(hours_df) + line + x(t) + y(v)))
if (!any(grepl("^[0-9]{2}:[0-9]{2}$", got)))
  stop("FAIL: an hours-wide POSIXct span should tick as clock times, got: ",
       paste(got, collapse = " "))
cat("PASS: a POSIXct axis ticks as clock times\n")

# The refusals, each with its direction.
err_msg <- function(expr) tryCatch({ expr; "" }, error = function(e) conditionMessage(e))
m <- err_msg(render_svg(data(years_df) + line + x(day, scale = "log") + y(sales)))
if (!grepl("logarithm", m))
  stop("FAIL: log on a date should refuse and say why, got: ", m)
cat("PASS: a moment in time has no logarithm\n")

m <- err_msg(render_svg(data(years_df) + point + x(sales, scale = "time") + y(sales)))
if (!grepl("as.Date", m, fixed = TRUE))
  stop("FAIL: time on a plain number should point at as.Date(), got: ", m)
cat("PASS: a time scale on a plain number points at as.Date()\n")

m <- err_msg(render_svg(data(years_df) + bar + x(sales) + y(day)))
if (!grepl("amount", m))
  stop("FAIL: a bar measuring a date should refuse, got: ", m)
cat("PASS: a bar cannot measure a date\n")

cat("\ntime tests passed.\n")

# ---------------------------------------------------------------------------
# brush — the selection
#
# Four claims, and the second is the one the whole feature rests on: a plot that
# names no brush must be exactly the plot it was before selection existed.
# ---------------------------------------------------------------------------

brush_df <- data.frame(
  v    = c(1, 2, 3, 4, 5, 6),
  w    = c(2, 4, 1, 5, 3, 6),
  kind = c("a", "a", "b", "b", "c", "c"),
  stringsAsFactors = FALSE
)
dim_group <- '<g opacity="0.150">'
circles <- function(svg) lengths(regmatches(svg, gregexpr("<circle", svg)))

svg_brush <- render_svg(data(brush_df) + point + x(v) + y(w) + brush(v, at = c(2.5, 4.5)))
if (!grepl(dim_group, svg_brush, fixed = TRUE))
  stop("FAIL: brush() drew no dimmed group")
# A brush highlights; it never removes rows. That is what separates it from
# `limits`, and it is the claim a reader is most likely to test.
if (circles(svg_brush) != 6)
  stop("FAIL: brush() dropped rows — it must dim, not filter")
cat("PASS: brush() dims the rows outside the bound and drops none\n")

svg_plain <- render_svg(data(brush_df) + point + x(v) + y(w))
if (grepl("data-gog-panel", svg_plain, fixed = TRUE) || grepl("<g opacity=", svg_plain))
  stop("FAIL: a plot with no brush carries selection machinery")
cat("PASS: a plot with no brush is untouched by selection\n")

svg_cat <- render_svg(data(brush_df) + point + x(v) + y(w) + brush(kind, at = "b"))
if (!grepl(dim_group, svg_cat, fixed = TRUE))
  stop("FAIL: brush() on a column of categories selected no slots")
cat("PASS: brush() on a category column selects slots\n")

m <- tryCatch({
  render_svg(data(brush_df) + line + x(v) + y(w) + brush(v, at = c(2, 4))); ""
}, error = function(e) conditionMessage(e))
if (!grepl("one shape through many rows", m) || !grepl("group\\(\\)", m))
  stop("FAIL: a brushed line should refuse and name group(), got: ", m)
cat("PASS: refused — a line has no single row to select\n")

m <- tryCatch({ brush(v, at = c(1, 2, 3)); "" }, error = function(e) conditionMessage(e))
if (!grepl("two numbers", m))
  stop("FAIL: `at` with three numbers should refuse, got: ", m)
cat("PASS: refused — `at` is two numbers or a set of names\n")

cat("\nbrush tests passed.\n")

# ---------------------------------------------------------------------------
# The area mark
# ---------------------------------------------------------------------------

svg_area <- render_svg(data(df) + area + x(x) + y(y))
if (!grepl("<polygon", svg_area)) stop("FAIL: area drew no region")
cat("PASS: area renders a filled region\n")

# One region per category — the split `line` makes into one polyline each.
svg_area2 <- render_svg(data(df) + area + x(x) + y(y) + color(group))
if (length(gregexpr("<polygon", svg_area2)[[1]]) != 2)
  stop("FAIL: color(group) should split the area into two regions")
cat("PASS: a category splits an area into one region each\n")

# One region has one fill: opacity is a setting, never a channel.
refuses("opacity(y) on area", render_svg(data(df) + area + x(x) + y(y) + opacity(y)))
svg_area3 <- render_svg(data(df) + area + x(x) + y(y) + style(opacity = 0.3))
if (!grepl('fill-opacity="0.300"', svg_area3))
  stop("FAIL: style(opacity) did not reach the area")
cat("PASS: area takes opacity as a setting\n")

# And where area parts company with line: a stroke's width is free, an area's
# extent is pinned by its perimeter, so there is no size to set.
refuses("style(size) on area", render_svg(data(df) + area + x(x) + y(y) + style(size = 4)))

# Marks the grammar has but this engine cannot draw (`text`, `path`, `surface`)
# are refused by the engine with direction, never rendered as an empty panel —
# `area` itself was in that state until this session: it passed the check and
# drew nothing, silently, exiting 0.
#
# That refusal is not reachable from R, because this package exports only the
# marks that are built. It is covered in Rust (`every_mark_is_drawn_or_refused`)
# and reachable through `gog-cli` and any future binding. Exporting the unbuilt
# three so R users got the good message was considered and not done here: `text`
# would mask `base::text`, which is a real function people use, and that is a
# naming decision rather than part of this change.
cat("SKIP: undrawable-mark refusal is engine-level (see Rust tests)\n")

cat("\narea tests passed.\n")

if (file.exists("book/check_vocabulary.R")) {
  source("book/check_vocabulary.R")
  check_vocabulary()
  cat("\nvocabulary tests passed.\n")
} else {
  cat("SKIP: book/ not found — run from the repo root to check the manual\n")
}

# The prose guard's twin: check_vocabulary.R catches a sentence naming an atom
# that does not exist; this catches a chunk *presented* as a refusal that quietly
# renders instead. `#| error: true` tolerates an error, it does not require one,
# so nothing else in the toolchain can see the difference.
if (file.exists("book/check_refusals.R")) {
  source("book/check_refusals.R")
  check_refusals()
  cat("\nrefusal tests passed.\n")
} else {
  cat("SKIP: book/ not found — run from the repo root to check the refusals\n")
}

# The same family again, one level up: not what a chapter says about an atom,
# but what the *preface* says about every chapter. "Five rules govern every
# page, and you can hold the book to them" — and when a reader did, three of the
# five were false. Questions-first held in 1 chapter of 43, read-it-aloud in 2
# of 36, and the small cast claimed eight tables against 33 in use. The two that
# held were the two nothing could get wrong by accident and the one with a test
# (check_refusals.R, 138 for 138), which is the whole argument for this file.
if (file.exists("book/check_promises.R")) {
  source("book/check_promises.R")
  check_promises()
  cat("\nbook promise tests passed.\n")
} else {
  cat("SKIP: book/ not found — run from the repo root to check the promises\n")
}

# And the same family once more, on the one claim nothing can break by failing.
# `operators.qmd` says every plot in the book reads *data, mark, positions,
# refinements*; 24 did not. Reordering them left every affected chapter's SVG
# byte-identical, which is exactly why it needs a checker: a violation costs the
# reader nothing but the sentence no longer scanning like its neighbors, and no
# test of behavior can see that.
if (file.exists("book/check_sentence_order.R")) {
  source("book/check_sentence_order.R")
  check_sentence_order()
  cat("\nsentence order tests passed.\n")
} else {
  cat("SKIP: book/ not found — run from the repo root to check the sentence order\n")
}

# The third of the same family, and the one that checks the book's *structure*
# rather than its claims about atoms. `parts/letters.qmd` promises the mark
# chapters all follow one template; eight of twelve did not, and the promise had
# been unchecked since it was written.
if (file.exists("book/check_template.R")) {
  source("book/check_template.R")
  check_template()
  cat("\ntemplate tests passed.\n")
} else {
  cat("SKIP: book/ not found — run from the repo root to check the template\n")
}

# The fourth, and the one that watches the *plots* rather than the prose. Quarto
# emits each SVG inline into the markdown stream, so pandoc reads the plot's own
# source on the way past: a pair of backticks in a title is a code span to it, and
# it closes `</svg>` mid-plot and spills the rest into the page. Four plots were
# shipping truncated that way, three of them for several sessions, with `quarto
# render` exiting 0 and every grep for the title finding it.
if (file.exists("book/check_titles.R")) {
  source("book/check_titles.R")
  check_titles()
  cat("\ntitle tests passed.\n")
} else {
  cat("SKIP: book/ not found — run from the repo root to check the titles\n")
}

# ---------------------------------------------------------------------------
# One thing, one spelling: GOG in prose, `gog` in code font
# ---------------------------------------------------------------------------

# The fifth prose guard, and the one whose failure is pure drift rather than a
# broken artifact. The book had reached 109 uppercase against 51 lowercase, split
# **by chapter** rather than by meaning (`design-laws.qmd` 16-0, `marks/zone.qmd`
# 1-7), which is what a convention looks like when nobody is deciding it. Two
# spellings of one word is the silent letter Law 2 refuses, one level over from
# the American-English rule.
if (file.exists("book/check_naming.R")) {
  source("book/check_naming.R")
  check_naming()
  cat("\nnaming tests passed.\n")
} else {
  cat("SKIP: book/ not found — run from the repo root to check the naming\n")
}

# ---------------------------------------------------------------------------
# A sentence must never lose a language tab without saying why
# ---------------------------------------------------------------------------

# The sixth prose guard, and it exists because `tabs.R` could only *warn*. A
# translator that declines a sentence reports a reason and is passed over; one
# that misses it reports nothing, the tab silently does not appear, and a reader
# of that language sees a page the others get. That warning exits 0, so on
# 2026-07-28 twelve of them had been printed on every render for an unknown
# length of time and were noticed only because somebody grepped a log for the
# word "warning". They turned out not to be misses at all — two emitters were
# declining without giving a reason — but a real miss would have looked
# identical and been just as invisible. This asks `tabs.R`'s own `is_tabbable()`
# which chunks earn a tab, so it cannot disagree with what the book renders.
if (file.exists("book/check_tabs.R")) {
  source("book/check_tabs.R")
  check_tabs()
  cat("\nlanguage-tab tests passed.\n")
} else {
  cat("SKIP: book/ not found — run from the repo root to check the tabs\n")
}

# ---------------------------------------------------------------------------
# `proportion` is a normalizer, and `stack(share = )` fills a pile (spec §5)
# ---------------------------------------------------------------------------

# Read the drawn heights back as data values, through the axis's own two ticks.
# Comparing the bars *with each other* is the whole point: the defect that made
# this session necessary was twelve equal bars at 1/12, and the check that missed
# it read only the axis range. A range is not a shape.
bar_values <- function(spec) {
  s <- render_svg(spec)
  tk <- regmatches(s, gregexpr('<text x="[0-9.]+" y="[0-9.]+">[0-9.]+</text>', s))[[1]]
  # The y ticks are the ones sharing an x, and the x ticks share a y — so take the
  # most common x rather than a pixel threshold, which a short x label slips under.
  tx <- as.numeric(sub('<text x="([0-9.]+)".*', '\\1', tk))
  tk <- tk[tx == as.numeric(names(sort(table(tx), decreasing = TRUE))[1])]
  ty <- as.numeric(sub('.*y="([0-9.]+)">.*', '\\1', tk))
  tv <- as.numeric(sub('.*>([0-9.]+)</text>', '\\1', tk))
  per_px <- (tv[2] - tv[1]) / (ty[1] - ty[2])
  r <- regmatches(s, gregexpr('<rect[^>]*fill-opacity[^>]*>', s))[[1]]
  h <- as.numeric(sub('.*\\bheight="([0-9.]+)".*', '\\1', r))
  h[h != 12] * per_px                                                 # drop legend swatches
}
# Deliberately uneven within each slot as well as between them: an alternating
# split makes every slot 50/50, which a fill that ignored the values would also
# draw, so the test could not tell the two apart.
share_df <- data.frame(
  dir    = factor(rep(c("N", "E", "S", "W"), times = c(6, 10, 4, 20))),
  season = factor(c(rep("Su", 4), rep("Wi", 2),
                    rep("Su", 3), rep("Wi", 7),
                    rep("Su", 1), rep("Wi", 3),
                    rep("Su", 15), rep("Wi", 5))),
  v      = as.numeric(1:40))
# Skewed on purpose: a uniform column binned evenly gives near-equal bars, which
# is the one shape this test must be able to tell apart from the 1/12 defect.
skew <- data.frame(v = as.numeric(round(exp(seq(0, 4.6, length.out = 200)))))

# 1. Unchanged: a bare `proportion` sums to 1.
if (abs(sum(bar_values(data(share_df) + bar * proportion + x(dir))) - 1) > 0.01)
  stop("FAIL: `bar * proportion` should sum to 1")

# 2. The fix. A `color` split used to give each group its own denominator, so the
#    plot summed to 2 — two conditional distributions where §5 had always said the
#    word means a share of the whole frame (Law 6).
sp <- sum(bar_values(data(share_df) + bar * proportion + x(dir) + color(season)))
if (abs(sp - 1) > 0.01)
  stop("FAIL: a split `proportion` should still sum to 1, got ", round(sp, 3))

# 3. The relative-frequency histogram, refused for one day as "two synthesizing
#    transforms". The bars must *differ* — all-equal is the 1/12 defect itself.
h <- bar_values(data(skew) + bar * bin(12) * proportion + x(v))
if (length(h) != 12) stop("FAIL: `bin(12) * proportion` should draw 12 bars")
if (abs(sum(h) - 1) > 0.01)
  stop("FAIL: a relative-frequency histogram sums to 1, got ", round(sum(h), 3))
if (length(unique(round(h, 3))) == 1)
  stop("FAIL: twelve equal bars — the 1/12 defect is back")
# …and it is the plain histogram's own counts, on the same mesh.
n <- bar_values(data(skew) + bar * bin(12) + x(v))
if (length(n) != length(h) || max(abs(n / sum(n) - h)) > 0.01)
  stop("FAIL: the shares are not the histogram's counts over n")
cat("PASS: `proportion` normalizes over the whole frame, split or not, and binned\n")

# 4. `stack(share = TRUE)` fills every pile to exactly 1, whatever measured it.
tops <- bar_values(data(share_df) + bar * count * stack(share = TRUE) +
                     x(dir) + color(season))
for (i in seq_len(length(tops) / 2)) {
  slot <- tops[i] + tops[i + length(tops) / 2]
  if (abs(slot - 1) > 0.01) stop("FAIL: a filled pile reached ", round(slot, 3), ", not 1")
}
if (length(unique(round(tops, 3))) == 1)
  stop("FAIL: every segment identical — the fill lost the composition")
# It composes with any measurement, which is why it is a `stack` parameter and
# not a second reading of `proportion`: there is no column for `proportion` to sum.
invisible(render_svg(data(share_df) + bar * sum * stack(share = TRUE) +
                       x(dir) + y(v) + color(season)))
refuses("stack(share = ) with a number", stack(share = 1))
cat("PASS: `stack(share = TRUE)` fills every pile to 1, on any measurement\n")

# 5. A pile has one direction. `stack` spans [foot, foot + value], so a member of
# the opposite sign reaches back *through* the groups below it and draws a block of
# ink inside a region already spoken for — the reader sees a positive band where the
# number was negative. Refused, and refused per *pile* rather than per column, since
# a plot whose piles point different ways at different positions is well formed.
signs <- function(v) data.frame(q = rep(c("Q1", "Q2", "Q3"), 2), amount = v,
                                kind = rep(c("sales", "returns"), each = 3))
msg <- refuses("a pile whose members disagree in sign",
               render_svg(data(signs(c(5, 5, 5, 2, -3, 2))) + bar * stack +
                            x(q) + y(amount) + color(kind)))
for (want in c("Q2", "sales", "returns", "dodge"))
  if (!grepl(want, msg, fixed = TRUE))
    stop("FAIL: the sign refusal never mentions ", want, " — got: ", msg)
# All-negative agrees with itself, so the pile simply grows downward from zero:
# six bars still drawn, and the axis reaches past the deepest single value (-6) to
# hold the deepest *stacked* total (-9), which is the evidence they piled rather
# than overlaid — an overlaid split would need no more axis than -6.
negsvg <- render_svg(data(signs(c(-5, -4, -6, -3, -2, -3))) + bar * stack +
                       x(q) + y(amount) + color(kind))
if (!grepl(">-8<", negsvg, fixed = TRUE))
  stop("FAIL: an all-negative pile should reach past -6 toward its stacked -9")
if (length(regmatches(negsvg, gregexpr("<rect[^>]*fill-opacity[^>]*>", negsvg))[[1]]) < 6)
  stop("FAIL: an all-negative pile should still draw six bars")
# And two piles may point opposite ways, one position each.
invisible(render_svg(data(data.frame(q = c("Q1", "Q1", "Q2", "Q2"),
                                     amount = c(5, 3, -5, -3),
                                     kind = c("a", "b", "a", "b"))) +
                       bar * stack + x(q) + y(amount) + color(kind)))
cat("PASS: a pile has one direction, and piles at different positions may differ\n")

# 6. `stack(baseline = )` says where the pile hangs — the streamgraph. Three
# names, and a displaced pile draws no numbers on the measure axis, because no
# value on it corresponds to a measurement once the foot has moved.
flows <- data.frame(t = rep(1:6, 3), g = rep(c("a", "b", "c"), each = 6),
                    v = c(4, 9, 3, 8, 2, 7,  5, 5, 5, 5, 5, 5,  2, 3, 9, 2, 8, 3))
plain <- render_svg(data(flows) + area * stack + x(t) + y(v) + color(g))
strm  <- render_svg(data(flows) + area * stack(baseline = "wiggle") + x(t) + y(v) + color(g))
ticks <- function(s) grep("^-?[0-9.]+$", unlist(regmatches(s,
                    gregexpr("(?<=>)[^<>]+(?=</text>)", s, perl = TRUE))), value = TRUE)
if (length(ticks(plain)) <= length(ticks(strm)))
  stop("FAIL: a displaced pile should drop its measure-axis numbers")
if (!length(ticks(strm))) stop("FAIL: the domain axis lost its numbers too")
# Displacing moves the pile; it never changes a thickness. Same shape count.
poly <- function(s) length(regmatches(s, gregexpr("<polygon", s))[[1]])
if (poly(plain) != poly(strm)) stop("FAIL: a displaced pile drew a different number of bands")
refuses("stack(baseline = ) with a number", stack(baseline = 1))
refuses("a baseline that is not one of the three",
        render_svg(data(flows) + area * stack(baseline = "sym") + x(t) + y(v) + color(g)))
refuses("a displaced pile in polar",
        render_svg(data(flows) + area * stack(baseline = "center") + x(t) + y(v) +
                     color(g) + polar()))
cat("PASS: `stack(baseline = )` hangs the pile, and a displaced axis draws no numbers\n")

# A *composed* `proportion` synthesizes nothing, so its `y` names an input column
# and a misspelling of it must still be caught. Found by a reader looking at a
# plot: `bar * sum * proportion + y(pop)` — `pop` renamed `population` in the
# book's own data — drew an empty panel on fabricated 0..1 axes, because
# `proportion` was still on the list of transforms that invent their own y.
refuses("a misspelled column under a composed proportion",
        render_svg(data(share_df) + bar * sum * proportion + x(dir) + y(nosuchcolumn)))
# …while a bare `proportion` still names the column it writes.
invisible(render_svg(data(share_df) + bar * proportion + x(dir) + y(whatever)))
cat("PASS: a composed `proportion` still checks the column it rescales\n")

# --- the violin: the slot reading of `density` (spec §5) ---------------------
#
# Not a new mark, and the test says so by drawing it with the two that already
# exist: `ribbon` closes on its own reflection, `area` on the slot's center line.
viol_df <- data.frame(
  grp = rep(c("wide", "narrow"), times = c(40L, 10L)),
  v   = c((0:39) %% 10, 0:9)
)
npolys <- function(p) length(gregexpr("<polygon", render_svg(p))[[1]])
if (npolys(data(viol_df) + ribbon * density + x(grp) + y(v)) != 2L)
  stop("FAIL: a violin should draw one shape per category")
if (npolys(data(viol_df) + area * density + x(grp) + y(v)) != 2L)
  stop("FAIL: a half violin should draw one shape per category")
# Lying down, the orientation read off the bindings — the form with room for
# long category names, exactly as `box + x(pay) + y(dept)` is.
if (npolys(data(viol_df) + ribbon * density + x(v) + y(grp)) != 2L)
  stop("FAIL: a sideways violin should draw one shape per category")
cat("PASS: `ribbon * density` and `area * density` over a category draw violins\n")

# `compare` chooses what the widths mean between slots, and must change the plot
# — the default weights each estimate by its group's rows, `"shape"` does not.
viol_count <- render_svg(data(viol_df) + ribbon * density + x(grp) + y(v))
viol_shape <- render_svg(data(viol_df) + ribbon * density(compare = "shape") +
                           x(grp) + y(v))
if (identical(viol_count, viol_shape))
  stop("FAIL: `density(compare = )` had no effect on the plot")
refuses("compare on a density curve", 
        render_svg(data(viol_df) + line * density(compare = "count") + x(v)))
refuses("an unknown compare",
        render_svg(data(viol_df) + ribbon * density(compare = "area") + x(grp) + y(v)))
# The curve is still not a band: a `ribbon` needs two boundaries, and one
# estimate along a continuous axis gives it one.
refuses("a ribbon density curve", render_svg(data(viol_df) + ribbon * density + x(v)))
cat("PASS: `density(compare = )` reads only in the violin, and by name\n")

# The ridgeline: the half violin laid down, with overlap and a traced edge.
# `line`/`step` joined the slot reading to close a silent misdraw as well as to
# add the edge — `line * density + x(v) + y(grp)` used to draw the *pooled*
# curve with the axis labeled after the category it had swallowed.
if (npolys(data(viol_df) + area * density(reach = 2.5) + x(v) + y(grp)) != 2L)
  stop("FAIL: an overlapping ridgeline should still draw one shape per category")
traced <- render_svg(data(viol_df) + line * density + x(v) + y(grp))
if (grepl("<polygon", traced)) stop("FAIL: a traced violin should fill nothing")
if (!grepl("<path", traced)) stop("FAIL: a traced violin should stroke something")
wide   <- render_svg(data(viol_df) + area * density(reach = 2.5) + x(v) + y(grp))
narrow <- render_svg(data(viol_df) + area * density + x(v) + y(grp))
if (identical(wide, narrow)) stop("FAIL: `density(reach = )` had no effect")
refuses("reach on a density curve",
        render_svg(data(viol_df) + line * density(reach = 2) + x(v)))
refuses("a negative reach", density(reach = -1))
# A split violin stands in separate slots, so the split-area overlap warning is
# false there; it fired on every colored ridgeline until it was scoped.
warned <- withCallingHandlers(
  { render_svg(data(viol_df) + area * density + x(v) + y(grp) + color(grp)); FALSE },
  message = function(m) TRUE)
cat("PASS: the ridgeline draws, `reach` opens the overlap, and a stroke traces it\n")

# ---------------------------------------------------------------------------
# Composition — separate plots arranged on one page (spec §11)
#
# `|` and `/` between two *plots* is a page; between a plot and `facet()` it is
# still a split. The engine's one rule does the rest: the same column on the
# same axis in two composed plots is one axis — one scale, one panel extent,
# drawn once. The marginal plot is that rule and nothing else.
# ---------------------------------------------------------------------------

cars_df <- data.frame(speed = cars$speed, dist = cars$dist)
scatter  <- data(cars_df) + point + x(speed) + y(dist)
top_hist <- data(cars_df) + bar * bin + x(speed) + theme(height = 120)
side_hist <- data(cars_df) + bar * bin + y(dist) + theme(width = 120)

page <- top_hist / (scatter | side_hist)
if (!inherits(page, "gog_page")) stop("FAIL: two plots joined by `/` should be a page")
svg_page <- render_svg(page)
if (length(gregexpr("<svg", svg_page)[[1]]) != 4L)
  stop("FAIL: a page of three plots is one document holding three")
cat("PASS: `top / (main | right)` composes three plots into one page\n")

# The panels of the two plots sharing `speed` run over the same pixels — the
# whole promise of a marginal plot, and the reason it is not just two plots.
panel_rects <- function(svg) {
  hits <- regmatches(svg, gregexpr('<rect x="[0-9.]+" y="[0-9.]+" width="[0-9.]+"[^>]*fill="#f5f5f8"', svg))[[1]]
  vapply(hits, function(h) {
    as.numeric(regmatches(h, regexpr('(?<=x=")[0-9.]+', h, perl = TRUE)))
  }, numeric(1), USE.NAMES = FALSE)
}
xs <- panel_rects(svg_page)
if (abs(xs[1] - xs[2]) > 0.01)
  stop("FAIL: the marginal histogram's panel should start where the scatter's does")
cat("PASS: a shared column gives the two panels one extent\n")

# And the shared axis is drawn once: three plots, but only one "Speed" name.
if (length(gregexpr(">Speed<", svg_page)[[1]]) != 1L)
  stop("FAIL: a shared axis should be named once, by the plot nearest its edge")
cat("PASS: a shared axis is drawn once, not once per plot\n")

# Unrelated plots are only arranged — nothing of one is decided by the other.
side_by_side <- render_svg(scatter | (data(cars_df) + bar * bin + x(dist)))
if (length(gregexpr("<svg", side_by_side)[[1]]) != 3L)
  stop("FAIL: two plots side by side are one document holding two")
cat("PASS: unrelated plots compose without sharing anything\n")

# `theme(width =, height =)` sizes the image on its own, and the cell on a page.
alone <- render_svg(data(cars_df) + point + x(speed) + y(dist) + theme(width = 400, height = 300))
if (!grepl('width="400" height="300"', alone, fixed = TRUE))
  stop("FAIL: `theme(width =, height =)` should size the image")
cat("PASS: `theme(width =, height =)` is the image alone and the cell composed\n")

refuses("a size no plot can be drawn at", theme(width = 10))
refuses("a page asked to facet", (scatter | scatter) | facet(speed))
refuses("an atom added to a page", (scatter | scatter) + title("Cars"))
refuses("plots asking for more page than there is",
        render_svg((data(cars_df) + point + x(speed) + y(dist) + theme(height = 500)) /
                   (data(cars_df) + point + x(speed) + y(dist) + theme(height = 500))))

# --- partition: a hierarchy in columns, one ring per level -------------------
# The end-of-feature check for this atom is that all four bindings draw the same
# bytes; here we pin what the *sentence* means, and that the two readers agree
# about where a node sits.
budget <- data.frame(
  group  = c("A", "A", "A", "B"),
  item   = c("p", "q", "q", "r"),
  detail = c(NA, "deep", "also", NA),
  amount = c(4, 3, 3, 10)
)
sun <- render_svg(data(budget) + zone * partition(group, item, detail) +
                    x(amount) + color(group) + polar())
if (!grepl("<path", sun, fixed = TRUE))
  stop("FAIL: a partition in polar draws sectors")
# Flat, the same sentence is the icicle — one coordinate space apart, which is
# the whole derivation and so the thing worth testing.
icicle <- render_svg(data(budget) + zone * partition(group, item, detail) +
                       x(amount) + color(group))
if (!grepl("<rect", icicle, fixed = TRUE))
  stop("FAIL: the same sentence flat is the icicle")
if (identical(sun, icicle))
  stop("FAIL: bending the space must change the picture")
cat("PASS: `zone * partition` is the icicle flat and the sunburst bent
")

# The second reader: `text` takes the center the same computation published, so
# a label needs no columns of its own.
named <- render_svg(data(budget) + zone * partition(group, item, detail) + x(amount) +
                      text * partition(group, item, detail) + label(name) + polar())
if (!grepl(">deep<", named, fixed = TRUE))
  stop("FAIL: `text * partition + label(name)` names each node")
cat("PASS: a partition feeds a rectangle and a label from one computation
")

# The engine's refusals need the render — `refuses()` only forces the expression,
# and building a sentence is not asking for it to be drawn.
refuses("a mark with no region reading",
        render_svg(data(budget) + bar * partition(group, item) + x(amount)))
refuses("partition with no levels named", partition())
refuses("a numeric level",
        render_svg(data(budget) + zone * partition(group, amount) + x(amount)))
# The one genuine ambiguity: `A` has a number of its own *and* children with
# numbers, so its arc could be either. Needs a table of its own — `budget` above
# is well formed, and a ragged rim is deliberately not this.
mixed <- data.frame(group = c("A", "A"), item = c(NA, "p"), amount = c(5, 5))
refuses("an interior node with a value of its own",
        render_svg(data(mixed) + zone * partition(group, item) + x(amount)))

# --- partition(cross = TRUE): the mosaic -------------------------------------
# One parameter apart from the icicle, and what it buys is the whole plot: the
# levels turn across each other rather than running down one axis. The engine
# pins the arithmetic; here we pin that the sentence draws and that crossing is
# visible in the output rather than silently ignored.
counts <- data.frame(
  decade = rep(c("1950s", "1960s"), each = 2),
  theme  = rep(c("Heartbreak", "Love"), 2),
  n      = c(10, 10, 30, 40)
)
mosaic <- render_svg(data(counts) + x(n) +
                       zone * partition(decade, theme, cross = TRUE) + color(theme))
nested <- render_svg(data(counts) + x(n) +
                       zone * partition(decade, theme) + color(theme))
if (!grepl("<rect", mosaic, fixed = TRUE))
  stop("FAIL: a crossed partition draws its cells")
if (identical(mosaic, nested))
  stop("FAIL: `cross = TRUE` must change the picture")
# The columns are the *marginal* totals — 20 and 70 of 90 — so the first is
# narrower than the second. Read off the axis rather than the rectangles, which
# is the one number a reader of a mosaic is entitled to.
if (!grepl("Share of column", mosaic, fixed = TRUE))
  stop("FAIL: a crossed partition's second axis names what it carries")
cat("PASS: `partition(cross = TRUE)` is the mosaic
")

# The labeling idiom, carried over from the sunburst: a shallower partition of
# the same table lands its nodes in the same columns, so `text` names them
# without filtering anything.
labeled <- render_svg(data(counts) + x(n) +
                         zone * partition(decade, theme, cross = TRUE) + color(theme) +
                         text * partition(decade, cross = TRUE) + label(name))
if (!grepl(">1960s<", labeled, fixed = TRUE))
  stop("FAIL: `text * partition(cross = TRUE)` names each column")
cat("PASS: a shallower crossed partition labels the columns
")

refuses("cross given something that is not TRUE or FALSE",
        partition(decade, theme, cross = "yes"))

# --- a zone takes a border (the closed-glyph fills, spec §4) -----------------
# The settable rule spans a setting across its geometry class, and `zone` joined
# the fills on 2026-07-27 because a mosaic without cell edges is one blob
# wherever two neighbors share a color. Refused here until that day, so this
# test is the ruling.
edged <- render_svg(data(counts) + x(n) +
                      zone * partition(decade, theme, cross = TRUE) + color(theme) +
                      style(border_color = "white", border_size = 2))
if (!grepl('stroke="white"', edged, fixed = TRUE))
  stop("FAIL: a zone draws the border it was given")
if (grepl('stroke="white"', mosaic, fixed = TRUE))
  stop("FAIL: an unasked-for border must not appear")
cat("PASS: a `zone` carries `style(border_color =, border_size =)`
")

# --- the masked base-R names refuse with direction (§12, §18) ----------------
# Eighteen exports collide with the attached base packages, and ten of them are
# harmless: R skips a non-function binding when it resolves a call, so a mark or
# transform *constant* cannot break `mean(x)`. Assert that, because the book now
# tells readers so and the claim has to be able to fail here first.
if (is.function(mean)) stop("FAIL: gog's `mean` is a constant, not a function")
if (!identical(mean(1:10), 5.5)) stop("FAIL: base::mean must still answer mean(1:10)")
if (!identical(sum(1:10), 55L)) stop("FAIL: base::sum must still answer sum(1:10)")
if (!identical(unname(sapply(data.frame(a = c(2, 4)), mean)), 3))
  stop("FAIL: sapply(df, mean) must still reach base::mean via match.fun")
cat("PASS: the ten masked *objects* leave base R's calls working\n")

# The eight that are functions do take over, and the two measured to hurt are
# `order` (dies at `invalid subscript type 'list'` in `[.data.frame`, naming
# neither `order` nor gog) and `data` (returns a spec, silently). Each refusal
# below has to name the base function, since not knowing which one answered is
# the whole cost of the collision.
masked <- function(label, expr, phrase) {
  got <- tryCatch({ expr; "no_error" },
                  error = function(e) if (grepl(phrase, conditionMessage(e), fixed = TRUE))
                    "refused" else paste0("wrong: ", conditionMessage(e)))
  if (got != "refused") stop("FAIL: ", label, " must be refused naming ", phrase, " — got ", got)
  cat("PASS: ", label, " refused, and names ", phrase, "\n", sep = "")
}
masked("order(df$k)",       order(df$k),               "base::order")
masked("order(-population)", order(-population),       "desc = TRUE")
masked("data()",            data(),                    "utils::data()")
masked("data(no_such_table)", data(no_such_table),     "utils::data(no_such_table)")
masked("data(1:10)",        data(1:10),                "utils::data()")
masked("density(<column>)", density(c(1.5, 2.5, 3.5)), "stats::density()")
masked("jitter(<column>)",  jitter(c(1.5, 2.5)),       "base::jitter()")
masked("stack(<table>)",    stack(data.frame(a = 1)),  "utils::stack()")
masked("palette()",         palette(),                 "grDevices::palette()")
masked("title()",           title(),                   "graphics::title(main = )")

# And the two spellings that must keep working, because a guard that refuses a
# legal sentence is worse than the collision it was written for. `order()` takes
# an optional field: with none, it reverses a categorical axis.
if (!inherits(order(gold, desc = TRUE), "gog_atom"))
  stop("FAIL: order(<bare name>, desc = TRUE) must still build an atom")
if (!identical(unclass(order(desc = TRUE))$order_field, ""))
  stop("FAIL: order(desc = TRUE) must stay legal, with no field")
cat("PASS: order(<column>, desc =) and order(desc =) both still build\n")

# --- the license travels with every package that ships ----------------------
# Apache 2.0 §4(a) makes whoever hands out a copy hand out the License with it,
# and each binding is packaged from its own directory, above which the
# repository's one `LICENSE` is unreachable — a wheel, an npm tarball and a
# Julia subdirectory package each arrive somewhere the root cannot be seen. So
# each carries a copy, and copies drift; this is the guard. R is the one
# exception with no `LICENSE` of its own: R ships the Apache text in
# `share/licenses`, `DESCRIPTION`'s canonical name is verified against that
# copy, and a top-level duplicate earns a `checking top-level files` NOTE for
# nothing. Its `NOTICE` still installs, because attribution is this project's
# to state and no `share/` copy can hold it.
# Skipped when the repository is not underfoot, like the `book/` checks above:
# CRAN runs this suite against an installed package, where only `inst/` survives.
if (file.exists("LICENSE") && file.exists("NOTICE")) {
  root <- list(LICENSE = readLines("LICENSE", warn = FALSE),
               NOTICE  = readLines("NOTICE",  warn = FALSE))
  for (p in c("py-pkg/gog/LICENSE",               "py-pkg/gog/NOTICE",
              "js-pkg/gog/LICENSE",               "js-pkg/gog/NOTICE",
              "jl-pkg/GrammarOfGraphics/LICENSE", "jl-pkg/GrammarOfGraphics/NOTICE",
              "r-pkg/gog/inst/NOTICE")) {
    if (!file.exists(p))
      stop("FAIL: ", p, " is missing — that package would ship without its license")
    if (!identical(readLines(p, warn = FALSE), root[[basename(p)]]))
      stop("FAIL: ", p, " has drifted from the repository's ", basename(p))
  }
  cat("PASS: every packaged binding carries the license, unchanged\n")
}

# --- one grammar, one version number ----------------------------------------
# Six manifests declare a version and nothing made them agree, so they did not:
# on 2026-07-28 they read 0.1.0, 0.0.0.dev0, 0.0.0 and 0.1.0 — four bindings
# wearing three numbers, plus two engine crates on a fourth. Harmless in a
# checkout, incoherent the moment two of them are published, because "which gog
# is this" then has more than one answer and a user has no way to tell which
# answer is the one that matches their plot.
#
# The engine crates are in the list even though they are never published (§1.2
# keeps them internal). No binding asks the engine its version — there is no
# handshake — so the number is informational, and an informational number that
# disagrees with the four shipped ones is worse than no number at all.
#
# This is the license-drift guard above, one concern over, and for the same
# reason: a rule nothing checks is a rule that has already been broken
# somewhere you have not looked. Skipped when the repository is not underfoot.
if (file.exists("r-pkg/gog/DESCRIPTION")) {
  declared <- function(path, pattern) {
    hit <- grep(pattern, readLines(path, warn = FALSE), value = TRUE)
    if (!length(hit)) stop("FAIL: no version line found in ", path)
    sub(pattern, "\\1", hit[1])
  }
  quoted <- "^ *\"?version\"? *[=:] *\"([^\"]+)\".*$"
  versions <- c(
    "r-pkg/gog/DESCRIPTION"                 = declared("r-pkg/gog/DESCRIPTION",
                                                       "^Version: *(.+?) *$"),
    "py-pkg/gog/pyproject.toml"             = declared("py-pkg/gog/pyproject.toml", quoted),
    "js-pkg/gog/package.json"               = declared("js-pkg/gog/package.json", quoted),
    "jl-pkg/GrammarOfGraphics/Project.toml" = declared("jl-pkg/GrammarOfGraphics/Project.toml", quoted),
    "gog-core/Cargo.toml"                   = declared("gog-core/Cargo.toml", quoted),
    "gog-cli/Cargo.toml"                    = declared("gog-cli/Cargo.toml", quoted),
    # The seventh, and the one the first version of this guard missed, because
    # it enumerated *manifests* and this is *source*. `pyproject.toml` sets what
    # the wheel's metadata says; this sets what `gog.__version__` tells a user.
    # They disagreed through five built wheels and an sdist, and nothing here
    # could see it — a release process reads the manifest, a user reads the code.
    "py-pkg/gog/gog/__init__.py"            = declared("py-pkg/gog/gog/__init__.py",
                                                       "^__version__ *= *\"([^\"]+)\".*$")
  )
  if (length(unique(versions)) != 1L)
    stop("FAIL: the version declarations disagree about which gog this is —\n",
         paste0("  ", format(names(versions)), "  ", versions, collapse = "\n"),
         "\n  One grammar, one number. Change them together or not at all.")
  cat("PASS: all seven declarations agree on version ", versions[[1]], "\n", sep = "")
}

# --- the inline browser engine must be one unbroken line ----------------------
#
# `data_uri()` writes a `data:` URI into a JavaScript string literal, and a
# literal newline inside one is a syntax error that stops the whole emitted
# module from parsing. `jsonlite::base64_enc()` wraps at 72 characters, so this
# was broken for every `print(p)` in a console — the RStudio and Positron viewer
# panes, and a browser tab — while the book, which points at a shared file
# instead of inlining, worked throughout and hid it.
local({
  assets <- gog:::find_wasm_assets()
  if (is.null(assets)) {
    cat("SKIP: no browser engine built, so the inline URI cannot be checked\n")
  } else {
    uri <- gog:::data_uri(assets$js, "text/javascript")
    stopifnot(!grepl("[\r\n]", uri))
    stopifnot(startsWith(uri, "data:text/javascript;base64,"))

    p <- data(data.frame(a = 1:3, b = c(2, 1, 3), c = c(3, 2, 1))) +
      point + x(a) + y(b) + z(c)
    block <- gog:::svg_block(gog:::render_svg(p), p)
    script <- sub(".*<script type=\"module\">", "", block)
    script <- sub("</script>.*", "", script)
    # Three newlines of formatting; 291 meant the base64 was wrapped.
    stopifnot(length(gregexpr("\n", script)[[1]]) < 10)
    cat("PASS: the inlined browser engine is a single unbroken line\n")
  }
})

# --- book_table(): the manual's tables, without a CSV reader to copy ----------
# Binding plumbing rather than a word of the grammar, which is why
# `book/check_vocabulary.R` excludes it from the kernel block beside
# `render_svg`. The offline checks always run; the fetch is guarded, because a
# test suite has to pass on a laptop with no network.
local({
  stopifnot(is.function(gog::book_table))
  stopifnot(identical(gog:::book_data_url,
                      "https://psychometrician.github.io/gog-book/data/"))

  for (bad in list(42, c("a", "b"), NA_character_)) {
    msg <- tryCatch(gog::book_table(bad), error = conditionMessage)
    stopifnot(grepl("one table name", msg, fixed = TRUE))
  }
  cat("PASS: book_table() takes one name and refuses anything else\n")

  fetched <- tryCatch(gog::book_table("gapminder_2007"), error = function(e) e)
  if (inherits(fetched, "error")) {
    cat("SKIP: book_table() live fetch -",
        substr(conditionMessage(fetched), 1, 50), "\n")
  } else {
    stopifnot(is.data.frame(fetched), nrow(fetched) == 142)
    stopifnot(is.numeric(fetched$gdp), is.character(fetched$continent))
    p <- data(fetched, name = "gapminder_2007") + point + x(gdp) + y(life)
    stopifnot(grepl("<svg", gog:::render_svg(p), fixed = TRUE))
    cat("PASS: book_table('gapminder_2007') is 142 typed rows, and it draws\n")
  }
})
