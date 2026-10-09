# book/check_tabs.R
# A sentence must never lose a language tab without saying why.
#
# `book/R/tabs.R` already separates the two ways a tab can be absent. A
# translator that **declines** a sentence reports a reason and is passed over
# quietly: the R chapter's pipes have no Python spelling, and a table computed
# in R is R arithmetic rather than a gog sentence. A translator that **misses**
# one reports nothing, and that is a defect — the tab silently does not appear,
# and a reader of that language sees a page the others get.
#
# `tabs.R` warns about a miss. That is where this check comes from: it warns,
# and the render exits 0, and nobody reads the log. On 2026-07-28 twelve of them
# had been reported on every build for an unknown length of time, and the only
# reason they were noticed at all is that somebody happened to grep a render log
# for the word "warning". (They turned out not to be misses — the JavaScript and
# Julia emitters were declining without giving a reason, so a legitimate refusal
# was being counted as a failure. The fix was one return value in each. But a
# real miss would have looked exactly the same and would have been just as
# invisible.)
#
# So this is the sixth prose guard, and unlike `tabs.R`'s warning it can fail.
# The project's own standard: a rule with no check that can break is a rule
# that goes stale. `check_refusals.R` exists because an
# `error: true` chunk could quietly stop refusing; this exists because a
# sentence can quietly stop being translatable.
#
# It reuses the emitters rather than reimplementing them, so it cannot disagree
# with what the book actually renders.
#
# It has a second job, added on 2026-10-08 at the author's word ("we should
# always have tabset for other languages"). A chunk that builds or subsets a
# table in R is not tabbable at all, so it never reaches a translator and the
# first job never sees it: the page shows R alone, with no warning. Twenty-nine
# chunks were in that state at once, and this guard's first job had passed them
# over on purpose, as chunks for which no tab was expected. Now they are a
# defect too. Every chunk the reader sees that holds a sentence must be
# tabbable, outside the four bindings chapters, each of which is about one
# language. The failure names the two fixes: a table built in
# `book/R/make-data.R` and read as a shared one, or, for R around a finished
# plot, the same example in all four languages with `run_spellings()`.

# `root` is the repository, where the emitters live. It is the book's parent
# unless a probe book elsewhere is being checked against this repository's emitters.
check_tabs <- function(book_dir = "book",
                       root = normalizePath(file.path(book_dir, ".."), mustWork = FALSE)) {

  js_emitter <- file.path(root, "js-pkg", "gog", "test", "book_parity", "translate.R")
  jl_emitter <- file.path(root, "jl-pkg", "GrammarOfGraphics", "test",
                          "book_parity", "translate.R")
  if (!file.exists(js_emitter) || !file.exists(jl_emitter)) {
    cat("SKIP: the JavaScript/Julia emitters are not here to ask\n")
    return(invisible(TRUE))
  }
  env <- new.env(parent = globalenv())
  sys.source(js_emitter, envir = env)
  sys.source(jl_emitter, envir = env)

  # Ask `tabs.R` itself which chunks earn a tab, rather than approximating it
  # here. The first draft of this check approximated, and immediately reported
  # 30 "misses" that the book never sends to a translator at all — a chunk that
  # manipulates a table in R (`gm$log_gdp <- log10(gm$gdp)`) is neither a
  # sentence nor a table definition, so `is_tabbable()` is already false and no
  # translator is asked. A guard that disagrees with the thing it guards is
  # worse than no guard. Such a chunk is caught by the second job below instead,
  # as a chunk shown in R alone.
  #
  # `tabs.R` needs `proj_root` at load and does nothing else at load time.
  env$proj_root <- root
  suppressWarnings(sys.source(file.path(book_dir, "R", "tabs.R"), envir = env))

  # The chunk reader. A refusal chunk (`error: true`) is tabbed like any other
  # sentence, so it is read here exactly as `tabs.R` reads it; only a chunk that
  # is never shown (`include: false`) is skipped.
  chunks_of <- function(path) {
    lines <- readLines(path, warn = FALSE)
    out <- character()
    i <- 1L
    while (i <= length(lines)) {
      if (grepl("^```\\{r", lines[i])) {
        j <- i + 1L
        while (j <= length(lines) && !grepl("^```\\s*$", lines[j])) j <- j + 1L
        body <- lines[seq.int(i + 1L, j - 1L)]
        opts <- body[grepl("^#\\|", body)]
        if (!any(grepl("include:\\s*false", opts)))
          out <- c(out, paste(body[!grepl("^#\\|", body)], collapse = "\n"))
        i <- j + 1L
      } else i <- i + 1L
    }
    out
  }

  # `tabs.R`'s own predicate, so this check asks exactly what the book asks.
  looks_like_sentence <- env$is_tabbable

  qmds <- list.files(book_dir, pattern = "[.]qmd$", recursive = TRUE, full.names = TRUE)
  qmds <- qmds[!grepl("/_book/", qmds, fixed = TRUE)]

  misses <- character(0)
  scanned <- 0L

  for (path in qmds) {
    short <- sub("^.*book/", "", path)
    for (code in chunks_of(path)) {
      if (!looks_like_sentence(code)) next
      scanned <- scanned + 1L
      for (lang in c("js", "julia")) {
        got <- tryCatch(
          if (lang == "js") env$translate_js(code) else env$translate_julia(code),
          error = function(e) NULL
        )
        if (is.null(got)) {
          misses <- c(misses, sprintf("  %s  [%s: the emitter raised an error]", short, lang))
          next
        }
        spelling <- if (lang == "js") got$js else got$julia
        # The defect: no spelling *and* no reason. Either alone is fine — a
        # spelling is the normal case, and a reason is an honest decline.
        if (is.na(spelling) && is.na(got$blocked)) {
          first <- sub("\\s+", " ", substr(gsub("\\s+", " ", code), 1, 64))
          misses <- c(misses, sprintf("  %s  [no %s tab, no reason given]  %s…",
                                      short, lang, first))
        }
        # The second defect, and the worse one: a spelling that *is* a refusal.
        # `NA_character_` is how every branch of the emitters says "I cannot
        # spell this", and `paste()` renders it as the two characters `NA` —
        # so a refusal caught inside a `c(...)` or a list came back out as a
        # literal, and the emitter reported success. Neither language has a
        # word `NA`. `marks/zone.qmd` shipped
        # `const edges = { sales: [NA, NA] };` to readers this way, and nothing
        # noticed, because a check for *absent* tabs cannot see a *wrong* one.
        # A bare `NA` in emitted code always means a refusal was laundered.
        if (!is.na(spelling) && grepl("\\bNA\\b", spelling)) {
          culprit <- grep("\\bNA\\b", strsplit(spelling, "\n")[[1]], value = TRUE)[1]
          misses <- c(misses, sprintf(
            "  %s  [%s tab contains a bare NA — a refusal was laundered]\n      %s",
            short, lang, substr(trimws(culprit), 1, 72)))
        }
      }
    }
  }

  # The second job: a chunk the reader sees, holding a sentence, that the hook
  # will show in R alone. Hidden chunks are skipped (`peek()`, and the
  # `run_spellings()` blocks that write their four tabs themselves), and so is
  # a chunk with no `data(`, which holds no sentence.
  alone <- character(0)
  for (path in qmds[!grepl("/bindings/", qmds, fixed = TRUE)]) {
    lines <- readLines(path, warn = FALSE)
    ends <- grep("^```\\s*$", lines)
    for (s in grep("^```\\{r", lines)) {
      e <- ends[ends > s][1]
      if (is.na(e)) next
      body <- if (e > s + 1L) lines[(s + 1L):(e - 1L)] else character(0)
      opts <- c(lines[s], body[grepl("^#\\|", body)])
      if (any(grepl("echo:\\s*false|include:\\s*false|echo\\s*=\\s*FALSE", opts))) next
      code <- paste(body[!grepl("^#\\|", body)], collapse = "\n")
      if (!grepl("\\bdata\\(", code) || looks_like_sentence(code)) next
      first <- trimws(body[!grepl("^#\\|", body) & nzchar(trimws(body))][1])
      alone <- c(alone, sprintf("  %s:%d  %s", sub("^.*book/", "", path), s,
                                substr(first, 1, 64)))
    }
  }
  if (length(alone)) {
    cat("FAIL: a chunk is shown in R alone\n")
    cat(paste(alone, collapse = "\n"), "\n")
    cat("  A chunk gets its Python, Julia and JavaScript tabs only when it holds\n",
        "  nothing but gog sentences and data.frame() tables. Two fixes:\n",
        "  * a table built or subset in R: build it in book/R/make-data.R, read it\n",
        "    in book/R/data.R, show it with peek(), and leave the sentence alone;\n",
        "  * R around a finished plot (render_svg(), save_svg(), save_gif()): write\n",
        "    it once per language in an echo: false chunk with run_spellings().\n",
        sep = "")
    stop("check_tabs: ", length(alone), " chunk(s) shown in R alone")
  }

  if (length(misses)) {
    cat("FAIL: a sentence lost a language tab, or was given a wrong one\n")
    cat(paste(unique(misses), collapse = "\n"), "\n")
    cat("  Either teach the emitter to spell it, or decline it with a reason\n",
        "  (`blocked = \"...\"`) so the book can pass it over honestly. A tab a\n",
        "  reader cannot run is worse than a tab that is not there.\n", sep = "")
    stop("check_tabs: ", length(unique(misses)), " sentence(s) with no tab or a bad one")
  }

  cat("PASS: every sentence has a tab or a stated reason, and none is shown in R alone (",
      scanned, "sentence chunks )\n")
  invisible(TRUE)
}
