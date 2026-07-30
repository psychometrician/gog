# book/R/setup.R
# Sourced at the top of every chapter (include: false).
# - Loads the GOG R package from source
# - Discovers the gog-cli binary
# - Sources the shared example data frames from data.R

# ---------------------------------------------------------------------------
# Locate project root and load the GOG package
# ---------------------------------------------------------------------------

# Quarto sets getwd() to the directory of the .qmd file being rendered.
# The project root is one or two levels up depending on chapter depth.
find_proj_root <- function() {
  for (up in c(".", "..", "../..")) {
    p <- normalizePath(file.path(up, "gog-cli"), mustWork = FALSE)
    if (dir.exists(p)) return(normalizePath(up))
  }
  stop("Cannot locate GOG project root from working directory: ", getwd())
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
