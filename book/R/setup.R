# book/R/setup.R
# Sourced at the top of every chapter (include: false).
# - Loads the gog R package from source
# - Discovers the gog-cli binary
# - Sources the shared example data frames from data.R

# ---------------------------------------------------------------------------
# Locate project root and load the gog package
# ---------------------------------------------------------------------------

# Quarto sets getwd() to the directory of the .qmd file being rendered, so the
# project root is however many levels up that page happens to sit. Five rather
# than the two a chapter needs, because a caller sourcing this from anywhere in
# the tree is the case it was written for, and `R/data.R` beside this one
# already walks four for the same reason.
find_proj_root <- function() {
  # A translated edition renders from a copy of `book/` somewhere else, which
  # has no repository above it, so the render names the root instead. Unset, the
  # root is found by walking up, as it always was.
  given <- Sys.getenv("GOG_PROJECT_ROOT")
  if (nzchar(given)) {
    if (!dir.exists(file.path(given, "gog-cli")))
      stop("GOG_PROJECT_ROOT is ", given, ", which holds no gog-cli/: name the repository's root")
    return(normalizePath(given))
  }
  for (up in c(".", "..", "../..", "../../..", "../../../..")) {
    p <- normalizePath(file.path(up, "gog-cli"), mustWork = FALSE)
    if (dir.exists(p)) return(normalizePath(up))
  }
  stop("Cannot locate gog project root from working directory: ", getwd())
}

proj_root <- find_proj_root()

# Point to the compiled binary (release first, then debug fallback)
if (!nzchar(Sys.getenv("GOG_CLI_PATH"))) {
  exe <- if (.Platform$OS.type == "windows") "gog-cli.exe" else "gog-cli"
  for (build in c("release", "debug")) {
    cli_path <- file.path(proj_root, "target", build, exe)
    if (file.exists(cli_path)) {
      Sys.setenv(GOG_CLI_PATH = normalizePath(cli_path))
      break
    }
  }
}

# Source the R package files
#
# `local = TRUE` here and below, on every nested source: it means *the
# environment this file is being evaluated in*, which is what the file has
# always meant. Bare `source()` evaluates in the **global** environment
# regardless, and that is only the same thing when setup.R itself was sourced
# globally — true under knitr (a chapter's `source("R/setup.R")` runs in
# `knit_global()`), false for anything that loads this file into an environment
# of its own. `book_parity/extract.R` does exactly that, to keep the chapters'
# frames out of its own workspace, so `proj_root` landed in its environment
# while the sources below looked for it in the global one and the corpus could
# not be re-recorded at all. Behavior under knitr is unchanged.
pkg_r <- file.path(proj_root, "r-pkg", "gog", "R")
for (f in list.files(pkg_r, pattern = "\\.R$", full.names = TRUE)) source(f, local = TRUE)

# ---------------------------------------------------------------------------
# Example data frames used across all chapters
# ---------------------------------------------------------------------------

# Defined in data.R rather than here so that reading the frames stays separable
# from loading the package. The dev notebook that motivated the split was removed
# 2026-07-29; the split is kept because `make-data.R` builds what `data.R` reads,
# and a caller wanting the tables should not have to take setup.R's package load
# with them.
source(file.path(proj_root, "book", "R", "data.R"), local = TRUE)

# The Python side of the book. `python.R` runs a Python sentence and embeds the
# plot it drew (the Python chapter); `tabs.R` installs the source hook that puts
# every sentence in the book into an R/Python tabset: the sentence is rendered
# once, and every language's spelling of it is shown. Both need only an
# interpreter here; pandas
# is checked when the Python chapter first runs a snippet, so a checkout without
# it loses one page rather than the book.
source(file.path(proj_root, "book", "R", "python.R"), local = TRUE)
source(file.path(proj_root, "book", "R", "tabs.R"), local = TRUE)

# ---------------------------------------------------------------------------
# The turnable cube
# ---------------------------------------------------------------------------

# A plot in the cube carries a script that lets a reader drag it to a different
# viewing angle. Nothing in any chapter asks for this — it is a property of the
# medium rather than of the sentence, so every 3-D plot already written becomes
# turnable and not one `.qmd` changes. The still SVG is what a reader sees in the
# PDF, in a viewer that strips JavaScript, and before the engine loads; the
# script only upgrades a picture that is already there.
#
# These two options are what stop it being expensive. Left unset, each binding
# carries the engine *inside* every plot as a base64 `data:` URI — right for a
# notebook, which has to survive being emailed, and wrong for a book, where it
# would add about 1.1 MB to every page with a cube on it. Pointed at a file
# instead, one 823 KB copy is fetched once and cached for the whole book, and a
# plot's own block drops to about 4 KB.
#
# Copied here rather than committed: they are build output, and `book/` is a
# public working tree. `_quarto.yml` lists both under `resources:` so Quarto
# puts them in `_book/` beside the pages that fetch them.
local({
  wasm <- file.path(proj_root, "gog-wasm", "target", "wasm32-unknown-unknown",
                    "release", "gog_wasm.wasm")
  js   <- file.path(proj_root, "js-pkg", "gog", "src", "interactive.js")
  if (!file.exists(wasm) || !file.exists(js)) {
    # Not an error. The engine is built by a separate cargo invocation from the
    # one that builds `gog-cli`, so a checkout that has not run it renders the
    # whole book correctly with still cubes. Said out loud because a silently
    # static book is exactly the thing that would go unnoticed.
    message("gog: no WebAssembly engine built — 3-D plots will render static.\n",
            "  cargo build --release --target wasm32-unknown-unknown ",
            "--manifest-path gog-wasm/Cargo.toml")
  } else {
    # The files themselves are staged by `R/copy-assets.R`, which runs as the
    # project's `pre-render` script. It has to happen before the render rather
    # than here, because Quarto decides what `resources:` covers at the start and
    # this runs once per chapter, long after. What is left for this file is the
    # URL, which is per-chapter work and could not move earlier anyway.
    #
    # Both URLs are relative to the *page*, and neither project is flat: a
    # chapter under `marks/` renders to `_book/marks/`, where a bare `gog.wasm`
    # would point at a file one directory up. Quarto runs each chunk in its own
    # page's directory — which is why a chapter under `marks/` sources
    # `../R/setup.R` — so the depth is readable from the working directory, and
    # the output tree mirrors the source tree exactly.
    #
    # Measured against the *nearest `_quarto.yml`*, which is the page's own
    # project. It named this directory outright once, and that was right only
    # while the book was the sole Quarto project in the tree: from a page in any
    # other one the pattern matched nothing, the subject stayed an absolute
    # path, and the depth came out as its directory count — eight on a typical
    # checkout, for a page two levels down. Two smaller faults went with it. A
    # filesystem path was being spliced in as a *regex*, so any character
    # special to one would have misbehaved; and `setdiff` drops duplicates as
    # well as empties, so a page at `marks/marks/` would have counted as one.
    here  <- normalizePath(getwd(), winslash = "/")
    qroot <- here
    while (!file.exists(file.path(qroot, "_quarto.yml")) &&
           !identical(dirname(qroot), qroot)) qroot <- dirname(qroot)
    depth <- sum(nzchar(strsplit(substring(here, nchar(qroot) + 1L), "/")[[1]]))
    up <- paste(rep("../", depth), collapse = "")
    options(gog.wasm_url = paste0(up, "gog.wasm"),
            gog.js_url   = paste0(up, "interactive.js"))
  }
})

# ---------------------------------------------------------------------------
# What a table looks like, before the first plot that uses it
#
# A sentence names its columns (`x(gdp)`, `y(life)`), which is more than most
# plotting code tells you — but it never shows what is *in* them. A reader who
# has not seen the rows cannot tell whether `life` is a count or a proportion,
# whether `year` repeats, or how many rows the picture is drawing.
#
# Three decisions are made here rather than at each of the sites that call it:
#
#   1. **The table is shown, not the call that prints it.** Every site is an
#      `echo: false` chunk, so the page carries the rows and not R's `head()`.
#      This book has four bindings and shows every sentence in all of them, so
#      an R-only inspection call beside a four-language sentence would be the
#      one place a Python reader is handed R. Showing a table how each language
#      does it is a job that already has a page, in `book-data.qmd`.
#   2. **The row count is stated.** Five rows of a 142-row table read as the
#      whole table otherwise, and that misreads every plot drawn from it.
#   3. **Short tables say so.** `medals` is five rows, so "first 5 of 5" would
#      invite a reader to look for the rest. A one-row table reads "its one
#      row": "all 1 rows" is not English, and a threshold table is often one row.
#
# The view is shown once per chapter, at the first sentence that reads the
# table, and it repeats in every chapter that draws from it.
# ---------------------------------------------------------------------------

peek <- function(x, n = 5) {
  name  <- deparse(substitute(x))
  total <- nrow(x)
  shown <- min(n, total)
  caption <- if (total == 1) sprintf(book_text("peek_one", "`%s`: its one row"), name)
             else if (shown >= total) sprintf(book_text("peek_all", "`%s`: all %d rows"), name, total)
             else sprintf(book_text("peek_first", "`%s`: first %d of %d rows"), name, shown, total)
  knitr::kable(utils::head(x, n), caption = caption)
}

# ---------------------------------------------------------------------------
# Words the helpers write, in the chapter's language
#
# A translated chapter (`first-plot.ko.qmd`) says its language in its front
# matter, `gog-translation: {lang: ko, …}`. Its code is the English code byte for
# byte, so the words a helper prints around a table (a caption, a header, a
# setting's description) are chosen here rather than written in the chunk. Each
# call carries its English, so an English chapter prints exactly what it always
# did, and a word with no translation yet stays English. The translations are in
# `book/i18n/strings.yml`, one block per language. Table and column names inside
# them are data, and stay as they are.
# ---------------------------------------------------------------------------

book_lang <- local({
  seen <- list()
  function() {
    input <- tryCatch(knitr::current_input(dir = TRUE), error = function(e) NULL)
    if (is.null(input) || !length(input) || !nzchar(input) || !file.exists(input)) return("en")
    if (!is.null(seen[[input]])) return(seen[[input]])
    lines <- readLines(input, warn = FALSE, n = 200)
    ends <- which(grepl("^---\\s*$", lines))
    lang <- "en"
    if (length(ends) >= 2 && ends[1] == 1) {
      fm <- tryCatch(yaml::yaml.load(paste(lines[(ends[1] + 1):(ends[2] - 1)], collapse = "\n")),
                     error = function(e) list())
      given <- fm[["gog-translation"]][["lang"]]
      if (is.character(given) && length(given) == 1 && nzchar(given)) lang <- given
    }
    seen[[input]] <<- lang
    lang
  }
})

.book_strings <- local({
  cached <- NULL
  function() {
    if (is.null(cached)) {
      f <- file.path(proj_root, "book", "i18n", "strings.yml")
      cached <<- if (file.exists(f)) yaml::read_yaml(f) else list()
    }
    cached
  }
})

book_text <- function(key, english) {
  lang <- book_lang()
  if (identical(lang, "en")) return(english)
  said <- .book_strings()[[lang]][[key]]
  if (is.character(said) && length(said) == 1 && nzchar(said)) said else english
}

# ---------------------------------------------------------------------------
# The glyph grids
#
# Five tables in the book are a grid of one-glyph cells under a row of atom
# names: four in `combinations.qmd` and the settings grid in `style.qmd`. The
# web edition fits them as they are, because a browser sizes each column to its
# content. The PDF does not. Pandoc turns any pipe table with a line longer than
# 72 characters into fixed columns of equal width, and a code-font name such as
# `opacity` is wider than its share of the page. On the 24-column chain grid
# every name ran into its neighbors and nothing could be read.
#
# So the PDF gets the layout a printed matrix uses: the column names turned on
# their side, the padding narrowed, and the first column given the width of its
# longest label, which is the only text in a row. The HTML table is the same
# Markdown as before, byte for byte.
#
# `glyph(row, col)` returns the glyph for one cell.
# ---------------------------------------------------------------------------

glyph_grid <- function(corner, rows, cols, glyph) {
  # The corner names what the rows are ("Mark", "Setting"), a word of the page
  # rather than of the grammar, so a translated chapter prints its own.
  if (nzchar(corner)) corner <- book_text(paste0("corner_", tolower(corner)), corner)
  pdf  <- knitr::is_latex_output()
  head <- if (pdf) {
    sprintf("\\rotatebox{90}{\\texttt{%s}}", gsub("_", "\\\\_", cols))
  } else {
    paste0("`", cols, "`")
  }
  # A column's share of the PDF page follows the length of its divider. A glyph
  # column gets the shortest one pandoc allows, and the label column two dashes
  # per character, which is what a code-font letter costs against a glyph.
  first <- if (pdf) strrep("-", 2 * max(nchar(c(corner, rows)))) else "--"
  cell  <- if (pdf) ":-:" else ":--:"
  lines <- c(
    paste0("| ", if (nzchar(corner)) paste0(corner, " ") else "",
           "| ", paste(head, collapse = " | "), " |"),
    paste0("|:", first, "|", paste(rep(cell, length(cols)), collapse = "|"), "|"),
    vapply(rows, function(r) {
      gs <- vapply(cols, function(c) glyph(r, c), character(1))
      paste0("| `", r, "` | ", paste(gs, collapse = " | "), " |")
    }, character(1))
  )
  if (pdf) {
    lines <- c("```{=latex}", "\\begingroup\\setlength{\\tabcolsep}{2pt}", "```", "",
               lines, "",
               "```{=latex}", "\\endgroup", "```")
  }
  cat(lines, sep = "\n")
}

# ---------------------------------------------------------------------------
# The per-mark options table
#
# Every mark chapter ends with "what you can set", and hand-typing that in a
# dozen chapters is the drift the three master grids exist to prevent: a new
# setting, or a mark joining a geometry class, would silently stale eleven pages.
# So the table is *generated* from `gog-cli --rules`, exactly as the grids in
# `combinations.qmd` and `style.qmd` are, and from the same dump.
#
# The vocabularies come from the engine too (`legality::setting_values`), which
# matters because one setting is realized differently per geometry: `pattern` is
# a stroke's dash on `line`/`rule` and a fill's hatch on `bar`/`zone`. A chapter
# that listed one set for both would be wrong for half the marks.
# ---------------------------------------------------------------------------

.gog_rules <- local({
  cached <- NULL
  function() {
    if (is.null(cached)) {
      cli <- find_gog_cli()
      cached <<- jsonlite::fromJSON(
        paste(system2(cli, "--rules", stdout = TRUE), collapse = "\n")
      )
    }
    cached
  }
})

# The settings described in words rather than listed are the same on every mark,
# so describing them once here cannot drift the way a per-chapter sentence could.
# A yes-or-no setting is among them: each language spells true its own way, so the
# engine lists no values for it and the table says "true or false".
.gog_open_values <- c(
  caps         = "true or false",
  center       = "true or false",
  color        = "any CSS color name or hex",
  border_color = "any CSS color name or hex",
  opacity      = "0 to 1",
  size         = "pixels",
  border_size  = "pixels",
  angle        = "degrees, counterclockwise from horizontal",
  gap          = "a share of the mark's place, from 0 to just under 1"
)

mark_options <- function(mark) {
  r <- .gog_rules()
  sc <- r$setting_cells
  sc <- sc[sc$mark == mark & sc$settable, ]
  vals <- vapply(seq_len(nrow(sc)), function(i) {
    v <- sc$values[[i]]
    if (length(v)) paste0("`", paste(v, collapse = "`, `"), "`")
    else if (!is.na(.gog_open_values[sc$setting[i]]))
      book_text(paste0("value_", sc$setting[i]), unname(.gog_open_values[sc$setting[i]]))
    # An empty cell is a row that promises "the values each accepts" and shows
    # none, which is what `angle` printed in the Text chapter until it had a
    # description above. `book/check_settings.R` finds it before a render does.
    else stop("mark_options(): the engine lists no values for `style(", sc$setting[i],
              " = )` and .gog_open_values describes none, so its row would print ",
              "an empty Value cell. Describe them in .gog_open_values.", call. = FALSE)
  }, character(1))

  # Rows only, and deliberately no trailing prose. Generated text is invisible
  # in the `.qmd`, so an author editing the chapter cannot see it, review it, or
  # fix it in place. A sentence listing *channels* used to be printed here and
  # read as part of the settings table: four names (`color`, `opacity`, `size`,
  # `pattern`) are both, so it invited mapping `size` on a `line`, which the
  # engine refuses. What a mark maps is a question the chapter's own sections
  # answer, and the mapping grid answers for every mark at once.
  # The header is printed here rather than in each chapter's chunk, so a
  # translated chapter, whose chunks are the English ones, gets its own words.
  cat(paste0("| ", book_text("settings_setting", "Setting"), " | ",
             book_text("settings_value", "Value"), " |\n|---|---|\n"))
  cat(paste0("| `style(", sc$setting, " = )` | ", vals, " |"),
      sep = "\n")
}

# ---------------------------------------------------------------------------
# The per-mark transforms table
#
# Every mark chapter has a "Transforms it takes" section (the author, 2026-10-22:
# mark × transform is "the most valuable at the same time most tricky part that
# readers should master"). Its table is generated from the same `gog-cli --rules`
# dump as the settings table, so a mark that gains or loses a transform changes
# its chapter on the next build. The chapter's galleries draw each one; this
# table is the complete list, refusals included.
#
# The kinds are the book's own (`grammar.qmd`, `transforms.qmd`), finer than the
# dump's three classes: the dump files `cumulative` and the four layouts under
# "statistic". A transform the engine adds without a kind here stops the build,
# so the table can never silently drop one.
# ---------------------------------------------------------------------------

.transform_kinds <- list(
  counting   = c("bin", "count", "density", "proportion"),
  summary    = c("sum", "mean", "median", "max", "min", "quantile"),
  fitted     = c("smooth", "straight"),
  running    = "cumulative",
  pair       = c("range", "confidence", "deviation", "smooth_band",
                 "straight_band", "bounds"),
  collision  = c("dodge", "stack", "jitter", "repel"),
  layout     = c("partition", "flow", "layout", "cluster")
)
.transform_kind_names <- c(
  counting  = "Counting",
  summary   = "Summaries",
  fitted    = "Fitted lines",
  running   = "Running total",
  pair      = "Pairs: a low and a high",
  collision = "Collision modifiers",
  layout    = "Layouts of a whole picture"
)
# The section of `transforms.qmd` that explains each one. Checked against that
# file's headings on every build, so a renamed section fails here, not as a
# dead link in fourteen chapters.
.transform_anchor <- c(
  bin = "bin-histogram", count = "count-frequency-aggregation",
  density = "density-kernel-density-estimate",
  proportion = "proportion-relative-frequency",
  sum = "aggregation-family-sum-mean-median-max-min-quantile",
  mean = "aggregation-family-sum-mean-median-max-min-quantile",
  median = "aggregation-family-sum-mean-median-max-min-quantile",
  max = "aggregation-family-sum-mean-median-max-min-quantile",
  min = "aggregation-family-sum-mean-median-max-min-quantile",
  quantile = "quantile-one-point-in-the-distribution",
  smooth = "smooth-loess-trend-line", straight = "straight-the-least-squares-line",
  cumulative = "cumulative-the-running-total",
  range = "range-a-lowhigh-pair", confidence = "confidence-the-interval-of-the-mean",
  deviation = "deviation-the-spread-of-the-data",
  smooth_band = "smooth_band-the-band-around-the-curve",
  straight_band = "straight_band-the-band-around-the-line",
  bounds = "bounds-a-pre-computed-lowhigh-pair",
  dodge = "dodge-side-by-side-instead-of-on-top",
  stack = "stack-piled-up-not-side-by-side",
  jitter = "jitter-spread-sideways-along-the-category",
  repel = "repel-move-the-labels-off-each-other",
  partition = "partition-a-whole-among-nested-parts",
  flow = "flow-a-magnitude-through-its-stages",
  layout = "layout-a-graph-placed-from-an-edge-table",
  cluster = "cluster-the-closest-leaves-joined-first"
)

mark_transforms <- function(mark) {
  r <- .gog_rules()
  known <- unlist(.transform_kinds, use.names = FALSE)
  missing_kind <- setdiff(r$transforms$name, known)
  if (length(missing_kind)) {
    stop("mark_transforms(): the engine has transform(s) with no kind in ",
         ".transform_kinds: ", paste(missing_kind, collapse = ", "),
         ". Give each a kind, and an anchor in .transform_anchor.", call. = FALSE)
  }
  ids <- regmatches(readLines(file.path(proj_root, "book", "transforms.qmd"),
                              warn = FALSE),
                    regexpr("\\{#[^}]+\\}", readLines(file.path(proj_root, "book",
                                                              "transforms.qmd"),
                                                    warn = FALSE)))
  ids <- sub("^\\{#(.*)\\}$", "\\1", ids)
  dead <- setdiff(unname(.transform_anchor[known]), ids)
  if (length(dead) || anyNA(.transform_anchor[known])) {
    stop("mark_transforms(): no section in transforms.qmd for ",
         paste(c(dead, known[is.na(.transform_anchor[known])]), collapse = ", "),
         call. = FALSE)
  }
  tc <- r$transform_cells
  tc <- tc[tc$mark == mark, ]
  if (!nrow(tc)) stop("mark_transforms(): the engine knows no mark `", mark, "`",
                      call. = FALSE)
  state <- setNames(tc$state, tc$transform)
  link <- function(t) sprintf("[`%s`](../transforms.qmd#%s)", t, .transform_anchor[[t]])
  cell <- function(ts) if (length(ts)) paste(vapply(ts, link, ""), collapse = ", ") else "—"
  cat(paste0("| ", book_text("transforms_kind", "Kind"), " | ",
             book_text("transforms_takes", "Takes"), " | ",
             book_text("transforms_refuses", "Refuses"), " |\n|---|---|---|\n"))
  for (k in names(.transform_kinds)) {
    ts <- .transform_kinds[[k]]
    takes <- ts[state[ts] %in% c("combines", "required")]
    # A required transform is marked: the mark draws only with one of them.
    takes_txt <- if (length(takes)) paste(vapply(takes, function(t)
      paste0(link(t), if (state[[t]] == "required") " ●" else ""), ""),
      collapse = ", ") else "—"
    cat(sprintf("| %s | %s | %s |\n",
                book_text(paste0("kind_", k), .transform_kind_names[[k]]),
                takes_txt, cell(ts[state[ts] == "none"])))
  }
}
