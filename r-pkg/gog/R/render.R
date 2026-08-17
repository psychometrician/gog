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

# The engine's input for a plot or a page, as JSON — the one place either is
# turned into what `gog-cli` reads.
#
# Split out of `render_svg()` when `save_gif()` became a second caller. Two
# functions serializing the same object is two chances to disagree about a
# number's precision or about what a missing value crosses as, and that
# disagreement would surface as a GIF that does not match the plot beside it in
# the book — the one difference no amount of comparing SVG could catch.
wire_json <- function(gog) {
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
  jsonlite::toJSON(
    request,
    auto_unbox = TRUE,
    null       = "null",
    na         = "null",
    force      = TRUE,
    digits     = NA
  )
}

#' Render a gog_spec to an SVG string.
#'
#' @param gog  A \code{gog_spec} object built with \code{data() + ...}.
#' @return A character string containing the SVG.
#' @export
render_svg <- function(gog) {
  json_str <- wire_json(gog)

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

  # `stdout = TRUE` hands back *lines*, with their separators removed, so
  # rejoining them cannot reproduce a trailing newline. The engine writes the
  # SVG with `print!` and adds nothing, which means that newline is part of the
  # document rather than punctuation from the shell: Python, JavaScript and
  # Julia all return it, and R alone returned a file one byte shorter than the
  # other three drew. The law this package is held to is byte-identical output,
  # so one byte is the whole of it.
  #
  # Restored rather than stripped everywhere else, because three bindings
  # passing the engine's bytes through untouched is the behavior worth keeping,
  # and R reconstructing them is the outlier.
  svg <- paste(result, collapse = "\n")
  if (nzchar(svg)) paste0(svg, "\n") else svg
}

# ---------------------------------------------------------------------------
# Write a played plot as a file that moves
# ---------------------------------------------------------------------------

#' Write a played plot to an animated GIF.
#'
#' A plot that binds \code{play()} moves in a browser, because the SVG carries
#' its own timing. Most other places do not read that: a message to a friend, a
#' slide, a post. This writes the same sequence as a GIF, which they do read.
#'
#' The frames come out of the one renderer, so the file cannot disagree with the
#' plot. Every scale, the color map and each legend are fitted across the whole
#' sequence at once, and the moments are cut from that single drawing rather
#' than drawn again one at a time.
#'
#' Nothing needs to be installed. The engine converts and encodes on its own.
#'
#' @param gog   A played \code{gog_spec} — one that binds \code{play()}.
#' @param path  Where to write, ending in \code{.gif}.
#' @param scale Multiplier on the plot's canvas. A plot is 800 by 600 unless its
#'   theme says otherwise, which is small for a post; \code{scale = 2} doubles it.
#' @return The path, invisibly.
#' @export
save_gif <- function(gog, path, scale = 1) {
  if (!is.character(path) || length(path) != 1L || is.na(path) || !nzchar(path)) {
    stop("gog: `save_gif()` needs one path \u2014 `save_gif(p, \"wave.gif\")`.",
         call. = FALSE)
  }
  # The name says what the file is, so a path that says otherwise is refused
  # rather than quietly corrected. Writing GIF bytes into `wave.png` is the kind
  # of small lie that is discovered much later, by someone else.
  #
  # The whole path is echoed back, not just the file's name. `basename()` used to
  # be applied here, so a reader who asked for `out/sub/wave.png` was told to
  # write `save_gif(p, "wave.gif")`. That advice puts the file in the working
  # directory while they go looking for it where they meant to put it. The other
  # three bindings always kept the directory; this one was alone in dropping it,
  # and the refusal was in none of the suite's recorded messages, so no
  # comparison across the four could see the difference.
  if (!grepl("\\.gif$", path, ignore.case = TRUE)) {
    stop("gog: `save_gif()` writes a GIF, so the path ends in `.gif` \u2014 ",
         "`save_gif(p, \"", tools::file_path_sans_ext(path), ".gif\")`.",
         call. = FALSE)
  }
  if (!is.numeric(scale) || length(scale) != 1L || is.na(scale) || scale <= 0) {
    stop("gog: `save_gif(scale = )` needs one positive number, e.g. ",
         "`save_gif(p, \"wave.gif\", scale = 2)`.", call. = FALSE)
  }

  json_str <- wire_json(gog)
  cli_path <- find_gog_cli()

  stderr_file <- tempfile()
  on.exit(unlink(stderr_file), add = TRUE)

  # `path.expand` because the engine is another process and will not read `~`
  # the way R does; `shQuote` because `system2` builds a command line, and a
  # plot saved to a folder with a space in its name is not an exotic case.
  result <- suppressWarnings(system2(
    cli_path,
    args   = c("--gif", shQuote(path.expand(path)), "--scale", format(scale)),
    stdout = TRUE, stderr = stderr_file, input = json_str
  ))

  msgs <- if (file.exists(stderr_file)) {
    readLines(stderr_file, warn = FALSE)
  } else {
    character()
  }

  status <- attr(result, "status")
  if (!is.null(status) && status != 0L) {
    stop(paste(msgs, collapse = "\n"), call. = FALSE)
  }
  if (length(msgs) > 0L) message(paste(msgs, collapse = "\n"))

  invisible(path)
}

# ---------------------------------------------------------------------------
# Inline SVG for HTML hosts — knitr, Jupyter
# ---------------------------------------------------------------------------

# Two hosts embed the same HTML, so the wrapper lives here rather than staying
# inside whichever method wrote it first. The engine draws a fixed canvas and
# knows nothing about the column it lands in; the style attribute is what lets
# that canvas shrink into a narrow column instead of overflowing it.
#
# **Whatever size the canvas is.** This matched the literal `width="800"
# height="600"` for as long as 800x600 was the only canvas, so `size()` on a plot
# quietly opted it out of fitting — 10 plots in the manual at 620 or 420 wide,
# each spilling past its column once the window was narrow enough to matter. The
# match is anchored inside the opening `<svg` tag because `[^>]` cannot cross the
# tag's own `>`, which keeps it off the background `<rect>` that carries the same
# two numbers a few characters later.
svg_block <- function(svg_str, gog = NULL) {
  svg_str <- sub(
    '(<svg[^>]*) width="([0-9]+)" height="([0-9]+)"',
    '\\1 width="\\2" height="\\3" style="max-width:100%;height:auto;"',
    svg_str
  )

  # A plot in the cube is turnable wherever the page can run the engine. The
  # static SVG above is still what gets written, and it is what a reader sees in
  # a PDF, in a viewer that strips JavaScript, and in the moment before the
  # engine loads — the script below only upgrades a picture that is already
  # there. When the assets are missing the plot simply stays still, which is the
  # same way `play` degrades in print.
  interactive <- if (!is.null(gog)) interactive_block(gog) else ""

  # **The script goes *inside* the container, and that is a layout rule rather
  # than a style choice.** Quarto's `layout-ncol` divides a chunk's output into
  # cells by counting top-level blocks, so a plot written as a `<div>` followed
  # by a sibling `<script>` is two cells, not one. Two plots in a
  # `layout-ncol: 2` chunk then become four cells and wrap into two rows: each
  # plot alone at full width, beside an empty cell holding only its script. The
  # pair a chapter asked to show side by side ends up stacked, and nothing
  # fails — the render exits 0 and the page looks deliberate.
  #
  # One element is one cell, so nesting the script fixes it everywhere at once.
  # Nothing else cares where it sits: `mountView` resolves its container by id,
  # the SVG is still the container's first element, and the engine path's
  # `innerHTML` replacement can only remove a module script that has already run.
  paste0('\n<div class="gog-plot" style="text-align:center;"',
         if (nzchar(interactive)) paste0(' id="', attr(interactive, "id"), '"') else "",
         '>\n',
         svg_str, '\n',
         interactive,
         '</div>\n')
}

# The browser assets: the WebAssembly engine, and the module that drives it.
#
# Searched the way `find_gog_cli()` searches for the binary and for the same
# reason — a package installed from source, a checkout with a `target/`, and a
# book building in place are three different layouts, and the caller should not
# have to know which one they are in. Returns `NULL` when the engine has not
# been built, which is not an error: it means this plot stays static.
find_wasm_assets <- function() {
  # An installed copy carries its own, beside the binary whose wire format
  # matches it — the same preference `find_gog_cli()` states for the engine.
  installed <- c(system.file("www", "gog.wasm", package = "gog"),
                 system.file("www", "interactive.js", package = "gog"))
  if (all(nzchar(installed)) && all(file.exists(installed))) {
    return(list(wasm = normalizePath(installed[1]), js = normalizePath(installed[2])))
  }

  # A checkout. Walked rather than counted, for the reason `walk_up_for_engine`
  # gives: the distance to the root is not fixed. A book renders from `book/`,
  # a test from the repository root, and `system.file()` answers somewhere else
  # again — so any fixed number of `..`s is right in one case and wrong in the
  # rest. That is the mistake this function made on its first attempt.
  starts <- unique(c(getwd(), system.file(package = "gog")))
  for (start in starts) {
    if (!nzchar(start)) next
    root <- normalizePath(start, winslash = "/", mustWork = FALSE)
    for (i in seq_len(7L)) {
      pair <- c(
        file.path(root, "gog-wasm", "target", "wasm32-unknown-unknown", "release", "gog_wasm.wasm"),
        file.path(root, "js-pkg", "gog", "src", "interactive.js")
      )
      if (all(file.exists(pair))) {
        return(list(wasm = normalizePath(pair[1]), js = normalizePath(pair[2])))
      }
      parent <- dirname(root)
      if (identical(parent, root)) break
      root <- parent
    }
  }
  NULL
}

# An `import` needs a *module specifier*, which is a stricter thing than a URL a
# `fetch` would accept. A bare word like `"gog.js"` is reserved for import maps
# and a browser refuses it outright — the script never runs, no asset is ever
# requested, and the page shows the static plot with nothing in the console to
# say why. That silence is what makes it worth normalizing here rather than
# documenting: `options(gog.js_url = "gog.js")` is the natural thing to write and
# it is the one spelling that fails.
module_specifier <- function(url) {
  if (grepl("^(data:|https?:|file:|/|\\./|\\.\\./)", url)) url else paste0("./", url)
}

# The module's own source, ready to sit inside `<script type="module">`.
#
# **A `data:` URL cannot be imported where a page has a content-security
# policy**, and every host that shows a plot outside a plain browser has one:
# RStudio's Viewer pane, Positron's, and Jupyter. `script-src` there does not
# list `data:`, so `import { mount } from "data:text/javascript;base64,…"` is
# refused — silently, because a blocked module import throws nothing a page can
# catch. The plot still drew, since the SVG is markup, and every control was
# missing: no zoom, no fit, no grab, no camera, no brush. Only the book worked,
# because it sets `gog.js_url` to a real file and never takes this path.
#
# Inlining the source is what survives that policy: an inline module *runs*
# under `script-src 'unsafe-inline'`, which is what those hosts allow, and it
# needs no URL of any scheme. The engine travels the same way, as bytes decoded
# in the page rather than a `data:` URI fetched from it.
inline_modules <- function(paths) {
  src <- paste(vapply(paths, function(p) paste(readLines(p, warn = FALSE),
                                               collapse = "\n"), character(1)),
               collapse = "\n")
  # `interactive.js` takes its view helpers from the sibling file. Inlined, that
  # specifier has nothing to resolve against, and both files are already in this
  # one scope, so the two statements that name it go.
  gsub("(import|export)\\s*\\{[^}]*\\}\\s*from\\s*\"\\./view\\.js\";?", "",
       src, perl = TRUE)
}

# The engine as a JavaScript expression evaluating to its bytes. `loadEngine()`
# takes a URL *or* a BufferSource, so this is the second of the two and needs no
# fetch, no scheme, and nothing from the policy. It is ~1.1 MB of base64 —
# which is why `options(gog.wasm_url =)` exists for a book, where one cached
# file beside the HTML serves every plot in it. Inline is the default because
# it is the only form that survives a notebook being emailed.
wasm_expression <- function(path) {
  url <- getOption("gog.wasm_url", NA_character_)
  if (!is.na(url)) return(paste0('"', url, '"'))
  raw_bytes <- readBin(path, "raw", file.info(path)$size)
  # The `gsub` is not cosmetic. `jsonlite::base64_enc()` wraps its output at 72
  # characters, this string sits inside a JavaScript literal, and a literal
  # newline inside one is a syntax error that kills the whole emitted module.
  # The book never sees it (its URLs are short relative paths); only a
  # `print(p)` in a console inlines the engine, and that is where it broke.
  b64 <- gsub("[\r\n]", "", jsonlite::base64_enc(raw_bytes))
  paste0('Uint8Array.from(atob("', b64, '"), c => c.charCodeAt(0))')
}

interactive_block <- function(gog) {
  spec <- if (inherits(gog, "gog_page")) gog$page else finalize_spec(gog)$spec

  # **Two questions, not one, and they used to be the same question.** Carrying
  # the *engine* has two reasons: a plot in the cube has an angle worth dragging,
  # and a plot naming a brush has a bound worth moving. Both redraw.
  #
  # Carrying the *module* has a third, and it is every plot: looking closer.
  # Zooming scales the SVG's viewBox and recomputes nothing, so it needs this
  # file and not the WebAssembly beside it — 65 KB against 861 KB. Asking one
  # question for both is what left a flat plot with no controls: `− + fit` were
  # behind a gate that exists to avoid the engine, which zoom never loads.
  needs_engine <- spec_needs_engine(spec)

  assets <- find_wasm_assets()
  if (is.null(assets)) return("")

  # **A flat plot names the smaller module and sends no data.** `mountView` takes
  # a container and stops — looking closer needs neither the spec nor the table —
  # so the block is one line beside an 8 KB module, where naming `interactive.js`
  # inlined 88 KB and the whole table again. `view.js` sits beside its sibling, so
  # the path and the URL are both derived rather than searched for a second time.
  view_path <- file.path(dirname(assets$js), "view.js")
  js_option <- getOption("gog.js_url", NA_character_)

  if (!needs_engine) {
    id <- paste0("gog-", paste(sample(c(letters, 0:9), 10, replace = TRUE), collapse = ""))
    head <- if (is.na(js_option)) {
      paste0(inline_modules(view_path), "\n")
    } else {
      paste0('import { mountView } from "',
             module_specifier(sub("interactive\\.js$", "view.js", js_option)), '";\n')
    }
    block <- paste0(
      '<script type="module">\n', head,
      'mountView("', id, '");\n',
      '</script>\n'
    )
    attr(block, "id") <- id
    return(block)
  }

  # The module arrives one of two ways, and the engine likewise. A book names
  # files it serves; everything else carries them, because a temp page in a
  # viewer pane has no directory behind it and a notebook cell has no server.
  head <- if (is.na(js_option)) {
    paste0(inline_modules(c(view_path, assets$js)), "\n")
  } else {
    paste0('import { mount } from "', module_specifier(js_option), '";\n')
  }

  frames <- mapply(resolve_query, gog$data_frames, names(gog$data_frames),
                   SIMPLIFY = FALSE)
  request <- jsonlite::toJSON(
    list(spec = spec, data = lapply(frames, df_to_wire)),
    auto_unbox = TRUE, null = "null", na = "null", force = TRUE, digits = NA
  )

  id <- paste0("gog-", paste(sample(c(letters, 0:9), 10, replace = TRUE), collapse = ""))
  block <- paste0(
    '<script type="module">\n', head,
    'mount("', id, '", ', request,
    ', { wasm: ', wasm_expression(assets$wasm), ' });\n',
    '</script>\n'
  )
  attr(block, "id") <- id
  block
}

# Does this spec draw in the cube? The twin of `isSpatial` in the browser module
# and of `space_of` in the engine: a bound `z` projects a plot even when the
# coordinate still reads "flat", so naming `space()` is sufficient and not
# necessary.
# A brush is plot-scoped, so it sits beside `coord` rather than in a layer.
spec_needs_engine <- function(spec) {
  spec_is_spatial(spec) || length(spec$brush) > 0L
}

spec_is_spatial <- function(spec) {
  # An angle worth dragging lives in two spaces: the cube's view, and the
  # globe's. Missing the second shipped globe pages with zoom buttons and no
  # drag, so half the earth was a picture rather than a place to turn to.
  if (is.list(spec$coord) &&
      (!is.null(spec$coord$space) || !is.null(spec$coord$globe))) return(TRUE)
  if (!is.null(spec$z)) return(TRUE)
  layers <- if (is.null(spec$layers)) list() else spec$layers
  for (layer in layers) {
    if (!is.null(layer$encodings$z)) return(TRUE)
  }
  # A page: any cell in the cube, or any cell naming a brush, makes the page
  # carry the engine. R spells the list `cells`; the other bindings spell it
  # `plots`, so both are read rather than one being assumed.
  cells <- if (is.null(spec$cells)) spec$plots else spec$cells
  if (is.null(cells)) cells <- list()
  for (cell in cells) {
    if (spec_needs_engine(cell)) return(TRUE)
  }
  FALSE
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

  knitr::asis_output(svg_block(svg_str, x))
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

# A refusal, shown in a cell as the sentence the engine wrote.
#
# The engine takes trouble over these: every one names what it would not do and
# what to write instead. A display hook that lets the condition through buries
# that sentence under thirty lines of `repr::mime2repr` internals, and not one of
# those lines is anywhere the author can act.
#
# Only the *display* path does this. `render_svg()` and `save_gif()` still stop,
# and so does `knit_print()` — the book documents its refusals with `error: true`
# chunks, and a chunk that stopped erroring would be a refusal the manual claims
# and the engine no longer makes.
refusal_block <- function(message) {
  escaped <- gsub("&", "&amp;", message, fixed = TRUE)
  escaped <- gsub("<", "&lt;", escaped, fixed = TRUE)
  escaped <- gsub(">", "&gt;", escaped, fixed = TRUE)
  paste0('<pre style="white-space:pre-wrap;word-break:break-word;',
         "border-left:3px solid #c2410c;padding:0.6em 0.9em;margin:0;",
         'font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:0.9em">',
         escaped, "</pre>")
}

# Draw for a display host, or show why not. A refusal carries the `gog: ` prefix
# the whole package writes its diagnostics with; anything else is a fault in this
# code or in R, and those must keep raising or a real bug becomes a grey box.
display_or_refusal <- function(obj) {
  tryCatch(
    svg_block(render_svg(obj), obj),
    error = function(e) {
      message_text <- conditionMessage(e)
      if (!startsWith(message_text, "gog: ")) stop(e)
      refusal_block(message_text)
    }
  )
}

#' repr method: the plot as inline SVG in a Jupyter cell.
#'
#' @param obj The plot or page being displayed.
#' @param ... Passed on by the display host; unused.
repr_html.gog_spec <- function(obj, ...) display_or_refusal(obj)

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
    IRdisplay::display_html(svg_block(svg_str, gog))
    return(invisible(svg_str))
  }

  # `svg_block()` rather than the bare SVG, so a plot in the cube can be turned
  # here too. This page is what `print(p)` shows in the RStudio and Positron
  # viewer panes and in a browser tab, and it used to embed the drawing on its
  # own — which meant the one place an R user *looks at a plot interactively* was
  # the one place that never offered it. The notebook branch above had been
  # wired and this had not.
  #
  # It carries the engine inline, and here that is the only thing that can work:
  # the page is a `file://` temp file with no directory of its own and no server
  # behind it, so a relative URL has nothing to resolve against. That is exactly
  # the case the `data:` default was written for, and it is why the option the
  # book sets is an option rather than the default.
  html <- paste0(
    "<!DOCTYPE html>\n<html>\n",
    "<head><meta charset='utf-8'>",
    "<style>body{margin:0;background:#fff;display:flex;",
    "justify-content:center;padding:16px;}</style></head>\n",
    "<body>\n", svg_block(svg_str, gog), "\n</body>\n</html>"
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
