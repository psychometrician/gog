# book/check_naming.R
# One thing, one spelling: `GOG` in prose, `` `gog` `` in code font.
#
# The grammar, the project and the system are **GOG**, an acronym of *Grammar Of
# Graphics*. That is not decoration: the preface spends three paragraphs on the
# fact that *A Grammar of Graphics* abbreviates to *agog*, and a lowercase "gog"
# in prose quietly costs the book the joke it was building. The **package** is
# `` `gog` `` in backticks, because that is the literal name a reader types
# (`library(gog)`), and so is the `"gog"` palette value.
#
# This is the American-English rule one level over: two spellings of one word
# that a reader has to learn to recognize where one would do, which is the
# silent letter Law 2 exists to refuse.
#
# It is checked rather than merely written down because it had already drifted
# once, and drifted in the way conventions actually drift: not by anyone
# deciding, but *by chapter*. On 2026-07-28 the book stood at 109 uppercase
# against 51 lowercase, with `design-laws.qmd` at 16-0 and `marks/zone.qmd` at
# 1-7 — one writing session's habit each, no distinction intended anywhere. A
# sweep fixed the 51. Nothing in the toolchain could have told you it happened,
# and nothing would tell you when it happened again, which is the whole reason
# the four checks beside this one exist.
#
# Two directions, because the mistake runs both ways:
#
#   1. a bare lowercase `gog` in prose  -> should be GOG
#   2. `GOG` used as a code identifier  -> should be gog (`library(GOG)` does
#      not exist; the installed package is lowercase)
#
# Paths and binaries are neither: `gog-cli`, `gog-core`, `book/gog.css` and
# `r-pkg/gog` are filenames, and the adjacency rules below leave them alone.

check_naming <- function(book_dir = "book") {
  qmds <- list.files(book_dir, pattern = "[.]qmd$", recursive = TRUE, full.names = TRUE)
  qmds <- qmds[!grepl("/_book/", qmds, fixed = TRUE)]

  # Bare lowercase `gog` in prose. The lookaround excludes a backtick on either
  # side (inline code, already stripped, but belt and braces), a word character
  # (gogh, agog), and the three path characters `/`, `.` and `-`, which is what
  # keeps `gog-cli`, `gog.css` and `r-pkg/gog` out of it.
  prose_pat <- "(?<![`\\w/.\\-])gog(?![\\w`/.\\-])"

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
      cat("FAIL: bare lowercase 'gog' in prose; the system is GOG\n")
      cat(paste(bad_prose, collapse = "\n"), "\n")
      cat("  Write GOG for the grammar, `gog` in backticks for the package.\n")
    }
    if (length(bad_code)) {
      cat("FAIL: 'GOG' used as a package name; the package is lowercase\n")
      cat(paste(bad_code, collapse = "\n"), "\n")
      cat("  Write library(gog), not library(GOG).\n")
    }
    stop("check_naming: ", length(bad_prose) + length(bad_code), " naming inconsistency(ies)")
  }

  cat("PASS: GOG/gog spelled consistently (", length(qmds), "chapters )\n")
  invisible(TRUE)
}
