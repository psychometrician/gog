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
  unready <- character()
  n <- 0L

  # A chapter's chunks run in order when the book renders, so a refusal may
  # bind a table an earlier chunk built (`wide` in the Data chapter, `revenue`
  # in Scales). Evaluated alone, such a chunk errors with "object not found"
  # before the engine ever sees the sentence, and that error used to count as
  # the refusal: seven chunks passed that way. So each chapter's earlier chunks
  # are evaluated first, into an environment the refusals inherit. Building a
  # spec is inert (nothing renders until knit_print), so this is cheap; the few
  # chunks that render or shell out on their own are skipped, and an earlier
  # chunk that fails is simply left out, since it is not the thing under test.
  inert <- "render_svg\\(|save_gif\\(|(py|jl|js)_[a-z]+\\(|tab_|source\\(|find_gog_cli|system2\\(|peek\\(|mark_options\\(|kable\\(|query\\("

  for (f in qmd) {
    ln <- readLines(f, warn = FALSE)
    chapter_env <- new.env(parent = book_env)
    for (s in grep("^```\\{r\\}\\s*$", ln)) {
      e <- s + 1L
      while (e <= length(ln) && !grepl("^```\\s*$", ln[e])) e <- e + 1L
      body <- ln[(s + 1L):(e - 1L)]
      opts <- grep("^#\\|", body, value = TRUE)
      code <- body[!grepl("^#\\|", body)]
      code <- code[nzchar(trimws(code))]
      if (!length(code)) next
      text <- paste(code, collapse = "\n")

      if (!any(grepl("error:\\s*true", opts))) {
        if (any(grepl("eval:\\s*false|include:\\s*false", opts))) next
        if (grepl(inert, text)) next
        try(suppressWarnings(suppressMessages(
          eval(parse(text = text), envir = chapter_env))), silent = TRUE)
        next
      }
      n <- n + 1L

      where <- sprintf("%s:%d", basename(f), s)
      chunk_env <- new.env(parent = chapter_env)
      err <- NULL
      outcome <- tryCatch({
        v <- eval(parse(text = text), envir = chunk_env)
        # A spec is inert until it is drawn — knit_print renders it, so the check
        # must too, or every refusal would look like a pass. A composed page is
        # inert the same way and prints through the same method, so it is
        # rendered here too; a page-level refusal was invisible until it was.
        if (inherits(v, c("gog_spec", "gog_page"))) {
          suppressMessages(render_svg(v))
          "drew a plot"
        } else "evaluated without error"
      }, error = function(e) { err <<- conditionMessage(e); NULL })

      if (!is.null(outcome))
        drew <- c(drew, sprintf("%s (%s): %s", where, outcome,
                                paste(trimws(code), collapse = " ")))
      # A refusal is the engine's. A sentence that never reached it, because a
      # name it binds does not exist, is a missing table rather than a refusal;
      # a chunk with no sentence at all (the R chapter's masking demonstrations)
      # is R's own error and is what the chunk is there to show.
      else if (grepl("\\b(data|query)\\(", text) &&
               grepl("object '[^']*' not found|could not find function", err))
        unready <- c(unready, sprintf("%s: %s", where, sub("\n.*$", "", err)))
    }
  }

  if (!n) fail("FAIL: found no `error: true` chunks — the scan is broken, not the book")
  if (length(drew))
    fail("FAIL: presented as refusals, but did not refuse:\n  ",
         paste(drew, collapse = "\n  "),
         "\n  Either the engine stopped refusing, or the prose should not claim it does.")
  if (length(unready))
    fail("FAIL: presented as refusals, but errored before the engine saw the sentence:\n  ",
         paste(unready, collapse = "\n  "),
         "\n  The table it binds is not built by an earlier chunk of the chapter, or that chunk is skipped here.")

  cat("PASS: every documented refusal refuses (", n, "`error: true` chunks )\n")
  invisible(TRUE)
}
