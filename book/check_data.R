# book/check_data.R
# What the book says about its shared tables matches the tables.
#
# The tables in `book/data/` are published for readers of all four languages,
# and two pages describe them: `data/LICENSE.md` puts each one under its terms,
# and Book Data names the columns a reader must read as text. Both are written
# by hand, and both fell behind on 2026-10-08. Eleven tables had been added with
# no license tier, which no check could see, and Book Data still said "five
# columns" in two places after the list beside it had grown to eight.
#
# Three questions, each asked of the files rather than of anyone's memory:
#
# * Every CSV in `book/data/` is named in `data/LICENSE.md`.
# * The `text =` columns Book Data spells out for `gog_table()` are exactly the
#   columns `data.R` reads as text whose values all look like numbers, the ones
#   a reader's CSV reader would turn into numbers. Its "Only ... need a `text`
#   argument" sentence names exactly their tables.
# * Every number word Book Data puts before those columns ("eight columns")
#   is their count.

check_data <- function(book_dir = "book") {
  data_dir <- file.path(book_dir, "data")
  problems <- character()

  # -- Every table has a license tier ---------------------------------------
  license <- paste(readLines(file.path(data_dir, "LICENSE.md"), warn = FALSE),
                   collapse = "\n")
  tables <- sub("\\.csv$", "", list.files(data_dir, pattern = "\\.csv$"))
  unlisted <- tables[!vapply(tables, function(t)
    grepl(paste0("`", t, "`"), license, fixed = TRUE), logical(1))]
  if (length(unlisted))
    problems <- c(problems, sprintf("  data/LICENSE.md names no tier for `%s`", unlisted))

  # -- The columns a reader must read as text -------------------------------
  # From `data.R`: each `.gog_read("name", chr = ...)` call, and of its `chr`
  # columns the ones whose every value reads as a number.
  calls <- list()
  walk <- function(e) {
    if (is.call(e)) {
      if (identical(e[[1]], as.name(".gog_read")) && !is.null(e$chr))
        calls[[length(calls) + 1]] <<- list(table = e[[2]], chr = eval(e$chr))
      for (part in as.list(e)[-1]) walk(part)
    }
  }
  for (e in parse(file.path(book_dir, "R", "data.R"), keep.source = FALSE)) walk(e)
  numeric_like <- character()
  for (call in calls) {
    frame <- utils::read.csv(file.path(data_dir, paste0(call$table, ".csv")),
                             colClasses = "character", na.strings = "",
                             check.names = FALSE)
    for (column in call$chr) {
      values <- frame[[column]][!is.na(frame[[column]])]
      if (length(values) && !anyNA(suppressWarnings(as.numeric(values))))
        numeric_like <- c(numeric_like, paste0(call$table, "$", column))
    }
  }
  numeric_like <- sort(unique(numeric_like))

  page <- file.path(book_dir, "book-data.qmd")
  if (file.exists(page)) {
    text <- paste(readLines(page, warn = FALSE), collapse = "\n")

    # The R spelling of the block after "takes a `text` argument".
    after <- regexpr("takes a `text` argument", text, fixed = TRUE)
    named <- character()
    if (after > 0) {
      rest <- substring(text, after)
      spelled <- regmatches(rest, regexpr("r = r\"\\((.|\n)*?\\)\",\n  python", rest, perl = TRUE))
      calls_r <- regmatches(spelled, gregexpr('gog_table\\("[^"]+", text = [^)]+\\)', spelled))[[1]]
      for (one in calls_r) {
        table <- sub('gog_table\\("([^"]+)".*', "\\1", one)
        columns <- regmatches(sub('.*text = ', "", one), gregexpr('"[^"]+"', sub('.*text = ', "", one)))[[1]]
        named <- c(named, paste0(table, "$", gsub('"', "", columns)))
      }
    }
    named <- sort(unique(named))
    missing <- setdiff(numeric_like, named)
    extra <- setdiff(named, numeric_like)
    if (length(missing))
      problems <- c(problems, sprintf(
        "  book-data.qmd spells no `text =` for %s, whose values look like numbers", missing))
    if (length(extra))
      problems <- c(problems, sprintf(
        "  book-data.qmd spells `text =` for %s, which data.R does not read as a number-like label", extra))

    only <- regmatches(text, regexpr("Only (.|\n)*?need a\\s+`text`\\s+argument", text, perl = TRUE))
    if (length(only)) {
      said <- sort(unique(gsub("`", "", regmatches(only, gregexpr("`[^`]+`", only))[[1]])))
      said <- setdiff(said, "text")
      should <- sort(unique(sub("\\$.*", "", numeric_like)))
      if (!identical(said, should))
        problems <- c(problems, sprintf(
          "  book-data.qmd's \"Only ... need a `text` argument\" names %s; the tables are %s",
          paste(said, collapse = ", "), paste(should, collapse = ", ")))
    }

    words <- c("one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
               "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen")
    counted <- regmatches(text, gregexpr(
      "(?i)\\b(\\w+) columns (in these tables|named below)", text, perl = TRUE))[[1]]
    for (phrase in counted) {
      word <- tolower(sub("^(?i)(the )?(\\w+) columns.*$", "\\2", phrase, perl = TRUE))
      word <- sub("^the ", "", word)
      if (match(word, words, nomatch = 0) != length(numeric_like))
        problems <- c(problems, sprintf(
          "  book-data.qmd says \"%s\"; the count is %d", phrase, length(numeric_like)))
    }
  }

  if (length(problems)) {
    cat("FAIL: what the book says about its tables does not match them\n")
    cat(paste(problems, collapse = "\n"), "\n")
    cat("  A new table goes into a tier of data/LICENSE.md. A column data.R reads\n",
        "  as text with values like numbers is named in Book Data's `text =` block,\n",
        "  its sentence of tables, and its counts.\n", sep = "")
    stop("check_data: ", length(problems), " mismatch(es)")
  }
  cat("PASS: every table has a license tier, and Book Data names its",
      length(numeric_like), "text columns (", length(tables), "tables )\n")
  invisible(TRUE)
}
