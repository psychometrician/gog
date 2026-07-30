# check_refusals.R — does every documented refusal actually refuse?
#
# `#| error: true` does **not** assert that a chunk errors. It *tolerates* an
# error, so Quarto keeps building instead of halting. The consequence is a blind
# spot exactly the shape of the `total`-transform prose bug that
# check_vocabulary.R exists to catch: a chunk the book presents as a refusal can
# quietly stop refusing, render a plot instead, and every build still exits 0.
#
# That happened. `writing.qmd`'s "Binning a category" chunk
# (`bar * bin + x(continent)`) sat under prose promising "the refusal points at
# the atom that does count categories" while it drew an **empty panel** with
# fabricated 0..1 axes — because `bin` answered the type question in
# `transform.rs`, downstream of the legality gate, where it could only warn and
# then hand the renderer an empty frame. 47 of the book's 48 `error: true`
# chunks refused correctly; this one did not, and nothing could tell.
#
# So the invariant, checked here: **an `error: true` chunk must error.** A chunk
# that renders is a false claim whether or not it printed a warning first —
# warning and then drawing is the silent-drop this project forbids (spec §12),
# not a refusal.
#
# Run from the repo root; sourced by r-pkg/gog/tests/test_basic.R.

check_refusals <- function(book = "book") {
  fail <- function(...) stop(..., call. = FALSE)

  if (!nzchar(Sys.getenv("GOG_CLI_PATH"))) {
    for (build in c("release", "debug")) {
      p <- file.path("target", build, "gog-cli")
      if (file.exists(p)) { Sys.setenv(GOG_CLI_PATH = normalizePath(p)); break }
    }
  }
  if (!nzchar(Sys.getenv("GOG_CLI_PATH")))
    fail("FAIL: no gog-cli binary — run `cargo build --release` first")

  # The example frames the chunks bind. Sourced into an env the chunks inherit,
  # so a chunk that assigns (the partial-spec refusal does) cannot leak into the
  # next one.
  book_env <- new.env(parent = globalenv())
  sys.source(file.path(book, "R", "data.R"), envir = book_env)

  # Chapters live at the root and in subdirectories; listed on purpose, so the
  # scan cannot wander into _book/ or .quarto/ (check_vocabulary.R does the same).
  qmd <- c(list.files(book, pattern = "\\.qmd$", full.names = TRUE),
           unlist(lapply(file.path(book, c("marks", "parts", "cookbook", "bindings")),
                         list.files, pattern = "\\.qmd$", full.names = TRUE)))

  drew <- character()
  n <- 0L

  for (f in qmd) {
    ln <- readLines(f, warn = FALSE)
    for (s in grep("^```\\{r\\}\\s*$", ln)) {
      e <- s + 1L
      while (e <= length(ln) && !grepl("^```\\s*$", ln[e])) e <- e + 1L
      body <- ln[(s + 1L):(e - 1L)]
      if (!any(grepl("error:\\s*true", grep("^#\\|", body, value = TRUE)))) next
      code <- body[!grepl("^#\\|", body)]
      code <- code[nzchar(trimws(code))]
      if (!length(code)) next
      n <- n + 1L

      where <- sprintf("%s:%d", basename(f), s)
      chunk_env <- new.env(parent = book_env)
      outcome <- tryCatch({
        v <- eval(parse(text = paste(code, collapse = "\n")), envir = chunk_env)
        # A spec is inert until it is drawn — knit_print renders it, so the check
        # must too, or every refusal would look like a pass.
        if (inherits(v, "gog_spec")) {
          suppressMessages(render_svg(v))
          "drew a plot"
        } else "evaluated without error"
      }, error = function(e) NULL)

      if (!is.null(outcome))
        drew <- c(drew, sprintf("%s (%s): %s", where, outcome,
                                paste(trimws(code), collapse = " ")))
    }
  }

  if (!n) fail("FAIL: found no `error: true` chunks — the scan is broken, not the book")
  if (length(drew))
    fail("FAIL: presented as refusals, but did not refuse:\n  ",
         paste(drew, collapse = "\n  "),
         "\n  Either the engine stopped refusing, or the prose should not claim it does.")

  cat("PASS: every documented refusal refuses (", n, "`error: true` chunks )\n")
  invisible(TRUE)
}
