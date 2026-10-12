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
#
# Once per chapter leaves one gap, and the second check closes it. A table can
# leave a chapter for hundreds of lines and come back: `listening` is shown in
# the `stack` section of Transforms and drawn again in `cumulative`, 489 lines
# later, by which point its columns are a long scroll away. A second view would
# break the once-per-chapter rule, so the prose links back to the first one
# instead. When a plot draws from a table whose last plot in the chapter is
# `far` lines or more above it, the prose since the chunk before must link to a
# section that holds the table's view. Only plots count, as uses and as the
# plots that need the link: a refusal (`#| error: true`) draws no table.

check_peeks <- function(book_dir = "book", far = 300L) {
  shared <- readLines(file.path(book_dir, "R", "data.R"), warn = FALSE)
  shared <- sub("^([A-Za-z_][A-Za-z0-9_.]*)\\s*<-.*", "\\1",
                grep("^[A-Za-z_][A-Za-z0-9_.]*\\s*<-", shared, value = TRUE))
  shared <- unique(shared)

  qmds <- list.files(book_dir, pattern = "[.]qmd$", recursive = TRUE, full.names = TRUE)
  qmds <- qmds[!grepl("/_book/|/parts/", qmds)]

  # The identifier pandoc gives a heading, by the rule check_links.R states:
  # lowercase, keep letters, digits, `_`, `-` and `.`, join the words with `-`,
  # drop everything before the first letter, and number a repeated heading.
  # check_links.R fails on an anchor that reaches no heading, so if this copy
  # ever drifts from that one, a correct link fails here, loudly.
  heading_id <- function(text, seen) {
    given <- regmatches(text, regexec("\\{[^}]*#([^\\s}]+)[^}]*\\}\\s*$", text, perl = TRUE))[[1]]
    if (length(given)) return(list(id = given[2], seen = seen))
    s <- sub("\\s*\\{[^}]*\\}\\s*$", "", text)
    s <- gsub("\\[([^]]*)\\]\\([^)]*\\)", "\\1", s)
    s <- tolower(gsub("<[^>]+>|[`*]", "", s))
    s <- gsub("[^\\p{L}\\p{N}_.\\s-]", "", s, perl = TRUE)
    s <- paste(strsplit(trimws(s), "\\s+", perl = TRUE)[[1]], collapse = "-")
    s <- sub("^[^\\p{L}]+", "", s, perl = TRUE)
    if (!nzchar(s)) s <- "section"
    n <- if (is.null(seen[[s]])) 0L else seen[[s]]
    seen[[s]] <- n + 1L
    list(id = if (n == 0L) s else paste0(s, "-", n), seen = seen)
  }

  late <- character(0)
  unlinked <- character(0)
  n_views <- 0L
  n_far <- 0L

  for (f in qmds) {
    lines <- readLines(f, warn = FALSE)
    where <- sub(paste0("^", book_dir, "/"), "", f)
    # Fence tracking rather than a grep: a ```python block holds sentences too,
    # and only the R chunks draw.
    fence <- grepl("^\\s*```", lines)
    open <- FALSE
    is_r <- FALSE
    start <- 0L
    seen <- character(0)          # tables this chapter has shown
    view_at <- list()             # table -> the line of its view
    chunks <- list()              # every R chunk, for the second check
    heads <- data.frame(line = integer(0), level = integer(0), id = character(0),
                        text = character(0))
    ids_seen <- list()
    in_yaml <- length(lines) && grepl("^---\\s*$", lines[1])
    for (i in seq_along(lines)) {
      if (in_yaml) {
        if (i > 1L && grepl("^---\\s*$", lines[i])) in_yaml <- FALSE
        next
      }
      if (fence[i]) {
        if (!open) {
          open <- TRUE
          is_r <- grepl("^\\s*```\\{r[,}[:space:]]", lines[i])
          start <- i
        } else {
          if (is_r) {
            body <- lines[seq_len(i - start - 1L) + start]
            code <- body[!grepl("^\\s*(#\\||$)", body)]
            chunks[[length(chunks) + 1L]] <- list(
              start = start, end = i,
              error = any(grepl("^\\s*#\\|\\s*error:\\s*true", body)),
              view = length(code) > 0L && all(grepl("^\\s*peek\\(", code)),
              used = unique(intersect(sub(".*\\(\\s*", "", unlist(regmatches(body,
                gregexpr("\\bdata\\(\\s*[A-Za-z_][A-Za-z0-9_.]*", body)))), shared)))
          }
          open <- FALSE
          is_r <- FALSE
        }
        next
      }
      if (!open) {
        m <- regmatches(lines[i], regexec("^(#{1,6})\\s+(.*?)\\s*$", lines[i]))[[1]]
        if (length(m)) {
          h <- heading_id(m[3], ids_seen)
          ids_seen <- h$seen
          heads <- rbind(heads, data.frame(line = i, level = nchar(m[2]), id = h$id,
                                           text = m[3]))
        }
        next
      }
      if (!is_r) next
      shown <- regmatches(lines[i], gregexpr("\\bpeek\\(\\s*[A-Za-z_][A-Za-z0-9_.]*", lines[i]))[[1]]
      if (length(shown)) {
        shown <- sub(".*\\(\\s*", "", shown)
        for (tbl in setdiff(shown, names(view_at))) view_at[[tbl]] <- i
        seen <- union(seen, shown)
        n_views <- n_views + length(shown)
      }
      used <- regmatches(lines[i], gregexpr("\\bdata\\(\\s*[A-Za-z_][A-Za-z0-9_.]*", lines[i]))[[1]]
      if (!length(used)) next
      used <- sub(".*\\(\\s*", "", used)
      for (tbl in setdiff(intersect(used, shared), seen)) {
        late <- c(late, sprintf("%s:%d  %s", where, i, tbl))
        seen <- union(seen, tbl)  # one report per table per chapter
      }
    }

    # The sections that hold a line: each heading above it whose section, which
    # runs to the next heading of the same level or higher, has not yet closed.
    holding <- function(line) {
      above <- heads[heads$line < line, , drop = FALSE]
      keep <- vapply(seq_len(nrow(above)), function(k) {
        later <- heads[heads$line > above$line[k] & heads$level <= above$level[k], ]
        !nrow(later) || later$line[1] > line
      }, logical(1))
      above[keep, , drop = FALSE]
    }

    last <- list()                # table -> the line of its last plot
    after <- 0L                   # the end of the last chunk that is not a view
    for (ch in chunks) {
      if (!ch$error && !ch$view) {
        prose <- paste(lines[seq_len(max(0L, ch$start - after - 1L)) + after], collapse = "\n")
        anchors <- sub("^\\]\\(#", "", sub("\\)$", "",
          regmatches(prose, gregexpr("\\]\\(#[^)[:space:]]+\\)", prose))[[1]]))
        for (tbl in ch$used) {
          gap <- if (is.null(last[[tbl]])) 0L else ch$start - last[[tbl]]
          last[[tbl]] <- ch$start
          if (gap < far || is.null(view_at[[tbl]])) next
          n_far <- n_far + 1L
          held <- holding(view_at[[tbl]])
          if (any(anchors %in% held$id)) next
          inner <- held[nrow(held), ]
          unlinked <- c(unlinked, sprintf(
            "%s:%d  %s, %d lines after its last plot. Its view is under \"%s\" (#%s)",
            where, ch$start, tbl, gap, inner$text, inner$id))
        }
      }
      if (!ch$view) after <- ch$end
    }
  }

  if (length(late)) {
    cat("FAIL: a chapter draws from a table it has not shown\n")
    cat(paste(late, collapse = "\n"), "\n")
    cat("  Put the view before the chunk that first reads the table:\n")
    cat("  ```{r}\n  #| echo: false\n  peek(<table>)\n  ```\n")
    cat("  The scope is the chapter, not the book: a view in an earlier chapter\n")
    cat("  does not count, and the same table is shown again here.\n")
  }
  if (length(unlinked)) {
    cat("FAIL: a table comes back from far away with no link to its view\n")
    cat(paste(unlinked, collapse = "\n"), "\n")
    cat("  In the prose just above the plot, link to the section that shows it:\n")
    cat("  The `<table>` table, shown in [<section>](#<id>), ...\n")
    cat("  The view stays where it is. A chapter shows a table once.\n")
  }
  if (length(late) || length(unlinked))
    stop("check_peeks: ", length(late), " table(s) drawn before being shown, ",
         length(unlinked), " far return(s) with no link to the view")
  cat("PASS: every chapter shows its tables before drawing them (", n_views,
      "views ), and links back to the view at each of", n_far, "far returns\n")
  invisible(TRUE)
}
