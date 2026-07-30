# book/R/julia.R — how a Julia sentence gets into an R-engine book
#
# The third of these, and the same argument as `python.R` and `javascript.R`: the
# book is a knitr project, every chapter runs R, and the Julia chapter still has
# to obey the rule every other page does — a code block the reader sees is a block
# the engine actually ran, because a block that does not run is how a manual ends
# up documenting things that do not work.
#
# So the chapter does not *show* Julia, it *runs* it. `jl_plot()` hands the
# sentence to `julia`, which builds the specification with the Julia binding and
# shells out to the same `gog-cli` every R chunk uses; what comes back is the SVG,
# embedded beside the sentence that drew it. A sentence the binding refuses stops
# the render.
#
# One thing differs from the other two, and it is the chapter's subject. Nine of
# the fifty kernel words are also Base words, and Julia refuses to pick a winner
# between two modules exporting one name, so the preamble imports those nine
# explicitly. The reader is shown that line on the page rather than having it
# hidden here, because it is a real thing they will have to write.

jl_root <- normalizePath(file.path(proj_root, "jl-pkg", "GrammarOfGraphics"),
                         mustWork = TRUE)
jl_cli <- Sys.getenv("GOG_CLI_PATH")

jl_exe <- Sys.getenv("GOG_BOOK_JULIA", unset = "")
if (!nzchar(jl_exe)) jl_exe <- Sys.which("julia")
if (!nzchar(jl_exe)) {
  juliaup <- file.path(path.expand("~"), ".juliaup", "bin", "julia")
  if (file.exists(juliaup)) jl_exe <- juliaup
}
if (!nzchar(jl_exe))
  stop("book: no julia on PATH, and bindings/julia.qmd needs one. ",
       "Install it with `curl -fsSL https://install.julialang.org | sh`, ",
       "or point GOG_BOOK_JULIA at one.")

# The chapter's table, written where Julia can read it — as Julia. A NamedTuple
# of vectors is the language with nothing installed, which is the same claim the
# Python chapter makes with a CSV and the JavaScript one with JSON, each in the
# form its reader would actually reach for.
jl_column <- function(values) {
  if (is.factor(values)) values <- as.character(values)
  if (is.character(values)) {
    paste0("[", paste(sprintf('"%s"', gsub('"', '\\\\"', values)), collapse = ", "), "]")
  } else {
    paste0("[", paste(format(values, trim = TRUE, scientific = FALSE), collapse = ", "), "]")
  }
}

jl_data <- file.path(tempdir(), "gapminder_2007.jl")
writeLines(paste0(
  "gapminder_2007 = (",
  paste(vapply(names(gapminder_2007),
               function(n) paste0(n, " = ", jl_column(gapminder_2007[[n]])),
               character(1)), collapse = ", "),
  ",)"), jl_data)

# A Julia string literal, which is **not** `shQuote()`. Single quotes delimit a
# `Char` in Julia, so the shell spelling that works for Python and JavaScript is a
# parse error here — caught by the first render of this chapter, which is what a
# live chunk is for.
jl_string <- function(s) paste0('"', gsub('"', '\\\\"', gsub("\\\\", "\\\\\\\\", s)), '"')

# The preamble every snippet runs under. The reader is shown the sentence, not
# this — except for the nine-word import, which the chapter shows on purpose.
jl_preamble <- function() {
  paste0(
    "ENV[\"GOG_CLI_PATH\"] = ", jl_string(jl_cli), "\n",
    "using GrammarOfGraphics\n",
    "using GrammarOfGraphics: bin, count, sum, min, max, range, size, step, stack\n",
    "include(", jl_string(jl_data), ")\n"
  )
}

jl_run <- function(code) {
  script <- tempfile(fileext = ".jl")
  on.exit(unlink(script), add = TRUE)
  writeLines(paste0(jl_preamble(), code), script)

  errors <- tempfile()
  on.exit(unlink(errors), add = TRUE)
  out <- suppressWarnings(system2(
    jl_exe, c(paste0("--project=", shQuote(jl_root)), "--startup-file=no",
              shQuote(script)),
    stdout = TRUE, stderr = errors
  ))
  status <- attr(out, "status")
  list(
    stdout = paste(out, collapse = "\n"),
    stderr = paste(readLines(errors, warn = FALSE), collapse = "\n"),
    failed = !is.null(status) && status != 0L
  )
}

# The code block the reader sees. Fenced as Julia for the highlighter; the content
# is the same string that was just executed, so the two cannot drift.
jl_block <- function(code) paste0("\n```julia\n", trimws(code), "\n```\n")

#' Show a Julia sentence and the plot it drew.
jl_plot <- function(code, setup = "", echo = TRUE) {
  result <- jl_run(paste0(setup, "\nprint(render_svg(", code, "))"))
  if (result$failed) {
    stop("book: the Julia sentence did not draw.\n", code, "\n", result$stderr,
         call. = FALSE)
  }
  knitr::asis_output(paste0(
    if (echo) jl_block(code) else "",
    svg_block(result$stdout)
  ))
}

#' Show a Julia sentence that the grammar refuses, and the message it gives.
#'
#' The counterpart to a `#| error: true` chunk, and it asserts what that option
#' only tolerates: a sentence here that *renders* stops the render.
jl_error <- function(code, setup = "") {
  result <- jl_run(paste0(
    setup, "\n",
    "try\n",
    "    render_svg(", code, ")\n",
    "    println(\"DREW\")\n",
    "catch error\n",
    "    error isa GogError || rethrow()\n",
    "    println(error.msg)\n",
    "end\n"
  ))
  if (result$failed || identical(trimws(result$stdout), "DREW")) {
    stop("book: this sentence is documented as a refusal and did not refuse.\n",
         code, "\n", result$stderr, call. = FALSE)
  }
  knitr::asis_output(paste0(
    jl_block(code),
    "\n```\n", trimws(result$stdout), "\n```\n"
  ))
}

#' Show a Julia snippet and whatever it printed.
jl_show <- function(code, setup = "") {
  result <- jl_run(paste0(setup, "\n", code))
  if (result$failed) {
    stop("book: the Julia snippet failed.\n", code, "\n", result$stderr, call. = FALSE)
  }
  knitr::asis_output(paste0(
    jl_block(code),
    if (nzchar(trimws(result$stdout)))
      paste0("\n```\n", trimws(result$stdout), "\n```\n") else ""
  ))
}
