# book/check_titles.R
# A plot's title is drawn as SVG text; it is never markdown.
#
# Quarto emits each plot's SVG *inline into the markdown stream*, so pandoc reads
# the SVG source as prose on its way past. A pair of backticks in a title is
# therefore a code span as far as pandoc is concerned: it closes the `<text>`
# element, closes `</svg>`, emits `<code>…</code>` as loose HTML, and leaves the
# rest of the plot — every cell, every axis — floating outside any SVG. The plot
# is silently truncated at the first backtick.
#
# Nothing in the toolchain could see it. `quarto render` exits 0, emits no WARN,
# the title text is present in the HTML, and a grep for the plot's own markup
# finds it. Four plots had been shipping broken this way, three of them for
# several sessions. This is the same blind spot `check_vocabulary.R` and
# `check_refusals.R` exist for, one level down: not prose naming an atom that
# does not exist, nor a refusal chunk that stopped refusing, but a *plot* that
# stopped drawing while every signal said it had.
#
# So the rule is a source-level one, checked here rather than in the render: no
# markdown syntax inside a title or an axis label. Say `tiling = hex`, not
# `` `tiling = "hex"` ``.

check_titles <- function(book_dir = "book") {
  qmds <- list.files(book_dir, pattern = "[.]qmd$", recursive = TRUE, full.names = TRUE)
  qmds <- qmds[!grepl("/_book/", qmds, fixed = TRUE)]

  # The label-family atoms, i.e. everything whose argument is drawn as SVG text.
  atoms <- c("title", "x_label", "y_label", "z_label")
  pat <- sprintf("\\b(%s)\\(\\s*\"((\\\\.|[^\"\\\\])*)\"", paste(atoms, collapse = "|"))

  bad <- character(0)
  for (f in qmds) {
    lines <- readLines(f, warn = FALSE)
    for (i in seq_along(lines)) {
      # Every label call on the line, not just the first: one line often carries
      # `x_label(...) + y_label(...)`.
      hits <- regmatches(lines[i], gregexpr(pat, lines[i], perl = TRUE))[[1]]
      for (h in hits) {
        arg <- sub(pat, "\\2", h, perl = TRUE)
        # A backtick is the demonstrated breakage. Underscores and asterisks are
        # left alone: they need a *pair* to mean anything, and a lone `*` in
        # "ribbon * bounds" is ordinary text pandoc passes through untouched.
        if (grepl("`", arg, fixed = TRUE)) {
          bad <- c(bad, sprintf("  %s:%d  %s",
                                sub("^.*book/", "", f), i, trimws(lines[i])))
        }
      }
    }
  }

  if (length(bad)) {
    cat("FAIL: a plot title contains markdown, which truncates its SVG\n")
    cat(paste(unlist(bad), collapse = "\n"), "\n")
    cat("  A title is drawn as text, not parsed. Drop the backticks.\n")
    stop("check_titles: ", length(bad), " title(s) would render a broken plot")
  }
  cat("PASS: no plot title contains markdown (", length(qmds), "chapters )\n")
  invisible(TRUE)
}
