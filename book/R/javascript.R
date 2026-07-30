# book/R/javascript.R — how a JavaScript sentence gets into an R-engine book
#
# The counterpart of `python.R`, and the same argument: the book is a knitr
# project, every chapter runs R, and the JavaScript chapter still has to obey the
# rule every other page does — a code block the reader sees is a block the engine
# actually ran, because a block that does not run is how a manual ends up
# documenting things that do not work.
#
# So the chapter does not *show* JavaScript, it *runs* it. `js_plot()` hands the
# sentence to `node`, which builds the specification with the JavaScript binding
# and shells out to the same `gog-cli` every R chunk uses; what comes back is the
# SVG, embedded beside the sentence that drew it. A sentence the binding refuses
# stops the render, exactly as a broken R chunk does.
#
# One difference from `python.R`, and it is the binding's rather than the book's:
# the Python chapter needs pandas, because a Python reader does their data work
# there. JavaScript has no such dependency to check — a table is an object of
# arrays, which is the language with nothing installed.

js_root <- normalizePath(file.path(proj_root, "js-pkg", "gog"), mustWork = TRUE)
js_cli <- Sys.getenv("GOG_CLI_PATH")

js_exe <- Sys.getenv("GOG_BOOK_NODE", unset = "")
if (!nzchar(js_exe)) js_exe <- Sys.which("node")
if (!nzchar(js_exe))
  stop("book: no node on PATH, and bindings/javascript.qmd needs one.")

# The chapter's table, written where JavaScript can read it. JSON rather than a
# CSV, because JavaScript parses JSON with nothing installed and a CSV with
# something installed — the same reasoning that put a CSV in the Python chapter,
# pointing the other way.
js_data <- file.path(tempdir(), "gapminder_2007.json")
writeLines(jsonlite::toJSON(as.list(gapminder_2007), dataframe = "columns"), js_data)

# The preamble every snippet runs under. The reader is shown the sentence, not
# this: the import path because the package is not published anywhere yet, and
# `GOG_CLI_PATH` because the engine is a local build. Both are the JavaScript
# spelling of what `setup.R` does for R.
#
# `Object.assign(globalThis, gog)` rather than a fifty-name destructuring: the
# grammar is meant to be spoken bare, and what the reader is shown is the
# sentence, not the ceremony that let it be written.
js_preamble <- function() {
  paste0(
    "import fs from 'node:fs';\n",
    "import * as gog from ", shQuote(file.path(js_root, "src", "index.js"), type = "sh"), ";\n",
    "Object.assign(globalThis, gog);\n",
    "process.env.GOG_CLI_PATH = ", shQuote(js_cli, type = "sh"), ";\n",
    "const gapminder_2007 = JSON.parse(fs.readFileSync(",
    shQuote(js_data, type = "sh"), ", 'utf8'));\n"
  )
}

# Run a snippet, returning its stdout and stderr and whether it failed.
js_run <- function(code) {
  script <- tempfile(fileext = ".mjs")
  on.exit(unlink(script), add = TRUE)
  writeLines(paste0(js_preamble(), code), script)

  errors <- tempfile()
  on.exit(unlink(errors), add = TRUE)
  out <- suppressWarnings(
    system2(js_exe, shQuote(script), stdout = TRUE, stderr = errors)
  )
  status <- attr(out, "status")
  list(
    stdout = paste(out, collapse = "\n"),
    stderr = paste(readLines(errors, warn = FALSE), collapse = "\n"),
    failed = !is.null(status) && status != 0L
  )
}

# The code block the reader sees. Fenced as JavaScript for the highlighter; the
# content is the same string that was just executed, so the two cannot drift.
js_block <- function(code) paste0("\n```js\n", trimws(code), "\n```\n")

#' Show a JavaScript sentence and the plot it drew.
#'
#' @param code One gog sentence, written in JavaScript.
#' @param setup Code the sentence needs but the reader has already seen, run
#'   before it and not shown again. Each snippet on the page is its own process,
#'   so a table built in an earlier block has to be rebuilt here.
#' @param echo Show the sentence as well as the plot.
js_plot <- function(code, setup = "", echo = TRUE) {
  result <- js_run(paste0(setup, "\nprocess.stdout.write(render_svg(", code, "));"))
  if (result$failed) {
    stop("book: the JavaScript sentence did not draw.\n", code, "\n", result$stderr,
         call. = FALSE)
  }
  knitr::asis_output(paste0(
    if (echo) js_block(code) else "",
    svg_block(result$stdout)
  ))
}

#' Show a JavaScript sentence that the grammar refuses, and the message it gives.
#'
#' The counterpart to a `#| error: true` chunk, and it asserts what that option
#' only tolerates: a sentence here that *renders* stops the render. The book once
#' carried a documented refusal that had quietly stopped refusing, which is what
#' `check_refusals.R` exists to catch; a helper that can make the same mistake
#' would be a step backwards.
js_error <- function(code, setup = "") {
  result <- js_run(paste0(
    setup, "\n",
    "try {\n",
    "  render_svg(", code, ");\n",
    "  console.log('DREW');\n",
    "} catch (error) {\n",
    "  if (!(error instanceof GogError)) throw error;\n",
    "  console.log(error.message);\n",
    "}\n"
  ))
  if (result$failed || identical(trimws(result$stdout), "DREW")) {
    stop("book: this sentence is documented as a refusal and did not refuse.\n",
         code, "\n", result$stderr, call. = FALSE)
  }
  knitr::asis_output(paste0(
    js_block(code),
    "\n```\n", trimws(result$stdout), "\n```\n"
  ))
}

#' Show a JavaScript snippet and whatever it printed.
js_show <- function(code, setup = "") {
  result <- js_run(paste0(setup, "\n", code))
  if (result$failed) {
    stop("book: the JavaScript snippet failed.\n", code, "\n", result$stderr,
         call. = FALSE)
  }
  knitr::asis_output(paste0(
    js_block(code),
    if (nzchar(trimws(result$stdout)))
      paste0("\n```\n", trimws(result$stdout), "\n```\n") else ""
  ))
}
