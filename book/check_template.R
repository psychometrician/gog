# check_template.R — do the mark chapters actually follow the template they claim?
#
# `parts/letters.qmd` states it as an invariant, in the book's own voice:
#
#   "The mark chapters follow one template: what it draws, what it requires, what
#    refines it, what derives from it, what it refuses. That sameness is the
#    point: it is the No Exceptions law applied to a book's structure, and it
#    means learning the second mark is faster than the first."
#
# On 2026-07-25 that was false, and had been for a while. Only four of twelve
# chapters had a refusals section; it was spelled three different ways ("What an
# area refuses", "What a ribbon refuses", "What it refuses"); `point` and `step`
# demonstrated no refusal at all; and the closing section was not in a fixed
# position, `rule` carrying its settings table fifth of eight. A reader who took
# the promise literally and went looking for the last section found a different
# thing in each chapter.
#
# That is the same species of defect as the `total`-transform prose bug and the
# `error: true` chunk that stopped refusing: a claim stated in prose that nothing
# in the toolchain could check. So it is checked here.
#
# Three assertions per mark chapter:
#   1. The chapter ends `## What you can set`, `## What it refuses`, then
#      `## What comes next`, with an optional `## In short` between the last two.
#      The closer joined the template on 2026-09-18 and is the last section in
#      every chapter of the book that has one, so a mark chapter ends the way the
#      rest of the book does. The summary is optional because it is only worth a
#      reader's attention where a chapter's rules are not already its headings,
#      which is 12 of the 14 marks; `check_closers.R` owns both of them.
#   2. One spelling, everywhere. Enforced by (1) being an exact string match.
#   3. The refusals section holds at least one `#| error: true` chunk — the
#      section must *show* a refusal, not merely be titled one. check_refusals.R
#      then proves each of those chunks really does refuse, so the two checks
#      together mean a mark chapter cannot claim a refusal it does not have.
#
# Run from the repo root; sourced by r-pkg/gog/tests/test_basic.R.

check_template <- function(book = "book") {
  fail <- function(...) stop(..., call. = FALSE)

  SET     <- "## What you can set"
  REFUSES <- "## What it refuses"
  NEXT    <- "## What comes next"
  SHORT   <- "## In short"

  chapters <- list.files(file.path(book, "marks"), pattern = "\\.qmd$",
                         full.names = TRUE)
  chapters <- chapters[basename(chapters) != "index.qmd"]
  if (!length(chapters)) fail("FAIL: no mark chapters found — the scan is broken")

  bad <- character()
  for (f in chapters) {
    ln <- readLines(f, warn = FALSE)

    # Headings only outside code fences: a chunk can legitimately contain a line
    # starting with `## ` (an R comment), and counting those would be nonsense.
    fence <- cumsum(grepl("^```", ln)) %% 2 == 1
    h2_at <- which(!fence & grepl("^## ", ln))

    # A `## ` line is not a heading unless a blank line precedes it: pandoc reads
    # it as lazy continuation of the paragraph above and renders the hashes as
    # literal text. This check itself passed a file with that defect once — the
    # source said `## What it refuses`, `rule.html` said
    # "…the same geometry. ## What it refuses" inside a <p>, and both this scan
    # and the two beside it were matching on the line prefix alone. So the shape
    # a *reader* sees is what gets asserted, not the shape the source suggests.
    invisible_h2 <- h2_at[h2_at > 1 & nzchar(trimws(ln[pmax(h2_at - 1, 1)]))]
    if (length(invisible_h2)) {
      bad <- c(bad, sprintf(paste("%s:%d: `%s` has no blank line before it, so",
                                  "pandoc renders the `##` as text, not a",
                                  "heading"),
                            basename(f), invisible_h2,
                            trimws(ln[invisible_h2])))
      next
    }
    h2 <- trimws(ln[h2_at])

    if (length(h2) < 3) {
      bad <- c(bad, sprintf("%s: only %d section(s)", basename(f), length(h2)))
      next
    }
    # Drop the optional summary before comparing: it sits between the refusals
    # and the closer, and whether a chapter has one is `check_closers.R`'s
    # question, not this file's.
    spine <- h2[h2 != SHORT]
    last_three <- tail(spine, 3)
    if (!identical(last_three, c(SET, REFUSES, NEXT))) {
      bad <- c(bad, sprintf("%s: ends on %s, expected `%s`, `%s`, then `%s`",
                            basename(f),
                            paste(sprintf("`%s`", last_three), collapse = " then "),
                            SET, REFUSES, NEXT))
      next
    }

    # The refusals section must show a refusal. It is the second to last section
    # by the check above, so it runs from its own heading to the closer's. Read
    # that span rather than everything after the last heading: the closer is
    # prose and holds no chunk, so scanning to the end of the file would search
    # the wrong section and pass for the wrong reason.
    refuses_at <- h2_at[which(trimws(ln[h2_at]) == REFUSES)]
    after <- h2_at[h2_at > refuses_at]
    body <- ln[refuses_at:(min(after) - 1)]
    if (!any(grepl("error:\\s*true", grep("^#\\|", body, value = TRUE))))
      bad <- c(bad, sprintf(paste("%s: `%s` contains no `error: true` chunk —",
                                  "the section must *show* a refusal, not just",
                                  "be titled one"),
                            basename(f), REFUSES))
  }

  if (length(bad))
    fail("FAIL: mark chapters that break the template `parts/letters.qmd` ",
         "promises:\n  ", paste(bad, collapse = "\n  "),
         "\n  Either fix the chapter, or stop claiming the template there.")

  cat("PASS: every mark chapter follows the template (", length(chapters),
      "chapters, ending `What you can set`, `What it refuses`,",
      "then `What comes next` )\n")
  invisible(TRUE)
}
