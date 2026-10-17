# book/i18n/tools/add-heading-ids.R
# One-off: give every level-2-and-deeper heading of the English book an explicit
# `{#id}` equal to the identifier pandoc gives it today, so a translated heading
# keeps the same anchor and every link to it still lands.
#
# Usage, from the repo root:
#   Rscript book/i18n/tools/add-heading-ids.R <rendered-book-dir>          # check only
#   Rscript book/i18n/tools/add-heading-ids.R <rendered-book-dir> --write  # then write
#
# <rendered-book-dir> holds the book as rendered from the same source (`_book`,
# or the published site downloaded page by page). Before writing anything, every
# page's computed identifiers are compared, in order, with the `<section id>`
# sequence the render wrote, and any page that differs stops the run.

source("book/check_common.R")
args <- commandArgs(trailingOnly = TRUE)
rendered <- args[1]
write <- "--write" %in% args

y <- yaml::read_yaml("book/_quarto.yml")
pages <- unlist(lapply(c(y$book$chapters, y$book$appendices), function(c)
  if (is.list(c)) c(c$part, unlist(c$chapters)) else c))
pages <- pages[grepl("[.]qmd$", pages)]

# Each heading's identifier by pandoc's rule: a stated one is kept; an automatic
# one that is already taken gets `-1`, `-2`, … until it is free. Every identifier
# counts as taken once given, stated or not.
plan_ids <- function(lines) {
  out <- list(); used <- character(0)
  for (i in prose_lines(lines)) {
    h <- parse_heading(lines[i])
    if (is.null(h)) next
    id <- h$id
    if (is.na(id)) {
      base <- auto_id(h$text); id <- base; k <- 0L
      while (id %in% used) { k <- k + 1L; id <- paste0(base, "-", k) }
    }
    used <- c(used, id)
    out[[length(out) + 1L]] <- list(line = i, level = h$level, id = id, stated = !is.na(h$id),
                                    attrs = h$attrs)
  }
  out
}

bad <- 0L; written <- 0L; added <- 0L
for (p in pages) {
  f <- file.path("book", p)
  lines <- readLines(f, warn = FALSE)
  plan <- plan_ids(lines)
  deep <- Filter(function(h) h$level >= 2L, plan)
  html <- file.path(rendered, sub("[.]qmd$", ".html", p))
  got <- regmatches(readLines(html, warn = FALSE),
                    gregexpr('<section id="[^"]+" class="level[2-6]', readLines(html, warn = FALSE)))
  got <- sub('^<section id="([^"]+)".*$', "\\1", unlist(got))
  want <- vapply(deep, `[[`, "", "id")
  if (!identical(want, got)) {
    bad <- bad + 1L
    cat("MISMATCH ", p, ": ", length(want), " computed, ", length(got), " rendered\n", sep = "")
    n <- max(length(want), length(got))
    w <- c(want, rep("", n - length(want))); g <- c(got, rep("", n - length(got)))
    d <- which(w != g)
    for (j in head(d, 4)) cat("   #", j, " computed ", w[j], " | rendered ", g[j], "\n", sep = "")
    next
  }
  if (!write) next
  for (h in deep) {
    if (h$stated) next
    line <- lines[h$line]
    lines[h$line] <- if (is.na(h$attrs)) {
      paste0(sub("\\s+$", "", line), " {#", h$id, "}")
    } else {
      sub("\\{([^}]*)\\}\\s*$", paste0("{#", h$id, " \\1}"), line)
    }
    added <- added + 1L
  }
  writeLines(lines, f)
  written <- written + 1L
}
cat(length(pages), "pages;", bad, "mismatched;", if (write) paste(added, "ids written in", written, "files") else "nothing written", "\n")
if (bad) quit(status = 1)
