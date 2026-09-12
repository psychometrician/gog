# book/check_sections.R
# Every section with a plot says something about it.
#
# A heading followed by nothing but a plot and its read-aloud sentence is a
# caption, not a section. The read-aloud sentence tells the reader what the
# specification *says*; nothing tells them what the plot *shows*, and a heading
# is a label for finding a place, not an argument. Two Basic usage sections
# shipped exactly this way, and one of them drew a single stroke through five
# countries at every year, a sawtooth, beside no sentence saying why. The engine
# warns on that sentence, and the book does not print the warning, so the reader
# had nothing at all. The sibling chapters explain their plots in a short
# paragraph after the read-aloud sentence; this holds every section to that.
#
# The threshold is one line of prose, deliberately. Whether a section has said
# *enough* is a judgment for the review pass; whether it has said *anything* is
# a fact a machine can own.
#
# A section runs from its heading to the next heading of any level. Prose is any
# line outside a code chunk, the YAML block or a div fence that is not blank, not
# a heading, and not part of a read-aloud sentence (a line opening with *" up to
# the line closing with "*).

check_sections <- function(book_dir = "book") {
  qmds <- list.files(book_dir, pattern = "[.]qmd$", recursive = TRUE, full.names = TRUE)
  qmds <- qmds[!grepl("/_book/", qmds, fixed = TRUE)]

  bad <- character(0)
  for (f in qmds) {
    lines <- readLines(f, warn = FALSE)
    in_chunk <- FALSE
    in_yaml  <- FALSE
    in_gloss <- FALSE
    heading  <- NULL     # c(line number, text) of the open section
    chunks   <- 0L
    prose    <- 0L

    close_section <- function() {
      if (!is.null(heading) && chunks > 0L && prose == 0L) {
        bad <<- c(bad, sprintf("  %s:%s  %s", sub("^.*book/", "", f), heading[1], heading[2]))
      }
    }

    for (i in seq_along(lines)) {
      ln <- lines[i]
      if (i == 1L && ln == "---") { in_yaml <- TRUE; next }
      if (in_yaml) { if (ln == "---") in_yaml <- FALSE; next }
      if (grepl("^```", ln)) {
        if (!in_chunk && !is.null(heading)) chunks <- chunks + 1L
        in_chunk <- !in_chunk
        next
      }
      if (in_chunk) next
      if (grepl("^#{1,6} ", ln)) {
        close_section()
        heading <- c(i, trimws(ln)); chunks <- 0L; prose <- 0L
        next
      }
      if (is.null(heading)) next
      # A read-aloud sentence opens with *" and closes with "*, possibly lines
      # later; prose may follow the closing "* on the same line and counts.
      if (in_gloss) {
        m <- regexpr("\"\\*", ln)
        if (m > 0L) {
          in_gloss <- FALSE
          if (nzchar(trimws(substr(ln, m + 2L, nchar(ln))))) prose <- prose + 1L
        }
        next
      }
      if (grepl("^\\*\"", ln)) {
        body <- substr(ln, 3L, nchar(ln))
        m <- regexpr("\"\\*", body)
        if (m > 0L) {
          if (nzchar(trimws(substr(body, m + 2L, nchar(body))))) prose <- prose + 1L
        } else {
          in_gloss <- TRUE
        }
        next
      }
      if (!nzchar(trimws(ln))) next
      if (grepl("^:::", ln)) next
      prose <- prose + 1L
    }
    close_section()
  }

  if (length(bad)) {
    cat("FAIL: a section holds a plot and says nothing about it\n")
    cat(paste(bad, collapse = "\n"), "\n")
    cat("  Say what the plot shows, in a sentence or two after its read-aloud sentence.\n")
    stop("check_sections: ", length(bad), " section(s) hold a chunk and no prose")
  }
  cat("PASS: every section with a plot says something about it (", length(qmds), "chapters )\n")
  invisible(TRUE)
}
