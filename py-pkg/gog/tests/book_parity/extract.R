# extract.R — take the manual's sentences, and what the engine drew for them
#
# The book is already the R binding's integration test: every plot in it comes
# from the compiled engine, so a chapter that renders is a chapter that works.
# This script makes the same 500-odd sentences available to a *second* binding,
# by recording three things per sentence: the R source exactly as the manual
# writes it, the SVG the engine drew for it (or the refusal it earned), and the
# tables it was resolved against.
#
# It works by re-running the chapters rather than pattern-matching them. A
# chunk's expressions are parsed with their source text kept, evaluated in
# order so that a frame defined in chunk 3 is there for chunk 4, and an
# expression is recorded as a sentence when its *value* is a `gog_spec` — not
# when its text looks like one. That is what lets it pick up
# `data(df) + point + …` and skip `knitr::kable(…)` without a list of
# exceptions.
#
# Run from the project root:
#   Rscript py-pkg/gog/tests/book_parity/extract.R

suppressMessages(pkgload::load_all("r-pkg/gog", export_all = FALSE, quiet = TRUE))
df_to_wire <- gog:::df_to_wire

root <- normalizePath(".")
out_dir <- file.path(root, "py-pkg", "gog", "tests", "book_parity", "corpus")
dir.create(out_dir, showWarnings = FALSE, recursive = TRUE)

setwd(file.path(root, "book"))
Sys.setenv(GOG_CLI_PATH = file.path(root, "target", "release", "gog-cli"))

# What every chapter loads on its first line. `setup.R` rather than `data.R`
# alone, because the mark chapters call its generated-options helper, and a
# chunk that cannot run is a chunk whose sentences go unrecorded.
shared <- new.env(parent = globalenv())
sys.source(file.path(root, "book", "R", "setup.R"), envir = shared)

chapters <- c(list.files(".", pattern = "\\.qmd$"),
              list.files(c("marks", "cookbook", "parts", "bindings"),
                         pattern = "\\.qmd$", full.names = TRUE))

# ---------------------------------------------------------------------------
# Chunk splitting — the options matter as much as the code
#
# `#| error: true` marks a chunk the book presents as a *refusal*; the sentence
# in it is expected to fail, and the message is the documented output. Those are
# as much a part of the manual as the plots, so they are recorded too, with the
# refusal in place of the SVG.
# ---------------------------------------------------------------------------

chunks_of <- function(path) {
  lines <- readLines(path, warn = FALSE)
  starts <- grep("^```\\{r", lines)
  out <- list()
  for (start in starts) {
    ends <- grep("^```\\s*$", lines)
    ends <- ends[ends > start]
    if (!length(ends)) next
    body <- lines[(start + 1):(ends[1] - 1)]
    options <- grepl("^#\\|", body)
    out[[length(out) + 1]] <- list(
      code    = body[!options],
      error   = any(grepl("error:\\s*true", body[options])),
      skip    = any(grepl("eval:\\s*false", body[options]))
    )
  }
  out
}

sentences <- list()
tables <- list()
skipped <- list()
stats <- c(chunks = 0, expressions = 0, sentences = 0, chunk_errors = 0)

# ---------------------------------------------------------------------------
# A sentence carries its own tables, taken from the spec it built
#
# Snapshotting per *chapter* is not good enough, and the manual proves it:
# `data.qmd` defines `severity` twice on purpose — once as a text column to show
# row order, then again as a factor to show declared order — so an end-of-chapter
# snapshot hands the first sentence the second table and quietly draws a
# different plot. A `gog_spec` already holds the frames it resolved against, so
# taking them from the object is both exact and cheaper than guessing.
#
# The frames are interned into a pool, because 481 sentences that each carry
# their own copy of gapminder is a corpus nobody can commit.
# ---------------------------------------------------------------------------

pool <- list()
pool_json <- character()

intern <- function(frame) {
  wire <- df_to_wire(frame)
  json <- as.character(jsonlite::toJSON(wire, digits = NA, na = "null"))
  hit <- match(json, pool_json)
  if (is.na(hit)) {
    pool_json <<- c(pool_json, json)
    pool[[length(pool) + 1]] <<- wire
    hit <- length(pool)
  }
  hit
}

for (chapter in chapters) {
  env <- new.env(parent = shared)
  chapter_id <- sub("\\.qmd$", "", sub("^\\./", "", chapter))

  for (chunk in chunks_of(chapter)) {
    if (chunk$skip || !length(chunk$code)) next
    stats["chunks"] <- stats["chunks"] + 1

    parsed <- tryCatch(parse(text = chunk$code, keep.source = TRUE),
                       error = function(e) NULL)
    if (is.null(parsed)) next
    refs <- attr(parsed, "srcref")

    for (i in seq_along(parsed)) {
      expression <- parsed[[i]]
      source_text <- paste(as.character(refs[[i]]), collapse = "\n")
      stats["expressions"] <- stats["expressions"] + 1

      value <- tryCatch(withCallingHandlers(eval(expression, env),
                                            message = function(m) invokeRestart("muffleMessage")),
                        error = function(e) structure(conditionMessage(e), class = "gog_refusal"))

      # An assignment returns its value invisibly, so `p <- data(…) + …` is
      # picked up here exactly like the bare form.
      if (inherits(value, "gog_refusal")) {
        # Only a refusal the book *documents* is a sentence; a genuine error in
        # setup code is a broken harness and should be visible as one.
        if (!chunk$error) {
          stats["chunk_errors"] <- stats["chunk_errors"] + 1
          skipped[[length(skipped) + 1]] <- paste0(
            chapter_id, ": ", substr(gsub("\\s+", " ", source_text), 1, 70),
            "  →  ", substr(as.character(value), 1, 90))
          next
        }
        outcome <- paste0("REFUSED\n", as.character(value))
      } else if (inherits(value, "gog_spec") || inherits(value, "gog_page")) {
        # A *page* of plots is recorded exactly as a plot is: it is a figure the
        # engine drew, and the whole point of composition is that nothing else
        # about it is special (spec §11).
        # A plot is recorded as the SHA-256 of its SVG, not the SVG. 481 plots
        # is 8.3 MB of markup that nobody reads and git would carry forever;
        # the hash is what the comparison actually uses, and it lets the check
        # run in an environment with no R in it. A refusal keeps its full text,
        # because there the *words* are the thing being compared.
        outcome <- tryCatch(
          paste0("SVG ", digest::digest(
            sub("[[:space:]]+$", "", suppressMessages(render_svg(value))),
            algo = "sha256", serialize = FALSE)),
          error = function(e) paste0("REFUSED\n", conditionMessage(e))
        )
      } else {
        next
      }

      # The tables this sentence actually resolved against. Absent only when the
      # binding refused before a spec existed, where the chapter's frames are
      # the honest fallback.
      used <- list()
      if (inherits(value, "gog_spec") || inherits(value, "gog_page")) {
        for (name in names(value$data_frames)) {
          used[[name]] <- intern(value$data_frames[[name]])
        }
      }

      stats["sentences"] <- stats["sentences"] + 1
      sentences[[length(sentences) + 1]] <- list(
        id       = paste0(chapter_id, "#", stats["sentences"]),
        chapter  = chapter_id,
        source   = source_text,
        expects  = if (chunk$error) "refusal" else "plot",
        outcome  = outcome,
        tables   = used
      )
    }
  }

  # Every frame the chapter could see, including ones a chunk built itself.
  # Recorded per chapter so the Python side resolves the same names against the
  # same values — the shared ones are dumped once, under "".
  for (name in ls(env)) {
    value <- get(name, envir = env)
    if (is.data.frame(value) && is.null(tables[[paste0(chapter_id, "/", name)]])) {
      tables[[paste0(chapter_id, "/", name)]] <- intern(value)
    }
  }
}

# The shared frames, from both places a chapter can put them. `source()` defaults
# to `local = FALSE`, so a chapter's own `source("R/setup.R")` line lands its
# frames in the global environment rather than in the chapter's — which is why
# both are swept here. Missing this is not a subtle failure: it left 432 of 481
# sentences unable to name their table.
for (env_holding_frames in list(shared, globalenv())) {
  for (name in ls(env_holding_frames)) {
    value <- get(name, envir = env_holding_frames)
    if (is.data.frame(value)) tables[[paste0("/", name)]] <- intern(value)
  }
}

writeLines(jsonlite::toJSON(sentences, auto_unbox = TRUE, digits = NA),
           file.path(out_dir, "sentences.json"))
# The chapter fallback stores pool *indices*, not frames — the same gapminder
# would otherwise be written twice, once here and once for the sentences that
# name it.
writeLines(jsonlite::toJSON(tables, auto_unbox = TRUE),
           file.path(out_dir, "tables.json"))
writeLines(jsonlite::toJSON(pool, auto_unbox = FALSE, digits = NA, na = "null"),
           file.path(out_dir, "pool.json"))

cat(sprintf("chunks %d | expressions %d | sentences %d | tables %d (%d distinct) | non-sentence errors %d\n",
            stats["chunks"], stats["expressions"], stats["sentences"],
            length(tables), length(pool), stats["chunk_errors"]))

# Record what this corpus was recorded *against* — the engine that drew every
# hash in it, and the live chunks of every chapter it read. `run.py` refuses to
# report a pass when either has moved since, which is the difference between a
# comparison and a fossil.
#
# Shelled out to `corpus_stamp.py` rather than re-derived here: two spellings of
# one hash is precisely the drift the manifest exists to catch, and R computing
# it one way while Python checks it another would be a stamp that always
# disagreed. The harness already requires Python to run at all.
stamp <- system2("python3",
                 c(shQuote(file.path(root, "py-pkg", "gog", "tests", "book_parity",
                                     "corpus_stamp.py")),
                   "write", shQuote(root), shQuote(Sys.getenv("GOG_CLI_PATH"))),
                 stdout = TRUE, stderr = TRUE)
cat(paste(stamp, collapse = "\n"), "\n")
if (!is.null(attr(stamp, "status")) && attr(stamp, "status") != 0) {
  stop("could not stamp the corpus — it would read as current forever after")
}

# Anything here is code the harness could not run — printed rather than
# counted, because a silently skipped chunk is a silently untested sentence.
if (length(skipped)) {
  cat("\nnot run (not marked `error: true`):\n")
  for (line in skipped) cat("  ", line, "\n", sep = "")
}
