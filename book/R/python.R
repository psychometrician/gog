# book/R/python.R — how a Python sentence gets into an R-engine book
#
# The book is a knitr project: every chapter runs R. The Python chapter still
# has to obey the same rule as every other page, that a code block the reader
# sees is a block the engine actually ran, because a block that does not run is
# how a manual ends up documenting things that do not work.
#
# So the chapter does not *show* Python, it *runs* it. `py_plot()` hands the
# sentence to `python3`, which builds the specification with the Python binding
# and shells out to the same `gog-cli` every R chunk uses; what comes back is
# the SVG, embedded beside the sentence that drew it. A sentence the binding
# refuses stops the render, exactly as a broken R chunk does.
#
# This needs no Jupyter and no reticulate: it is one subprocess, the same shape
# as `setup.R`'s `gog-cli --rules` call. The cost is that the Python chapter's
# blocks are fenced by these helpers rather than by Quarto, which is why they
# are written here where the mechanism is visible.

py_root <- normalizePath(file.path(proj_root, "py-pkg", "gog"), mustWork = TRUE)
py_cli <- Sys.getenv("GOG_CLI_PATH")

# One interpreter for the chapter. `book/.venv` first, because the chapter shows
# pandas doing the data work — that is what a Python reader will actually have
# in front of them, and it is where the grammar sends computation it refuses to
# do itself (a channel takes a column, so a derived column is the host
# language's job). Create it once:
#
#   uv venv book/.venv && uv pip install --python book/.venv/bin/python pandas
#
# `GOG_BOOK_PYTHON` overrides, and a bare `python3` is the fallback for a
# checkout that has not made the venv yet.
py_exe <- Sys.getenv("GOG_BOOK_PYTHON", unset = "")
if (!nzchar(py_exe)) {
  # The directory is normalized and the file name appended, never the other way
  # round: `.venv/bin/python` is a symlink to the base interpreter, and
  # resolving it steps outside the environment — the venv is found by where the
  # interpreter is *invoked* from, beside its `pyvenv.cfg`.
  venv_bin <- file.path(proj_root, "book", ".venv", "bin")
  if (dir.exists(venv_bin)) py_exe <- file.path(normalizePath(venv_bin), "python")
}
if (!nzchar(py_exe)) py_exe <- Sys.which("python3")
if (!nzchar(py_exe)) py_exe <- Sys.which("python")
if (!nzchar(py_exe)) stop("book: no python3 on PATH, and bindings/python.qmd needs one.")

# Checked when a snippet first runs, not when this file loads. Every chapter
# sources it now (the language tabs in `tabs.R` need the interpreter), and only
# the Python chapter runs pandas examples, so a checkout without pandas should
# lose that one page rather than the whole book.
py_pandas_checked <- FALSE

py_require_pandas <- function() {
  if (py_pandas_checked) return(invisible(TRUE))
  if (system2(py_exe, c("-c", shQuote("import pandas")),
              stdout = FALSE, stderr = FALSE) != 0L) {
    stop("book: bindings/python.qmd renders pandas examples, and this interpreter ",
         "(", py_exe, ") has no pandas.\n",
         "  uv venv book/.venv && uv pip install --python book/.venv/bin/python pandas\n",
         "Or point GOG_BOOK_PYTHON at an interpreter that has it.", call. = FALSE)
  }
  py_pandas_checked <<- TRUE
  invisible(TRUE)
}

# The chapter's table, written where Python can read it. A CSV rather than the
# JSON wire format, because reading a CSV is what a Python user would actually
# do, and the chapter shows that reader the same 12 lines it runs on.
py_data <- file.path(tempdir(), "gapminder_2007.csv")
write.csv(gapminder_2007, py_data, row.names = FALSE)

# The preamble every snippet runs under. The reader is shown the sentence, not
# this: `sys.path` because the package is not installed anywhere yet (see the
# chapter's last section), and `GOG_CLI_PATH` because the engine is a local
# build. Both are the Python spelling of what `setup.R` does for R.
py_preamble <- function() {
  paste0(
    "import os, sys\n",
    "sys.path.insert(0, ", shQuote(py_root, type = "sh"), ")\n",
    "os.environ['GOG_CLI_PATH'] = ", shQuote(py_cli, type = "sh"), "\n",
    "from gog import *\n",
    "from gog import GogError\n",
    "import pandas as pd\n",
    "GAPMINDER_CSV = ", shQuote(py_data, type = "sh"), "\n",
    "gapminder_2007 = pd.read_csv(GAPMINDER_CSV)\n"
  )
}

# Run a snippet, returning its stdout and stderr and whether it failed.
py_run <- function(code) {
  py_require_pandas()
  script <- tempfile(fileext = ".py")
  on.exit(unlink(script), add = TRUE)
  writeLines(paste0(py_preamble(), code), script)

  errors <- tempfile()
  on.exit(unlink(errors), add = TRUE)
  out <- suppressWarnings(
    system2(py_exe, shQuote(script), stdout = TRUE, stderr = errors)
  )
  status <- attr(out, "status")
  list(
    stdout = paste(out, collapse = "\n"),
    stderr = paste(readLines(errors, warn = FALSE), collapse = "\n"),
    failed = !is.null(status) && status != 0L
  )
}

# The code block the reader sees. Fenced as Python for the highlighter; the
# content is the same string that was just executed, so the two cannot drift.
py_block <- function(code) paste0("\n```python\n", trimws(code), "\n```\n")

#' Show a Python sentence and the plot it drew.
#'
#' @param code One gog sentence, written in Python.
#' @param setup Code the sentence needs but the reader has already seen, run
#'   before it and not shown again. Each snippet on the page is its own process,
#'   so a frame built in an earlier block has to be rebuilt here; re-running the
#'   block the reader was shown is the honest way to do that, since the two
#'   cannot then drift.
#' @param echo Show the sentence as well as the plot.
py_plot <- function(code, setup = "", echo = TRUE) {
  result <- py_run(paste0(setup, "\nprint(render_svg(", code, "))"))
  if (result$failed) {
    stop("book: the Python sentence did not draw.\n", code, "\n", result$stderr,
         call. = FALSE)
  }
  knitr::asis_output(paste0(
    if (echo) py_block(code) else "",
    svg_block(result$stdout)
  ))
}

#' Show a Python sentence that the grammar refuses, and the message it gives.
#'
#' The counterpart to a `#| error: true` chunk, and it asserts what that option
#' only tolerates: a sentence here that *renders* stops the render. The book
#' once carried a documented refusal that had quietly stopped refusing, which is
#' what `check_refusals.R` exists to catch; a helper that can make the same
#' mistake would be a step backwards.
py_error <- function(code, setup = "") {
  result <- py_run(paste0(
    setup, "\n",
    "try:\n",
    "    render_svg(", code, ")\n",
    "    print('DREW')\n",
    "except GogError as error:\n",
    "    print(error)\n"
  ))
  if (result$failed || identical(trimws(result$stdout), "DREW")) {
    stop("book: this sentence is documented as a refusal and did not refuse.\n",
         code, "\n", result$stderr, call. = FALSE)
  }
  knitr::asis_output(paste0(
    py_block(code),
    "\n```\n", trimws(result$stdout), "\n```\n"
  ))
}

#' Show a Python snippet and whatever it printed.
py_show <- function(code, setup = "") {
  result <- py_run(paste0(setup, "\n", code))
  if (result$failed) {
    stop("book: the Python snippet failed.\n", code, "\n", result$stderr, call. = FALSE)
  }
  knitr::asis_output(paste0(
    py_block(code),
    if (nzchar(trimws(result$stdout))) paste0("\n```\n", trimws(result$stdout), "\n```\n") else ""
  ))
}
