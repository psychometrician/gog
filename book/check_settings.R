# book/check_settings.R
# The settings the book lists are the settings the engine has.
#
# Two places show `style()`'s settings, and both went wrong the day `angle` was
# added. The settings table in each mark's chapter is generated from
# `gog-cli --rules`, but a setting whose values the engine does not list (a
# color, a size, a number of degrees) takes its Value cell from a description
# written by hand in `book/R/setup.R`. `angle` had none, and the Text chapter
# printed its row with an empty cell, under a sentence promising "the values
# each accepts". And two sentences, in `style.qmd` and `combinations.qmd`, named
# the settings that only `style()` sets and counted them: seven names, "Seven
# settings". The engine had eight. A generated cell cannot drift from the code,
# but a sentence beside it can, and nothing read it. The first run of this guard
# found a third: "The last five rows of the grid above", describing five
# settings, over a grid whose last row had become `angle`.
#
# So this guard asks the engine, and checks the claims a sentence makes:
#
#   1. Every setting a mark takes has something for its Value cell: values the
#      engine lists, or a description in `.gog_open_values`.
#   2. A paragraph that names four or more of the settings with no channel
#      behind them, and counts them ("Eight settings", "Eight of them"), names
#      all of them, and the count is their number.
#   3. A paragraph that names "the last N rows" of the settings grid names the
#      last N settings in the engine's order, which is the grid's order.
#
# A paragraph that names some of the settings and claims nothing about how
# many there are is left alone: a section may well discuss three of them.
#
# Run from the repo root; sourced by r-pkg/gog/tests/test_basic.R, which passes
# the package's own answer for where the engine is.

check_settings <- function(book = "book",
                           cli = Sys.getenv("GOG_CLI_PATH", "target/release/gog-cli"),
                           open_values = NULL) {
  rules <- jsonlite::fromJSON(paste(system2(cli, "--rules", stdout = TRUE), collapse = "\n"))

  # The descriptions are read from the file that prints them, without running
  # it: `setup.R` loads the package and the book's tables, and only one
  # assignment in it is wanted here.
  if (is.null(open_values)) {
    for (e in parse(file.path(book, "R", "setup.R"), keep.source = FALSE)) {
      if (is.call(e) && identical(e[[1]], as.name("<-")) &&
          identical(e[[2]], as.name(".gog_open_values")))
        open_values <- eval(e[[3]], baseenv())
    }
    if (is.null(open_values)) stop("check_settings: no .gog_open_values in R/setup.R")
  }

  # 1. Every Value cell has something in it.
  sc <- rules$setting_cells
  sc <- sc[sc$settable & lengths(sc$values) == 0, ]
  undescribed <- character(0)
  for (s in setdiff(unique(sc$setting), names(open_values))) {
    undescribed <- c(undescribed, sprintf("  `style(%s = )` on %s", s,
      paste0("`", sc$mark[sc$setting == s], "`", collapse = ", ")))
  }

  # 2 and 3. What a paragraph claims about the settings it names.
  only_set <- setdiff(rules$settings, rules$channels)
  numbers <- c("one", "two", "three", "four", "five", "six", "seven", "eight",
               "nine", "ten", "eleven", "twelve", "thirteen", "fourteen",
               "fifteen", "sixteen", "seventeen", "eighteen", "nineteen", "twenty")
  count_pat <- sprintf("\\b(%s)\\s+(settings|of them)\\b", paste(numbers, collapse = "|"))
  rows_pat <- sprintf("\\blast\\s+(%s)\\s+rows\\b", paste(numbers, collapse = "|"))
  qmds <- list.files(book, pattern = "[.]qmd$", recursive = TRUE, full.names = TRUE)
  qmds <- qmds[!grepl("/_", qmds, fixed = TRUE)]
  partial <- character(0)
  miscounted <- character(0)
  for (f in qmds) {
    lines <- readLines(f, warn = FALSE)
    short <- sub(paste0("^", book, "/"), "", f)
    in_chunk <- FALSE
    para <- character(0); start <- NA_integer_
    check_para <- function() {
      if (!length(para)) return()
      text <- paste(para, collapse = " ")
      spans <- gsub("`", "", regmatches(text, gregexpr("`[^`]+`", text))[[1]])
      named <- unique(spans[spans %in% only_set])
      if (length(named) < 4) return()
      counts <- regmatches(text, gregexpr(count_pat, text, ignore.case = TRUE))[[1]]
      if (length(counts)) {
        gone <- setdiff(only_set, named)
        if (length(gone))
          partial <<- c(partial, sprintf("  %s:%d  counts them and names %d of the %d; missing %s",
            short, start, length(named), length(only_set), paste0("`", gone, "`", collapse = ", ")))
        for (m in counts) {
          word <- tolower(sub("\\s.*$", "", m))
          if (match(word, numbers) != length(only_set))
            miscounted <<- c(miscounted, sprintf("  %s:%d  \"%s\", and the engine has %d",
              short, start, m, length(only_set)))
        }
      }
      for (m in regmatches(text, gregexpr(rows_pat, text, ignore.case = TRUE))[[1]]) {
        k <- match(tolower(sub("^last\\s+(\\S+).*$", "\\1", m, ignore.case = TRUE)), numbers)
        tail_k <- utils::tail(rules$settings, k)
        if (!setequal(tail_k, unique(spans[spans %in% rules$settings])))
          miscounted <<- c(miscounted, sprintf("  %s:%d  \"%s\" names %s; the grid's last %d are %s",
            short, start, m, paste0("`", unique(spans[spans %in% rules$settings]), "`", collapse = ", "),
            k, paste0("`", tail_k, "`", collapse = ", ")))
      }
    }
    for (i in seq_along(lines)) {
      line <- lines[i]
      if (grepl("^\\s*```", line)) { check_para(); para <- character(0); in_chunk <- !in_chunk; next }
      if (in_chunk) next
      # A blank line ends a paragraph, and so does the start of a list item.
      if (!nzchar(trimws(line)) || grepl("^\\s*([-*+]|[0-9]+\\.)\\s", line)) {
        check_para(); para <- character(0)
        if (!nzchar(trimws(line))) next
      }
      if (!length(para)) start <- i
      para <- c(para, line)
    }
    check_para()
  }

  n <- length(undescribed) + length(partial) + length(miscounted)
  if (n) {
    if (length(undescribed)) {
      cat("FAIL: a setting's row in the settings table would print an empty Value cell\n")
      cat(paste(undescribed, collapse = "\n"), "\n")
      cat("  The engine lists no values for it. Describe them in .gog_open_values in R/setup.R.\n")
    }
    if (length(partial)) {
      cat("FAIL: a sentence counts the settings only style() sets and leaves some out\n")
      cat(paste(partial, collapse = "\n"), "\n")
      cat("  The engine's list:", paste0("`", only_set, "`", collapse = ", "), "\n")
    }
    if (length(miscounted)) {
      cat("FAIL: a sentence counts the settings, or the grid's rows, and is wrong\n")
      cat(paste(miscounted, collapse = "\n"), "\n")
    }
    stop("check_settings: ", n, " problem(s)")
  }
  cat("PASS: the book's settings match the engine's (", length(only_set),
      "set only with style(), every Value cell described )\n")
  invisible(TRUE)
}
