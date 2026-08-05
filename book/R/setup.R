# book/R/setup.R
# Sourced at the top of every chapter (include: false).
# - Loads the gog R package from source
# - Discovers the gog-cli binary
# - Sources the shared example data frames from data.R

# ---------------------------------------------------------------------------
# Locate project root and load the gog package
# ---------------------------------------------------------------------------

# Quarto sets getwd() to the directory of the .qmd file being rendered, so the
# project root is however many levels up that page happens to sit. Five, not the
# book's own two, because this file is sourced from more than the book now: a
# blog post at `blog/posts/<slug>/` is four levels down, and `R/data.R` beside
# this one already walks four for the same reason.
find_proj_root <- function() {
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
    # project. It named the book's directory outright until the blog arrived,
    # and that was right only while the book was the sole project here: from a
    # blog page the pattern matched nothing, the subject stayed an absolute
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
#      invite a reader to look for the rest.
# ---------------------------------------------------------------------------

peek <- function(x, n = 5) {
  name  <- deparse(substitute(x))
  total <- nrow(x)
  shown <- min(n, total)
  knitr::kable(utils::head(x, n), caption = sprintf(
    if (shown >= total) "`%s`: all %d rows" else "`%s`: first %d of %d rows",
    name, shown, total
  ))
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

# The four open-ended settings are the same on every mark, so describing them
# once here cannot drift the way a per-chapter sentence could.
.gog_open_values <- c(
  color        = "any CSS color name or hex",
  border_color = "any CSS color name or hex",
  opacity      = "0 to 1",
  size         = "pixels",
  border_size  = "pixels"
)

mark_options <- function(mark) {
  r <- .gog_rules()
  sc <- r$setting_cells
  sc <- sc[sc$mark == mark & sc$settable, ]
  vals <- vapply(seq_len(nrow(sc)), function(i) {
    v <- sc$values[[i]]
    if (length(v)) paste0("`", paste(v, collapse = "`, `"), "`")
    else if (!is.na(.gog_open_values[sc$setting[i]])) unname(.gog_open_values[sc$setting[i]])
    else ""
  }, character(1))

  cells <- r$cells
  mapped <- cells[cells$mark == mark & cells$obligation != "cannot" &
                    !is.na(cells$renders) & !cells$channel %in% c("x", "y", "z"), ]

  cat(paste0("| `style(", sc$setting, " = )` | ", vals, " |"),
      sep = "\n")
  cat("\n\nAnd these vary per row if you map them to a column instead: ",
      paste0("`", mapped$channel, "()`", " (",
             ifelse(mapped$accepts == "discrete", "categories",
                    ifelse(mapped$accepts == "continuous", "numbers", "either")),
             ")", collapse = ", "),
      ".\n", sep = "")
}
