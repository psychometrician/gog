# check_vocabulary.R — does the manual name only things that exist?
#
# The live-chunk rule catches broken *code*: a ```{r} chunk runs, so it cannot
# document a call the engine refuses. Prose has no such guard. `marks/bar.qmd`
# told readers to "use the `total` transform" for several sessions — there has
# never been a `total` atom — and every book build exited 0.
#
# Two checks, both against the kernel block in grammar.qmd, which is where the
# book declares the vocabulary:
#
#   1. Anything prose calls a mark / channel / transform / atom / setting must
#      appear in the kernel block. This is what `total` failed.
#   2. Every atom exported from NAMESPACE must appear in the kernel block. This
#      is the reverse drift, and it has happened too: `group` shipped and sat
#      undocumented in the kernel tables for a session.
#   3. Every word in the kernel block must ship, or be marked ⬜ (designed, not
#      drawn). Checks 1 and 2 both treat the block as the authority, so neither
#      could ever doubt it: `click`, `brush`, `lasso`, `globe` and `map` sat in
#      it with no code behind them in any language, under a paragraph promising
#      GOG would tell you which words are undrawn.
#
# Run from the repo root; sourced by r-pkg/gog/tests/test_basic.R.

check_vocabulary <- function(book = "book", namespace = "r-pkg/gog/NAMESPACE") {
  fail <- function(...) stop(..., call. = FALSE)

  # -- the declared vocabulary ---------------------------------------------
  grammar <- readLines(file.path(book, "grammar.qmd"), warn = FALSE)
  start <- grep("^## The kernel", grammar)
  if (!length(start)) fail("FAIL: grammar.qmd has no '## The kernel' section")
  # The block was a monospace listing until 2026-07-28 and is now a table, one
  # row per kind: `| **Marks** | `point` `line` … |`. Reading the atoms out of
  # backticks rather than by splitting on whitespace is the more robust half of
  # the change — an atom is now delimited rather than inferred, so a stray note
  # in a cell cannot be mistaken for a name.
  block <- grammar[start:length(grammar)]
  block <- block[seq_len(which(!grepl("^\\|", block) & nzchar(trimws(block)) &
                               seq_along(block) > 2)[1])]
  block <- block[grepl("^\\|", block)]

  # A word may carry the ⬜ marker — `globe`⬜ — meaning designed but not drawn.
  # It sits *outside* the backticks on purpose, so the name reads the same to
  # the parser above whether or not it is marked, and check 3 below reads the
  # marker separately. The tables below in grammar.qmd use ✅ for "draws today",
  # so the pair reads together.
  undrawn <- character()
  kernel <- list()
  for (ln in block) {
    lbl <- regmatches(ln, regexpr("\\*\\*[A-Za-z]+\\*\\*", ln))
    if (!length(lbl)) next
    kind <- sub("s$", "", tolower(gsub("\\*", "", lbl)))   # **Marks** -> mark
    words <- gsub("`", "", regmatches(ln, gregexpr("`[^`]+`", ln))[[1]])
    kernel[[kind]] <- c(kernel[[kind]], words)
    marked <- regmatches(ln, gregexpr("`[^`]+`⬜", ln))[[1]]
    undrawn <- c(undrawn, gsub("`", "", gsub("⬜", "", marked, fixed = TRUE)))
  }
  kernel <- lapply(kernel, function(v) unique(v[grepl("^[a-z][a-z_0-9]*$", v)]))
  if (!length(kernel$mark) || !length(kernel$transform))
    fail("FAIL: could not parse the kernel block in grammar.qmd")

  all_names <- unique(c(unlist(kernel, use.names = FALSE), "data"))

  # -- 1. prose must not name atoms that do not exist -----------------------
  # Chapters live at the root and in subdirectories (marks/, parts/,
  # cookbook/); list each on purpose — a recursive listing would wander into
  # _book/ and .quarto/.
  qmd <- c(list.files(book, pattern = "\\.qmd$", full.names = TRUE),
           unlist(lapply(file.path(book, c("marks", "parts", "cookbook", "bindings")),
                         list.files, pattern = "\\.qmd$", full.names = TRUE)))

  # That directory list is hand-maintained in two files, and a hand-maintained
  # list beside the real thing always loses: a chapter in a directory nobody
  # added is never checked, and nothing says so. `bindings/` was exactly that on
  # the day it was created — a new chapter, invisible to both checks, and only
  # the pipe section's `error: true` chunk would have noticed, by not being run.
  # So the list checks itself. A recursive listing is not used for the scan
  # (it would wander into _book/ and .quarto/), only for this comparison.
  present <- list.files(book, pattern = "\\.qmd$", full.names = TRUE, recursive = TRUE)
  present <- present[!grepl("(^|/)(_book|\\.quarto|site_libs)/", present)]
  missed <- setdiff(normalizePath(present), normalizePath(qmd))
  if (length(missed))
    fail("FAIL: chapters in book/ that no check can see — add the directory to ",
         "`qmd` in BOTH check_vocabulary.R and check_refusals.R: ",
         paste(basename(missed), collapse = ", "))

  # -- 1a. every heading must actually be a heading -------------------------
  #
  # A `## ` line with no blank line before it is *not* a heading: pandoc reads it
  # as lazy continuation of the paragraph above and renders the hashes as literal
  # text mid-sentence. `marks/rule.qmd` shipped that way for one render —
  # `rule.html` read "…the same geometry. ## What it refuses" inside a `<p>` — and
  # every source-level scan here was happy, because they all match on the line
  # prefix alone. The book builds clean either way, so nothing but a human reading
  # the HTML could tell. Checked book-wide, since any chapter can catch it.
  # Two regions are not prose and are skipped: a code fence, and **YAML front
  # matter**. A `#` inside front matter is a YAML *comment*, which pandoc never
  # renders at all, so flagging one is a false positive — and not a harmless
  # one, because it fails this whole check and with it the test suite. Adding
  # `date-modified: today` to `index.qmd` with a comment explaining it did
  # exactly that on 2026-07-28. Front matter is only front matter when the file
  # opens with it, which is why the `---` toggle is armed on line 1 alone; a
  # `---` later in a chapter is a horizontal rule and must not start a region.
  invisible <- character()
  for (f in qmd) {
    ln <- readLines(f, warn = FALSE)
    fence <- FALSE
    yaml <- length(ln) > 0L && grepl("^---\\s*$", ln[1])
    for (i in seq_along(ln)) {
      if (yaml) {
        if (i > 1L && grepl("^---\\s*$", ln[i])) yaml <- FALSE
        next
      }
      if (grepl("^```", ln[i])) { fence <- !fence; next }
      if (fence || i == 1L) next
      if (grepl("^#{1,6} ", ln[i]) && nzchar(trimws(ln[i - 1L])))
        invisible <- c(invisible, sprintf("%s:%d: %s", basename(f), i,
                                          trimws(ln[i])))
    }
  }
  if (length(invisible))
    fail("FAIL: `#` lines that pandoc will render as text, not headings — they ",
         "need a blank line before them:\n  ",
         paste(invisible, collapse = "\n  "))

  # A sentence may legitimately name something that does not exist, as long as
  # it says so: "there is no `flip` atom", "a `repel` transform is on the
  # roadmap". Naming one *without* that signal is the defect.
  disclaims <- "\\bno\\b|\\bnot\\b|roadmap|coming|planned|would be|instead of|does not|cannot|never"

  claim <- "`([a-z_][a-z_0-9]*)`[ ]+(mark|channel|transform|atom|setting|scale|space|selection)s?\\b"
  bad <- character()
  for (f in qmd) {
    ln <- readLines(f, warn = FALSE)
    inside <- cumsum(grepl("^```", ln)) %% 2 == 1
    for (i in which(!inside & !grepl("^```", ln) & grepl(claim, ln))) {
      m <- regmatches(ln[i], gregexpr(claim, ln[i]))[[1]]
      for (hit in m) {
        nm <- sub("^`([^`]+)`.*", "\\1", hit)
        kind <- sub(".*`[ ]+", "", hit); kind <- sub("s$", "", kind)
        known <- if (kind == "atom") all_names else c(kernel[[kind]], recursive = TRUE)
        if (nm %in% known) next
        if (grepl(disclaims, ln[i])) next   # explicitly says it does not exist
        bad <- c(bad, sprintf("%s: `%s` called a %s, but no such %s exists",
                              basename(f), nm, kind, kind))
      }
    }
  }
  if (length(bad))
    fail("FAIL: the manual names atoms that do not exist:\n  ",
         paste(bad, collapse = "\n  "))

  # -- 2. every shipped atom must be in the kernel block --------------------
  ns <- readLines(namespace, warn = FALSE)
  exported <- sub("\\).*", "", sub("^export\\(", "", grep("^export\\(", ns, value = TRUE)))
  # render_svg is the R binding's entry point, not a word of the grammar.
  # `colour` is exported only to be refused: it is the British spelling of
  # `color`, and a reader arriving from ggplot2 types it, so the refusal names
  # the fix instead of leaving R to say "could not find function". A word that
  # exists to teach its own absence must not be documented as a channel — this
  # is the one export that is deliberately *not* in the kernel block.
  exported <- setdiff(exported, c("render_svg", "colour"))
  undocumented <- setdiff(exported, all_names)
  if (length(undocumented))
    fail("FAIL: exported but absent from the kernel block in grammar.qmd — ",
         "shipped and undocumented: ", paste(undocumented, collapse = ", "))

  # -- 3. every kernel word must be shipped, or marked as not drawn ---------
  #
  # The reverse of check 2, and the direction that was missing. Check 2 catches
  # an atom that ships without being documented; nothing caught a word that is
  # *documented without shipping*. `click`, `brush` and `lasso` sat in the
  # kernel block from the first draft with no code behind them in any of the
  # four languages, and `globe` and `map` joined them — five names a reader
  # could type and get "could not find function" for, while the paragraph under
  # the table promised GOG would say which words are undrawn. Nothing could
  # fail, because no check read the block in this direction.
  #
  # A word is allowed here only if it is exported, marked ⬜, or named below.
  # So the ⬜ list cannot go stale in either direction: adding a word without
  # code fails until it is marked, and the day `globe` ships, its marker has to
  # come off or check 2 is happy while the book still calls it undrawn.
  #
  # These are real, and are the reason the rule is not simply "exported":
  #   `flat`                            the default space — what you get by
  #                                     binding no other one, never called
  #   `linear` `log` `time` `category`  values of a position's `scale =`
  #                                     argument, x(gdp, scale = "log"),
  #                                     not functions
  not_called <- c("flat", "linear", "log", "time", "category")
  vapor <- setdiff(all_names, c(exported, undrawn, not_called, "data", "colour"))
  if (length(vapor))
    fail("FAIL: in the kernel block in grammar.qmd, but nothing ships it — ",
         "mark it with ⬜ or delete it: ", paste(vapor, collapse = ", "))

  stale <- intersect(undrawn, exported)
  if (length(stale))
    fail("FAIL: marked ⬜ (not drawn) in grammar.qmd but exported — the ",
         "feature landed and the marker did not come off: ",
         paste(stale, collapse = ", "))

  # -- 4. every mark chapter must *generate* its settings table -------------
  #
  # Each mark's "What you can set" table comes from `mark_options()`, which
  # reads the engine's own rule table, so a new setting appears everywhere at
  # once and no page can claim something `style()` refuses. A chapter that
  # hand-types the table instead looks identical the day it is written and rots
  # from then on: `zone` shipped with a typed one and was still carrying it two
  # features later, by which time its opacity row was wrong. This is the same
  # failure the engine hit twice in one session — a hand-maintained list beside
  # a generated one always loses — caught here for the book.
  chapters <- list.files(file.path(book, "marks"), pattern = "\\.qmd$", full.names = TRUE)
  chapters <- chapters[basename(chapters) != "index.qmd"]
  typed <- character()
  for (f in chapters) {
    src <- readLines(f, warn = FALSE)
    if (!any(grepl("mark_options\\(", src))) typed <- c(typed, basename(f))
  }
  if (length(typed))
    fail("FAIL: these mark chapters do not generate their settings table from ",
         "the engine (add a `mark_options(\"<mark>\")` chunk): ",
         paste(typed, collapse = ", "))

  cat("PASS: the manual names only atoms that exist (",
      length(all_names), "declared,", length(exported), "shipped,",
      length(undrawn), "marked not drawn )\n")
  cat("PASS: every mark chapter generates its settings table (",
      length(chapters), "chapters )\n")
  invisible(TRUE)
}
