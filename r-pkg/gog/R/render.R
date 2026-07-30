# render.R — SVG rendering and display

# ---------------------------------------------------------------------------
# Find the gog-cli binary
# ---------------------------------------------------------------------------

# The engine shipped inside this package, if this is an installed copy.
#
# `configure` puts the binary at `inst/bin/` before installation, so an
# installed package carries the engine and draws on a machine with no Rust
# toolchain and no checkout — the long-standing blocker on releasing this
# binding, and the one Python closed first by putting `gog-cli` in the wheel.
#
# **A development checkout can have one too, and then it wins.** This comment
# used to claim that a `pkgload::load_all()` copy has no `bin/` and falls through
# to the `target/` build below — true until `configure` started writing here, and
# false the moment the pre-commit install gate has been run once: the copy it
# leaves behind is preferred over `target/release` by the ordering below, so the
# dev loop silently draws with whatever engine was current when the gate last
# ran. It cost a session's first round of verification on 2026-07-26. The file is
# gitignored and rebuilt at install time, so `rm -rf r-pkg/gog/inst/bin` is the
# fix; `GOG_CLI_PATH` is the way to override it without deleting anything.
#
# The executable bit is restored if it went missing. `R CMD INSTALL` preserves
# the mode, but a tarball copied by a tool that does not would produce
# "Permission denied" from a subprocess, which says nothing about what to do.
bundled_gog_cli <- function() {
  exe <- if (.Platform$OS.type == "windows") "gog-cli.exe" else "gog-cli"
  binary <- system.file("bin", exe, package = "gog")
  if (!nzchar(binary) || !file.exists(binary)) return(NULL)
  if (.Platform$OS.type != "windows" && file.access(binary, mode = 1L) != 0L) {
    ok <- tryCatch(Sys.chmod(binary, "0755", use_umask = FALSE),
                   error = function(e) FALSE)
    if (!isTRUE(ok)) return(NULL)
  }
  binary
}

# The engine built from the crate sources this package is sitting beside, if it
# is sitting beside any — `<root>/target/{release,debug}/gog-cli` for a checkout
# whose `<root>/Cargo.toml` is the workspace the package lives inside.
#
# **This is what makes the development loop correct rather than lucky.** `configure`
# writes the bundled engine into `inst/bin/` of the *source* tree, so running the
# mandated pre-commit install gate leaves a copy sitting in `pkgload::load_all()`'s
# path — and the moment the workspace is rebuilt, that copy is stale. Before this
# existed the gate silently poisoned the dev loop every time it ran: two rounds of
# verification on 2026-07-26 measured a binary from earlier the same afternoon, one
# of them after the trap had already been found and written down once.
#
# The test that tells a checkout from an installed copy is a **fact, not an mtime**,
# the same one `configure` uses to pick its own source: a release tarball has no
# Cargo workspace beside it, and a checkout does. So this can safely outrank the
# bundled engine — it only ever fires where the bundled engine is a build artifact.
# Walk up from `start` looking for a Cargo workspace with a built engine in it:
# a directory holding both `Cargo.toml` and `target/{release,debug}/gog-cli`.
#
# Both conditions, and the `Cargo.toml` is the one that carries the meaning — it
# is what tells a checkout from an installed copy. Walked rather than counted
# because the distance to the root is not fixed: `system.file(package = "gog")`
# answers `<pkg>/inst` under `pkgload::load_all()` and `<lib>/gog` when installed,
# so any fixed number of `..`s is right in one case and wrong in the other. That
# mistake cost a debugging round on 2026-07-26 before this was walked.
walk_up_for_engine <- function(start, levels = 6L) {
  exe <- if (.Platform$OS.type == "windows") "gog-cli.exe" else "gog-cli"
  root <- normalizePath(start, winslash = "/", mustWork = FALSE)
  for (i in seq_len(levels + 1L)) {
    if (file.exists(file.path(root, "Cargo.toml"))) {
      for (build in c("release", "debug")) {
        candidate <- file.path(root, "target", build, exe)
        if (file.exists(candidate)) return(candidate)
      }
    }
    parent <- dirname(root)
    if (identical(parent, root)) break
    root <- parent
  }
  NULL
}

workspace_gog_cli <- function() {
  pkg <- system.file(package = "gog")
  if (!nzchar(pkg)) return(NULL)
  walk_up_for_engine(pkg)
}

find_gog_cli <- function() {
  # 1. Explicit env var (recommended for development)
  env_path <- Sys.getenv("GOG_CLI_PATH", unset = "")
  if (nzchar(env_path) && file.exists(env_path)) return(env_path)

  # 2. A workspace build, when this package is sitting in a checkout. Above the
  #    bundled engine deliberately: in a checkout the bundled copy is a build
  #    artifact `configure` left behind, and preferring it is how a stale engine
  #    gets served to the dev loop. See `workspace_gog_cli`.
  workspace <- workspace_gog_cli()
  if (!is.null(workspace)) return(workspace)

  # 3. The engine that shipped with this package — before PATH, because the
  #    binary installed alongside the package is the one whose wire format
  #    matches it. An unrelated `gog-cli` earlier on PATH would otherwise
  #    silently answer for it. Same ordering rule as `py-pkg`'s, same reason.
  bundled <- bundled_gog_cli()
  if (!is.null(bundled)) return(bundled)

  # 4. On PATH
  on_path <- Sys.which("gog-cli")
  if (nzchar(on_path)) return(on_path)

  # 5. A local build reached from the working directory rather than from the
  #    package — same walk, different starting point, so a checkout used without
  #    installing at all still finds `target/` from wherever the plot is drawn.
  from_wd <- walk_up_for_engine(getwd())
  if (!is.null(from_wd)) return(from_wd)

  example <- if (.Platform$OS.type == "windows") {
    "  Sys.setenv(GOG_CLI_PATH = 'C:/path/to/gog-cli.exe')\n"
  } else {
    "  Sys.setenv(GOG_CLI_PATH = '/path/to/gog/target/release/gog-cli')\n"
  }
  stop(
    "gog: cannot find the `gog-cli` binary \u2014 the engine that draws the plot.\n",
    "An installed copy of gog carries its own; this one does not, so either\n",
    "it was installed without one or this is a development checkout.\n",
    "  Build it:  cargo build --release -p gog-cli\n",
    "  Or point at one:\n",
    example,
    call. = FALSE
  )
}

# ---------------------------------------------------------------------------
# Data serialization
# ---------------------------------------------------------------------------

# A factor is a category column that remembers what order its categories go in.
# `factor(x, levels = c("Low", "Medium", "High"))` is the normal way an R user
# says so, and until this shipped the levels were dropped here: `is.numeric()` is
# FALSE for a factor, so it fell to `as.character()`, which keeps only the words.
# The chart then fell back to the order of the rows and said nothing about it.
#
# Both kinds of factor count. `ordered = TRUE` means "Low really is less than
# High", but plenty of people use a plain factor purely to fix display order and
# mean nothing mathematical by it — honoring only the ordered ones would leave
# the commoner case silently broken, which is the bug being fixed.
# A date column has the same wire problem a factor had: `is.numeric()` is FALSE
# for both `Date` and `POSIXct`, so both used to fall through to
# `as.character()` and arrive at the engine as category strings — "2026-01-02"
# was a label on a bar, not a moment on an axis, and nothing said so.
#
# The engine's one temporal unit is seconds since 1970-01-01. A `Date` is days
# since that epoch, so it is multiplied out here (exactly — a double holds any
# realistic date-in-seconds without rounding); a `POSIXct` already is seconds.
# The `dates` map carries what the values cannot: the declared resolution,
# which is what stops a column of dates growing ticks at 06:00.
df_to_wire <- function(df) {
  floats  <- stats::setNames(list(), character())
  strings <- stats::setNames(list(), character())
  levels_ <- stats::setNames(list(), character())
  dates   <- stats::setNames(list(), character())
  for (col in names(df)) {
    v <- df[[col]]
    if (inherits(v, "Date")) {
      floats[[col]] <- as.numeric(v) * 86400
      dates[[col]]  <- "day"
    } else if (inherits(v, "POSIXt")) {
      # The engine is timezone-naive on purpose: it draws the clock time the
      # user sees. format() renders each moment in the column's own zone
      # (DST and all); re-reading that as UTC makes the reading *be* the
      # number, so no zone survives the trip to disagree with it later.
      floats[[col]] <- as.numeric(as.POSIXct(format(v, "%Y-%m-%d %H:%M:%S"), tz = "UTC"))
      dates[[col]]  <- "second"
    } else if (is.factor(v)) {
      strings[[col]] <- as.character(v)
      levels_[[col]] <- as.character(levels(v))
    } else if (is.numeric(v) || is.integer(v)) {
      floats[[col]] <- as.numeric(v)
    } else {
      strings[[col]] <- as.character(v)
    }
  }
  # A column is an array even when it holds one value. `toJSON(auto_unbox =
  # TRUE)` is needed for the scalar *spec* fields (a title, a start angle) and
  # cannot tell them from a length-1 *column*, so it unboxed `[30000]` to
  # `30000` and the engine rejected the frame outright: `invalid type: integer
  # 30000, expected a sequence`. Any single-row table was unrenderable, and so
  # was a single-level factor (`levels`), which is why the fix wraps all three
  # array maps rather than only the one that surfaced it. `I()` marks a vector
  # AsIs, which is jsonlite's own opt-out from unboxing; for length > 1 it
  # changes nothing, so the wire bytes of every existing plot are unaffected.
  #
  # `dates` is deliberately not wrapped: it maps a column name to *one* string
  # ("day" / "second"), a scalar the engine reads as `HashMap<String, TimeUnit>`.
  # It is the one map here whose values are not columns.
  list(
    floats  = lapply(floats,  I),
    strings = lapply(strings, I),
    levels  = lapply(levels_, I),
    dates   = dates
  )
}

# ---------------------------------------------------------------------------
# Render to SVG string
# ---------------------------------------------------------------------------

#' Render a gog_spec to an SVG string.
#'
#' @param gog  A \code{gog_spec} object built with \code{data() + ...}.
#' @return A character string containing the SVG.
#' @export
render_svg <- function(gog) {
  # A page and a plot are one wire format (`ir::Figure`): the engine tells them
  # apart by their own required fields, so this is the only line that has to
  # know which it was handed.
  wire_spec <- if (inherits(gog, "gog_page")) {
    gog$page
  } else {
    gog <- finalize_spec(gog)
    gog$spec
  }

  # A `query()` table is resolved here and nowhere else — one place, at render,
  # which is what leaves room for the planner to rewrite the sentence before the
  # database is ever asked (the pushdown design).
  frames <- mapply(resolve_query, gog$data_frames, names(gog$data_frames),
                   SIMPLIFY = FALSE)
  wire_data <- lapply(frames, df_to_wire)

  request <- list(spec = wire_spec, data = wire_data)

  # digits = NA means full double precision. jsonlite's *default* is 4 decimal
  # places, which silently rounded every number on its way to the engine:
  # 0.000123456 arrived as 0.0001, a 23% error, and any column of small values —
  # rates, probabilities, proportions — was quietly mangled. It went unnoticed
  # because the usual example data (incomes, populations, medal counts) is large
  # enough that four decimals are invisible. Found when `base = exp(1)` arrived
  # as 2.7183 and stopped being recognizable as e.
  # na = "null": an R `NA` crosses the wire as JSON `null`, not the string "NA"
  # jsonlite emits by default — a numeric column with a gap (`penguins`'
  # flipper_length_mm has two) would otherwise arrive as `["NA"]` and fail the
  # engine's `f64` parse. The engine reads `null` as missing and drops the row
  # from the columns the plot maps, reporting how many; representation is the
  # binding's job, the drop policy is the engine's.
  json_str <- jsonlite::toJSON(
    request,
    auto_unbox = TRUE,
    null       = "null",
    na         = "null",
    force      = TRUE,
    digits     = NA
  )

  cli_path <- find_gog_cli()

  stderr_file <- tempfile()
  on.exit(unlink(stderr_file), add = TRUE)

  # suppressWarnings: a non-zero exit is a legality refusal we report ourselves,
  # not an R-level warning worth showing on top of the real message.
  result <- suppressWarnings(
    system2(cli_path, stdout = TRUE, stderr = stderr_file, input = json_str)
  )

  msgs <- if (file.exists(stderr_file)) {
    readLines(stderr_file, warn = FALSE)
  } else {
    character()
  }

  status <- attr(result, "status")
  if (!is.null(status) && status != 0L) {
    # The diagnostics ARE the error. Surface them as-is rather than wrapping
    # them in an exit-code message that says nothing actionable.
    stop(paste(msgs, collapse = "\n"), call. = FALSE)
  }

  # Non-fatal diagnostics (assumptions, warnings) go to the message stream so
  # they appear in the console without corrupting the SVG on stdout.
  if (length(msgs) > 0L) message(paste(msgs, collapse = "\n"))

  paste(result, collapse = "\n")
}

# ---------------------------------------------------------------------------
# Inline SVG for HTML hosts — knitr, Jupyter
# ---------------------------------------------------------------------------

# Two hosts embed the same HTML, so the wrapper lives here rather than staying
# inside whichever method wrote it first. The engine always draws an 800x600
# canvas (Layout's defaults in render/svg.rs); the style attribute is what lets
# that canvas shrink into a narrow column instead of overflowing it.
svg_block <- function(svg_str) {
  svg_str <- sub(
    'width="800" height="600"',
    'width="800" height="600" style="max-width:100%;height:auto;"',
    svg_str,
    fixed = TRUE
  )
  paste0('\n<div class="gog-plot" style="text-align:center;">\n',
         svg_str,
         '\n</div>\n')
}

# A plot on a LaTeX page — the counterpart of `svg_block`, and the reason a PDF
# build used to be text-only.
#
# knitr cannot embed an SVG in LaTeX (there is no `\includegraphics` driver for
# one), so this writes the plot to a file and converts it to **PDF** — vector in,
# vector out, so the page keeps the resolution the SVG had. `rsvg-convert` is the
# converter the project already uses for raster checks, and it is
# looked for the way `find_gog_cli` looks for the engine: told where it is, then on
# `PATH`. If it is absent the plot is *named as missing* rather than silently
# dropped, because a figure that vanished with the build still exiting 0 is the
# failure this project refuses everywhere else.
#
# The files go where knitr already puts figures (`fig.path`), so `quarto render`
# cleans them up with the rest of the build rather than leaving them in the tree.
latex_block <- function(svg_str, label) {
  conv <- Sys.getenv("RSVG_CONVERT_PATH", unset = "")
  if (!nzchar(conv) || !file.exists(conv)) conv <- Sys.which("rsvg-convert")
  dir <- knitr::opts_chunk$get("fig.path")
  if (is.null(dir) || !nzchar(dir)) dir <- "gog-figure/"
  dir.create(dir, recursive = TRUE, showWarnings = FALSE)
  # **Absolute, because the two programs do not share a working directory.**
  # knitr evaluates a chunk from the document's own folder (a book's mark chapters
  # sit one level down), and LaTeX then runs from wherever the `.tex` was written.
  # A relative `\includegraphics` therefore resolves against the wrong folder for
  # every chapter that is not at the top, and LaTeX's answer to a missing figure is
  # a *draft box* plus a warning — a plot silently replaced by an empty rectangle,
  # which is the failure this project refuses. An absolute path cannot be read from
  # the wrong place.
  dir <- normalizePath(dir, winslash = "/", mustWork = FALSE)

  svg_file <- file.path(dir, paste0(label, ".svg"))
  writeLines(svg_str, svg_file)

  if (!nzchar(conv)) {
    return(paste0(
      "\n\\begin{center}\\fbox{\\texttt{", label, ".svg} \u2014 install \\texttt{rsvg-convert} ",
      "to place this plot on the page}\\end{center}\n\n"
    ))
  }

  pdf_file <- file.path(dir, paste0(label, ".pdf"))
  status <- suppressWarnings(system2(conv, c("-f", "pdf", shQuote(svg_file),
                                             "-o", shQuote(pdf_file)),
                                     stdout = FALSE, stderr = FALSE))
  if (!identical(status, 0L) || !file.exists(pdf_file)) {
    return(paste0("\n\\begin{center}\\fbox{\\texttt{", label,
                  "} \u2014 could not be converted for the page}\\end{center}\n\n"))
  }
  # `\linewidth`, not a fixed size: the engine draws one 800x600 canvas and the
  # page decides how wide that is, which is the same job `svg_block`'s
  # `max-width:100%` does for a browser column.
  paste0("\n\\begin{center}\\includegraphics[width=\\linewidth]{",
         pdf_file, "}\\end{center}\n\n")
}

# A stable, unique file name per plot. knitr numbers within a chunk, so a chunk
# drawing four plots (the surface chapter's angle tour) needs the counter as well
# as the label, or the four would overwrite one another and the page would show the
# last one four times.
.gog_fig_seq <- new.env(parent = emptyenv())

gog_fig_label <- function() {
  chunk <- knitr::opts_current$get("label")
  if (is.null(chunk) || !nzchar(chunk)) chunk <- "gog"
  chunk <- gsub("[^A-Za-z0-9]+", "-", chunk)
  seen <- .gog_fig_seq[[chunk]]
  n <- if (is.null(seen)) 1L else seen + 1L
  .gog_fig_seq[[chunk]] <- n
  paste0(chunk, "-", n)
}

#' knitr print method: the plot inline as SVG in HTML, and as a converted
#'
#' @param x The plot or page being displayed.
#' @param ... Passed on by the display host; unused.
#' vector figure on a LaTeX page.
#' Called automatically by knitr when a gog_spec is the last expression
#' in a code chunk.
knit_print.gog_spec <- function(x, ...) {
  svg_str <- render_svg(x)

  if (isTRUE(knitr::is_latex_output())) {
    return(knitr::asis_output(latex_block(svg_str, gog_fig_label())))
  }

  knitr::asis_output(svg_block(svg_str))
}

# ---------------------------------------------------------------------------
# repr — inline SVG for Jupyter notebooks (IRkernel)
# ---------------------------------------------------------------------------

# Jupyter is the third host that has to be taught how to show a plot, after the
# RStudio viewer (print) and knitr (knit_print). IRkernel asks the `repr`
# package for a mime bundle, so a plot appears in a notebook cell only if
# `repr_html` has a method for it.
#
# `repr_text` matters just as much and far less obviously. IRkernel always puts
# text/plain in the bundle as the fallback, and `repr_text.default` builds it by
# calling `print()` — which for a gog_spec means render_and_display(), which
# opens a *browser window*. Without this method every notebook cell would draw
# its plot inline and pop a browser tab beside it. The text form is never the
# one displayed while text/html is in the bundle, so it only has to say what the
# plot is.

#' repr method: the plot as inline SVG in a Jupyter cell.
#'
#' @param obj The plot or page being displayed.
#' @param ... Passed on by the display host; unused.
repr_html.gog_spec <- function(obj, ...) svg_block(render_svg(obj))

# A page draws through the very same methods — it is a figure like any other,
# and every host (knitr, Jupyter, the viewer) asks the same question of it.
# Registered rather than inherited because S3 dispatch is by class, and a page
# is not a plot.

#' knitr print method for a composed page.
#'
#' @param x The plot or page being displayed.
#' @param ... Passed on by the display host; unused.
knit_print.gog_page <- knit_print.gog_spec

#' repr method: a composed page as inline SVG in a Jupyter cell.
#'
#' @param obj The plot or page being displayed.
#' @param ... Passed on by the display host; unused.
repr_html.gog_page <- repr_html.gog_spec

#' repr method: a one-line description of a composed page.
#'
#' @param obj The plot or page being displayed.
#' @param ... Passed on by the display host; unused.
repr_text.gog_page <- function(obj, ...) {
  paste0("<gog page: ", length(obj$page$cells), " cells, ",
         obj$page$arrange, ">")
}

#' repr method: a one-line description for the text/plain fallback.
#'
#' @param obj The plot or page being displayed.
#' @param ... Passed on by the display host; unused.
repr_text.gog_spec <- function(obj, ...) {
  obj   <- finalize_spec(obj)
  marks <- unlist(lapply(obj$spec$layers, function(l) l$mark))
  paste0("<gog plot: ",
         if (length(marks)) paste(marks, collapse = " + ") else "no mark",
         " on ", obj$spec$data, ">")
}

# Register the foreign-generic methods so they are found when the package is
# source()-d rather than installed — which is how the book and the test suite
# load it. NAMESPACE covers the installed case.
if (requireNamespace("knitr", quietly = TRUE)) {
  registerS3method("knit_print", "gog_spec", knit_print.gog_spec,
                   envir = asNamespace("knitr"))
  registerS3method("knit_print", "gog_page", knit_print.gog_page,
                   envir = asNamespace("knitr"))
}
if (requireNamespace("repr", quietly = TRUE)) {
  registerS3method("repr_html", "gog_spec", repr_html.gog_spec,
                   envir = asNamespace("repr"))
  registerS3method("repr_text", "gog_spec", repr_text.gog_spec,
                   envir = asNamespace("repr"))
  registerS3method("repr_html", "gog_page", repr_html.gog_page,
                   envir = asNamespace("repr"))
  registerS3method("repr_text", "gog_page", repr_text.gog_page,
                   envir = asNamespace("repr"))
}

# ---------------------------------------------------------------------------
# Display in the notebook, the RStudio viewer, or the default browser
# ---------------------------------------------------------------------------

render_and_display <- function(gog) {
  svg_str <- render_svg(gog)

  # Under Jupyter, hand the SVG to the kernel. Auto-display of a cell's last
  # value goes through repr_html above and never reaches here, but an explicit
  # print(p) does — and so does the idiom for showing several plots at once,
  # `for (p in plots) print(p)`. Without this branch both would write a temp
  # file and open a browser tab next to the notebook.
  if ("IRkernel" %in% loadedNamespaces()) {
    IRdisplay::display_html(svg_block(svg_str))
    return(invisible(svg_str))
  }

  html <- paste0(
    "<!DOCTYPE html>\n<html>\n",
    "<head><meta charset='utf-8'>",
    "<style>body{margin:0;background:#fff;display:flex;",
    "justify-content:center;padding:16px;}</style></head>\n",
    "<body>\n", svg_str, "\n</body>\n</html>"
  )

  tmp <- tempfile(fileext = ".html")
  writeLines(html, tmp)

  viewer <- getOption("viewer")
  if (!is.null(viewer)) {
    viewer(tmp)           # RStudio viewer pane
  } else {
    utils::browseURL(tmp) # system default browser
  }

  invisible(svg_str)
}
