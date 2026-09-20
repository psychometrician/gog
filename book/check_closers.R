# check_closers.R — does every teaching chapter end by naming its edge and the
# chapter that answers it?
#
# Added 2026-09-18. Measured first: of 57 chapters, 7 ended on a refused
# specification with no prose after it (all five cookbook recipes, `text` and
# `zone`), 19 more ended on a one-line gloss of that refusal, and only 5 handed
# off to the next chapter at all. Quarto prints the next chapter's *name* in the
# page footer, so what a closer adds is the *reason*, which the footer cannot
# carry.
#
# What a closer is, and what it is deliberately not. Two sentences: the first
# says what this chapter's subject does not do, the second names the chapter
# that does and the question it answers. It is **not a summary** — a per-chapter
# recap has been removed from this book twice, as 33 callout boxes folded into
# prose and then as the bolded takeaway sentence they had become, and the
# read-aloud gloss was narrowed on the same reasoning: a device repeated in
# every chapter teaches the reader to skip it unless each instance carries
# something new. A boundary is new. A recap is not.
#
# A chapter may also carry `## In short` immediately before the closer: two to
# four sentences of the rules it fixed, for the chapters whose rules are not
# already their section headings. Measured before it was built: `marks/bar.qmd`
# states 15 rules under 14 headings, **none** of which is a rule, so a reader
# scanning the sidebar cannot get the zero baseline or the orientation rule back;
# `scales.qmd` states 9 of its rules in headings, so it carries only the few they
# hide; `marks/edge.qmd` states three of its four rules in its first 26 lines, so
# it carries none. The test is applied by writing the summary: if its sentences
# are the chapter's own headings in other words, the chapter does not get one.
# That last half is the only part a script can check, and it is checked below.
#
# Four assertions per chapter in scope:
#   1. The last section is `## What comes next`.
#   2. It holds no code chunk. The closer is prose, and a chunk there would be a
#      plot introduced after the chapter has ended.
#   3. It runs to 60 words or fewer, which is the difference between a pointer
#      and the recap this is not.
#   4. It links to the chapter that genuinely follows in `_quarto.yml` order, so
#      reordering the book fails the build rather than leaving stale sentences
#      in 44 chapters. This is the assertion the whole file is for: the prose is
#      hand-written, the order is data, and nothing else joins them.
#
# Scope is read from `_quarto.yml` rather than listed here, so a chapter added
# tomorrow is covered the day it is added — the same reason `check_promises.R`
# reads its own span from the file.
#
# Run from the repo root; sourced by r-pkg/gog/tests/test_basic.R.

check_closers <- function(book = "book") {
  fail <- function(...) stop(..., call. = FALSE)

  HEAD <- "## What comes next"
  SHORT <- "## In short"
  MAX_WORDS <- 60
  MAX_SHORT <- 80

  yml <- readLines(file.path(book, "_quarto.yml"), warn = FALSE)
  # Same reader as `check_promises.R`: list entries only, trailing comments
  # stripped, because `_quarto.yml` names chapters inside comments as well.
  entries <- grep("^\\s*-\\s*(part:\\s*)?[A-Za-z0-9_/-]+\\.qmd\\s*(#.*)?$", yml, value = TRUE)
  listed <- sub("\\s*#.*$", "", sub("^\\s*-\\s*(part:\\s*)?", "", trimws(entries)))

  appendix_at <- grep("^\\s*appendices:", yml)
  if (length(appendix_at)) {
    # Everything after `appendices:` is reference matter and out of scope, but
    # the entries were collected from the whole file. Cut by position.
    n_before <- sum(grepl("^\\s*-\\s*(part:\\s*)?[A-Za-z0-9_/-]+\\.qmd\\s*(#.*)?$",
                          yml[seq_len(appendix_at[1])]))
    listed <- listed[seq_len(n_before)]
  }

  # The reading flow: every page a reader turns, in order, minus the part
  # dividers. `index.qmd` is the preface and `references.qmd` the bibliography;
  # neither teaches, and the afterword closes the book, so it is a destination
  # rather than a chapter with a next.
  flow <- listed[!grepl("^parts/", listed)]
  flow <- flow[!flow %in% c("index.qmd", "references.qmd")]

  # In scope: the teaching chapters. The five cookbook recipes are excluded on
  # purpose — a recipe already ends by linking back to the chapter that owns its
  # pieces, and its shape is fixed elsewhere (question, sentence, plot, why,
  # variations). The afterword is a destination, not a chapter in scope.
  scope <- flow[!grepl("^cookbook/", flow) & flow != "afterword.qmd"]
  if (!length(scope))
    fail("FAIL: check_closers found no chapters — the _quarto.yml scan is broken")

  nxt <- c(flow[-1], "afterword.qmd")
  names(nxt) <- flow

  problems <- character()
  for (f in scope) {
    path <- file.path(book, f)
    ln <- readLines(path, warn = FALSE)

    # Headings outside code fences only: a chunk can hold an R comment that
    # starts with `## `, and a blank line has to precede a heading or pandoc
    # renders the hashes as text. Both traps are `check_template.R`'s, met here
    # for the same reason.
    fence <- cumsum(grepl("^```", ln)) %% 2 == 1
    h2_at <- which(!fence & grepl("^## ", ln))
    h2_at <- h2_at[h2_at == 1 | !nzchar(trimws(ln[pmax(h2_at - 1, 1)]))]

    if (!length(h2_at) || trimws(ln[tail(h2_at, 1)]) != HEAD) {
      last <- if (length(h2_at)) trimws(ln[tail(h2_at, 1)]) else "(no sections)"
      problems <- c(problems, sprintf("%s: ends on `%s`, expected `%s`",
                                      f, last, HEAD))
      next
    }

    # `## In short`, where a chapter has one, is the section immediately before
    # the closer, holds no chunk, and says something the headings do not.
    short_at <- which(trimws(ln[h2_at]) == SHORT)
    if (length(short_at)) {
      if (short_at != length(h2_at) - 1)
        problems <- c(problems, sprintf(
          "%s: `%s` is not the section before `%s`", f, SHORT, HEAD))
      sbody <- ln[(h2_at[short_at] + 1):(tail(h2_at, 1) - 1)]
      if (any(grepl("^```", sbody)))
        problems <- c(problems, sprintf(
          "%s: `%s` holds a code chunk; it states rules, it does not draw", f, SHORT))
      sw <- length(unlist(strsplit(trimws(paste(sbody, collapse = " ")), "\\s+")))
      if (sw > MAX_SHORT)
        problems <- c(problems, sprintf(
          "%s: `%s` runs to %d words, over %d. It is the rules, not the chapter",
          f, SHORT, sw, MAX_SHORT))

      # The restatement test, as far as a script can take it: a summary sentence
      # may not contain one of this chapter's own headings word for word. A
      # reader gets the headings from the sidebar for free, so a sentence that
      # repeats one is spending the reader's attention on nothing. Headings of
      # three words or more only: `Color` is a heading and also an ordinary word.
      norm <- function(x) gsub("[^a-z ]", "", tolower(gsub("\\s+", " ", x)))
      heads <- norm(sub("^##\\s*", "", trimws(ln[h2_at])))
      heads <- heads[vapply(strsplit(heads, " "), length, integer(1)) >= 3]
      sent <- unlist(strsplit(norm(paste(sbody, collapse = " ")), "(?<=[.]) ",
                              perl = TRUE))
      for (h in heads)
        for (x in sent)
          if (nzchar(h) && grepl(h, x, fixed = TRUE))
            problems <- c(problems, sprintf(
              "%s: `%s` repeats the heading \"%s\"; the sidebar already gives the reader that",
              f, SHORT, h))
    }

    body <- ln[(tail(h2_at, 1) + 1):length(ln)]

    if (any(grepl("^```", body)))
      problems <- c(problems, sprintf(
        "%s: `%s` holds a code chunk. The closer is prose; a plot belongs in a section of its own",
        f, HEAD))

    words <- length(unlist(strsplit(trimws(paste(body, collapse = " ")), "\\s+")))
    if (words > MAX_WORDS)
      problems <- c(problems, sprintf(
        "%s: `%s` runs to %d words, over %d. Two sentences: the edge, then the handoff",
        f, HEAD, words, MAX_WORDS))

    # The link. A chapter under `marks/` links its siblings bare and the root
    # with `../`, so compare basenames and accept either spelling; what is being
    # checked is which chapter is named, not how the path is written.
    want <- nxt[[f]]
    links <- regmatches(paste(body, collapse = " "),
                        gregexpr("\\]\\([^)]*\\.qmd[^)]*\\)", paste(body, collapse = " ")))[[1]]
    targets <- basename(sub("#.*$", "", gsub("^\\]\\(|\\)$", "", links)))
    if (!basename(want) %in% targets)
      problems <- c(problems, sprintf(
        "%s: `%s` does not link to %s, the chapter that follows it. Links found: %s",
        f, HEAD, want,
        if (length(targets)) paste(targets, collapse = ", ") else "none"))
  }

  if (length(problems))
    fail("FAIL: chapters whose closing section is missing or stale:\n  ",
         paste(problems, collapse = "\n  "),
         "\n  Each chapter ends with `", HEAD, "`: what its subject cannot do, ",
         "then the chapter that answers it.")

  n_short <- sum(vapply(scope, function(f)
    any(grepl("^## In short", readLines(file.path(book, f), warn = FALSE))),
    logical(1)))
  cat("PASS: every teaching chapter names its edge and what follows (",
      length(scope), "chapters,", n_short, "of them summarizing their rules first )\n")
  invisible(TRUE)
}
