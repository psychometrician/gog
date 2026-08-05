# check_promises.R — does the book keep the five rules its preface states?
#
# `index.qmd` says, in the book's own voice, "Five rules govern every page, and
# you can hold the book to them." On 2026-07-28 a reader held it to them and
# three of the five were false:
#
#   * "Questions first. Chapters and recipes open with a question about data,
#     never with a feature name." — 1 of 43 plot-drawing chapters opened with a
#     question. Thirteen opened with a bare atom name as the literal first word
#     ("`point` draws one glyph per row of data").
#   * "A small cast … eight tables in all" — `book/R/data.R` defined 36 tables
#     and 33 were in use.
#   * "Read it aloud. Through Part III, every specification is accompanied by
#     its English sentence." — 473 specifications, 19 sentences. Two chapters
#     kept it perfectly (`reading.qmd`, `first-plot.qmd`) and 26 had none.
#
# The two that held were "every plot is live" and "errors on stage", and the
# second one is the lesson: it is the only rule with a test behind it
# (check_refusals.R), and it held at 138 for 138. A promise a script cannot
# check is a promise that drifts, so the wording was changed to be checkable and
# this file checks it.
#
# The read-aloud rule was narrowed deliberately, not to make the check pass. A
# gloss teaches a *skill*, and a skill is taught once; fifty-one of them in
# `transforms.qmd` would train a reader to skip them. So the rule is now the
# first specification in each chapter, and repetition after that is a choice.
#
# Scope. The opening rules bind the **teaching chapters** — Parts I, II and III
# plus the cookbook, which is exactly the span the preface's wording covers
# ("chapters and recipes"). The list is read out of `_quarto.yml` between the
# Part I divider and the Part V divider, so adding a chapter puts it under the
# rules automatically. Part V's reasons chapters, the four binding chapters and
# the generated appendices are outside it: they are reference, not teaching, and
# a question opening would be a costume rather than a structure.
#
# check_template.R owns the *end* of a mark chapter (last two sections, in
# order). This file owns the *opening* of every teaching chapter. They do not
# overlap.
#
# Run from the repo root; sourced by r-pkg/gog/tests/test_basic.R.

check_promises <- function(book = "book") {
  fail <- function(...) stop(..., call. = FALSE)
  problems <- character()

  yml <- readLines(file.path(book, "_quarto.yml"), warn = FALSE)
  # List entries only. `_quarto.yml` names chapters in comments too ("map
  # — coming, alongside space.qmd and polar.qmd"), and matching anywhere on the
  # line put `space.qmd` and `composition.qmd` in the scan twice.
  # `- part: parts/morning.qmd  # the whole grammar, one sitting` — the part
  # lines carry trailing comments, so the pattern has to allow one and then be
  # stripped back off.
  entries <- grep("^\\s*-\\s*(part:\\s*)?[A-Za-z0-9_/-]+\\.qmd\\s*(#.*)?$", yml, value = TRUE)
  listed <- sub("\\s*#.*$", "", sub("^\\s*-\\s*(part:\\s*)?", "", trimws(entries)))

  # Teaching span: everything between the Part I and Part V dividers, minus the
  # dividers themselves. Read from the file so a new chapter is covered on the
  # day it is added, which is the failure mode this whole file exists for.
  start <- match("parts/morning.qmd", listed)
  stop_ <- match("parts/reasons.qmd", listed)
  if (is.na(start) || is.na(stop_))
    fail("FAIL: check_promises cannot find the Part I / Part V dividers in _quarto.yml")
  teaching <- listed[(start + 1):(stop_ - 1)]
  teaching <- teaching[!grepl("^parts/", teaching)]

  read_chapter <- function(f) readLines(file.path(book, f), warn = FALSE)

  # --- Rule 1: every plot is live -----------------------------------------
  #
  # Two images are allowed in the whole book, and they are named here rather
  # than pattern-matched, so that adding a third has to be a decision. A plot
  # arriving as a screenshot is the failure this guards, and neither of these is
  # a plot: the *Hunminjeongeum* is a photograph of a 1446 woodblock, and the
  # mouth saying ㄱ is an anatomical diagram of the tongue. No engine draws
  # either one. The preface's own wording is kept in step with this list.
  #
  # The *Hunminjeongeum* is named by its `_paper` copy, which is still two
  # images and not three: the facsimile carries no background and its strokes
  # take SVG's default black, so on a dark page it was invisible, and the copy
  # is the same picture with a white ground and a margin added. Renaming it is
  # what this rule caught, which is the rule working: an exact filename is how
  # a third image is made to be a decision rather than a slip.
  allowed_images <- c("images/Hunmin_Jeongeum_paper.svg",
                      "images/Pronounciation.png")
  for (f in listed) {
    ln <- read_chapter(f)
    img <- grep("!\\[", ln)
    for (i in img) {
      if (!any(vapply(allowed_images,
                      function(a) grepl(a, ln[i], fixed = TRUE), logical(1))))
        problems <- c(problems, sprintf(
          "%s:%d static image, but every plot must be live: %s",
          f, i, substr(trimws(ln[i]), 1, 60)))
    }
  }

  # --- Rule 2: questions first --------------------------------------------
  #
  # The first prose paragraph must ask something. A chapter may still define its
  # atom in the very next sentence — the rule fixes the *order*, question then
  # definition, not the presence of the definition. `marks/bar.qmd` is the
  # shape: "How large is each category? `bar` draws a rectangle…".
  first_prose <- function(ln) {
    ln <- ln[!grepl("^\\s*(#|:::|\\||\\[\\^)", ln)]          # headings, divs, tables, footnotes
    inchunk <- FALSE
    keep <- character()
    for (l in ln) {
      if (grepl("^```", l)) { inchunk <- !inchunk; next }
      if (!inchunk) keep <- c(keep, l)
    }
    para <- character()
    for (l in keep) {
      if (!nzchar(trimws(l))) { if (length(para)) break else next }
      para <- c(para, trimws(l))
    }
    paste(para, collapse = " ")
  }
  # The question must be in the *opening*, not merely somewhere in the first
  # paragraph. `marks/box.qmd` passed a bare `grepl("?")` while opening with
  # "`box` draws the box-and-whisker of a distribution" and asking nothing until
  # several sentences later. 120 characters is about two short sentences, which
  # leaves room for the cookbook's shape ("You have one column. What can it tell
  # you?") without letting a question four sentences down count as an opening.
  for (f in teaching) {
    p <- first_prose(read_chapter(f))
    if (!grepl("\\?", substr(p, 1, 120)))
      problems <- c(problems, sprintf(
        "%s does not open with a question: %s", f, substr(p, 1, 64)))
  }

  # --- Rule 4: read it aloud ----------------------------------------------
  #
  # The first live specification in a chapter must be followed by its English
  # sentence, written as an italic quoted gloss — *"Given medals: bars, x is
  # country, y is gold."* Ten lines is the window: long enough for a chunk's
  # closing fence and a blank line, short enough that a gloss belonging to some
  # later plot cannot satisfy it.
  for (f in teaching) {
    ln <- read_chapter(f)
    fences <- grep("^```", ln)
    firstplot <- NA_integer_
    if (length(fences) >= 2) {
      for (k in seq(1, length(fences) - 1, by = 2)) {
        body <- ln[fences[k]:fences[k + 1]]
        if (grepl("^```\\{r\\}", ln[fences[k]]) &&
            any(grepl("data\\(", body)) &&
            !any(grepl("error: true|include: false", body))) {
          firstplot <- fences[k + 1]
          break
        }
      }
    }
    if (is.na(firstplot)) next                    # a chapter may draw nothing
    window <- ln[firstplot:min(length(ln), firstplot + 10)]
    if (!any(grepl('\\*"[^"]*"\\*', window)))
      problems <- c(problems, sprintf(
        "%s:%d first specification has no read-aloud sentence within 10 lines",
        f, firstplot))
  }

  # --- Rule 3: a small cast -----------------------------------------------
  #
  # The promise used to carry a count of tables, which is precisely the kind of
  # number that rots: it said eight while 33 were in use. What the rule actually
  # cares about is that a reader meets few enough tables to stop noticing them,
  # so the claim is now a *share* and is re-derived here rather than asserted in
  # prose. Locally built frames are deliberately included in the denominator —
  # they are the thing the share is a share of.
  families <- c(
    "gm_all", "gapminder_2007", "gm_eras", "gm_continents", "gm_europe",
    "gapminder_asia", "gdp_rug", "life_rug",                     # gapminder
    "iris_flowers", "score_band",                                # iris
    "medals",                                                    # medals
    "actuals", "forecast",                                       # forecast
    "winds", "day_cycle")                                        # winds
  used <- character()
  for (f in listed) {
    m <- regmatches(read_chapter(f),
                    gregexpr("data\\(\\s*[A-Za-z_][A-Za-z0-9_.]*", read_chapter(f)))
    used <- c(used, sub("data\\(\\s*", "", unlist(m)))
  }
  share <- mean(used %in% families)
  if (share < 0.70)
    problems <- c(problems, sprintf(
      "the five table families carry %.0f%% of plots; the preface claims three in four",
      share * 100))

  # Rule 5, errors on stage, is check_refusals.R's job and is not repeated here.

  # Printed rather than pasted into the condition message: R truncates a long
  # `stop()` string, and a list of thirty chapters is exactly the case where the
  # part that gets cut is the part you needed.
  if (length(problems)) {
    for (p in problems) message("  ", p)
    fail(sprintf("FAIL: the book breaks %d rules its preface states (listed above)",
                 length(problems)))
  }
  message(sprintf(
    "check_promises: OK (%d teaching chapters open with a question and gloss their first specification; five families carry %.0f%% of plots)",
    length(teaching), share * 100))
  invisible(TRUE)
}
