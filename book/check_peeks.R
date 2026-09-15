# book/check_peeks.R
# A chapter shows every table it draws from, before it draws from it.
#
# A sentence names its columns and never its rows, so a reader who has not seen
# the table cannot tell whether `life` is a count or a proportion, or how many
# rows the picture holds. `peek()` answers that in five rows, and the rule for
# where one goes is **per chapter**.
#
# It was per book until 2026-09-14, and that is the change this guard records.
# Book scope reads well only for a reader going front to back, and a manual is
# not read that way: someone opening Polar meets `tide` for the first time,
# whatever an earlier chapter showed. So every chapter now stands alone, and the
# view repeats where the table does. Inside one chapter it still appears once,
# at the first sentence that reads the table.
#
# The scope is the shared tables, the ones defined in `book/R/data.R` and never
# shown in the page that uses them. A table a chapter builds in a visible chunk
# needs no view: the reader watched it being made.

check_peeks <- function(book_dir = "book") {
  shared <- readLines(file.path(book_dir, "R", "data.R"), warn = FALSE)
  shared <- sub("^([A-Za-z_][A-Za-z0-9_.]*)\\s*<-.*", "\\1",
                grep("^[A-Za-z_][A-Za-z0-9_.]*\\s*<-", shared, value = TRUE))
  shared <- unique(shared)

  qmds <- list.files(book_dir, pattern = "[.]qmd$", recursive = TRUE, full.names = TRUE)
  qmds <- qmds[!grepl("/_book/|/parts/", qmds)]

  late <- character(0)
  n_views <- 0L

  for (f in qmds) {
    lines <- readLines(f, warn = FALSE)
    # Fence tracking rather than a grep: a ```python block holds sentences too,
    # and only the R chunks draw.
    fence <- grepl("^\\s*```", lines)
    open <- FALSE
    is_r <- FALSE
    seen <- character(0)          # tables this chapter has shown
    for (i in seq_along(lines)) {
      if (fence[i]) {
        if (!open) {
          open <- TRUE
          is_r <- grepl("^\\s*```\\{r[,}[:space:]]", lines[i])
        } else {
          open <- FALSE
          is_r <- FALSE
        }
        next
      }
      if (!open || !is_r) next
      shown <- regmatches(lines[i], gregexpr("\\bpeek\\(\\s*[A-Za-z_][A-Za-z0-9_.]*", lines[i]))[[1]]
      if (length(shown)) {
        shown <- sub(".*\\(\\s*", "", shown)
        seen <- union(seen, shown)
        n_views <- n_views + length(shown)
      }
      used <- regmatches(lines[i], gregexpr("\\bdata\\(\\s*[A-Za-z_][A-Za-z0-9_.]*", lines[i]))[[1]]
      if (!length(used)) next
      used <- sub(".*\\(\\s*", "", used)
      for (tbl in setdiff(intersect(used, shared), seen)) {
        late <- c(late, sprintf("%s:%d  %s", sub("^book/", "", f), i, tbl))
        seen <- union(seen, tbl)  # one report per table per chapter
      }
    }
  }

  if (length(late)) {
    cat("FAIL: a chapter draws from a table it has not shown\n")
    cat(paste(late, collapse = "\n"), "\n")
    cat("  Put the view before the chunk that first reads the table:\n")
    cat("  ```{r}\n  #| echo: false\n  peek(<table>)\n  ```\n")
    cat("  The scope is the chapter, not the book: a view in an earlier chapter\n")
    cat("  does not count, and the same table is shown again here.\n")
    stop("check_peeks: ", length(late), " table(s) drawn before being shown")
  }
  cat("PASS: every chapter shows its tables before drawing them (", n_views, "views )\n")
  invisible(TRUE)
}
