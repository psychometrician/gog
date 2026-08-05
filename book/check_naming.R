# book/check_naming.R
# One thing, one spelling: `gog`, lowercase, everywhere.
#
# The name is the name, in one case, the way `ggplot2`, `pandas`, `npm` and
# `matplotlib` are. Nobody writes GGPLOT2, and the hex sticker has said `gog`
# from the start, so prose that shouted GOG was arguing with the logo.
#
# This reverses the earlier rule, which wrote **GOG** for the grammar and
# `` `gog` `` for the package. The argument for it was Vega-Lite, which
# capitalizes because it is a published specification that independent tools
# consume. gog is not that: all four packages are first-party and the JSON
# between them is internal, so the two-level naming protected a distinction only
# the project could see, while every reader had to carry two spellings of one
# name. Lowercase also puts *gog* inside *agog* on the page, which is the pun the
# capitals were supposed to be protecting.
#
# Sentence-initial lowercase is allowed, as it is for ggplot2 and npm. One rule
# with no exceptions is worth an occasional odd-looking first word.
#
# Two directions, because the mistake runs both ways:
#
#   1. `GOG` anywhere in prose         -> should be gog
#   2. `GOG` used as a code identifier -> should be gog (`library(GOG)` does
#      not exist; the installed package is lowercase)
#
# `GOG_STRICT`, `GOG_CLI_PATH` and the other environment variables keep their
# capitals: they are identifiers rather than the name, and the word-boundary in
# the pattern below leaves them alone. Paths and binaries are neither the name
# nor an identifier: `gog-cli`, `gog-core`, `book/gog.css` and `r-pkg/gog` are
# filenames, and the adjacency rules leave those alone too.

check_naming <- function(book_dir = "book") {
  qmds <- list.files(book_dir, pattern = "[.]qmd$", recursive = TRUE, full.names = TRUE)
  qmds <- qmds[!grepl("/_book/", qmds, fixed = TRUE)]

  # `GOG` in prose. The `\\b` on the right is what protects `GOG_STRICT` and the
  # other environment variables: `_` is a word character, so the boundary fails
  # inside them. The left lookbehind keeps a hypothetical `X-GOG` header out.
  prose_pat <- "(?<![\\w/.\\-])GOG\\b(?![_/.\\-])"

  # `GOG` where a package name belongs. Checked **only inside code** — a chunk
  # body, or the inside of an inline span — never against prose. The first
  # version of this pattern ran on prose too and immediately fired on
  # `coverage.qmd`'s "Two families are absent **from GOG**", because `from` is an
  # ordinary English word before it is a Python keyword. That is the whole reason
  # the two passes below are kept apart rather than merged into one grep.
  code_pat <- paste0(
    "(\\b(library|require|using)\\s*\\(\\s*[\"']?GOG\\b)",
    "|(\\bfrom\\s+GOG\\s+import\\b)",
    "|(\\bimport\\s+GOG\\b)",
    "|(\\binstall[._]packages\\s*\\(\\s*[\"']GOG)"
  )

  bad_prose <- character(0)
  bad_code  <- character(0)

  for (f in qmds) {
    lines <- readLines(f, warn = FALSE)
    short <- sub("^.*book/", "", f)
    in_chunk <- FALSE

    for (i in seq_along(lines)) {
      line <- lines[i]

      # A fence toggles the chunk state and is never itself prose.
      if (grepl("^\\s*```", line)) {
        in_chunk <- !in_chunk
        next
      }

      if (in_chunk) {
        if (grepl(code_pat, line, perl = TRUE)) {
          bad_code <- c(bad_code, sprintf("  %s:%d  %s", short, i, trimws(line)))
        }
        next
      }

      # Inline code carries the literal package name and the "gog" palette
      # value, both correctly lowercase. Its *contents* are code, so they get
      # the code check; the rest of the line is prose and gets the other one.
      spans <- regmatches(line, gregexpr("`[^`]*`", line))[[1]]
      if (length(spans) && any(grepl(code_pat, spans, perl = TRUE))) {
        bad_code <- c(bad_code, sprintf("  %s:%d  %s", short, i, trimws(line)))
      }
      stripped <- gsub("`[^`]*`", "", line)

      if (grepl(prose_pat, stripped, perl = TRUE)) {
        bad_prose <- c(bad_prose, sprintf("  %s:%d  %s", short, i, trimws(line)))
      }
    }
  }

  if (length(bad_prose) || length(bad_code)) {
    if (length(bad_prose)) {
      cat("FAIL: 'GOG' in prose; the name is lowercase everywhere\n")
      cat(paste(bad_prose, collapse = "\n"), "\n")
      cat("  Write gog, the way ggplot2 and pandas are written.\n")
    }
    if (length(bad_code)) {
      cat("FAIL: 'GOG' used as a package name; the package is lowercase\n")
      cat(paste(bad_code, collapse = "\n"), "\n")
      cat("  Write library(gog), not library(GOG).\n")
    }
    stop("check_naming: ", length(bad_prose) + length(bad_code), " naming inconsistency(ies)")
  }

  cat("PASS: gog spelled consistently (", length(qmds), "chapters )\n")
  invisible(TRUE)
}
