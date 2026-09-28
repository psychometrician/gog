# book/check_links.R
# A link to a section says where it goes twice: in its text, and in its anchor.
#
# The render checks neither half. Quarto resolves a link's *file* and warns when
# it is missing, but an anchor that reaches no section is written into the page
# as it stands, and the build exits 0: the reader's click lands at the top of the
# chapter, or nowhere. And an anchor that does reach a section can still reach
# the wrong one. `selection.qmd` sent "What can be brushed" to the section on
# stamping, because the text was written for one section and the anchor copied
# from a neighbor. Both are true or false of the source, so both are checked
# here: every anchor must reach something, and a link whose text names a section
# of the target chapter must reach that section.
#
# Run from the repo root; sourced by r-pkg/gog/tests/test_basic.R.

check_links <- function(book = "book") {
  # Paths inside the book, so a chapter is named as the book names it and the
  # rule below reads only the book's own folders.
  rel <- list.files(book, pattern = "[.]qmd$", recursive = TRUE)
  # Build output and withheld drafts, the same rule the prose guard applies.
  rel <- rel[!grepl("(^|/)_", rel)]
  qmds <- file.path(book, rel)

  # What a reader sees of a heading or of a link's text: the words, without the
  # marks that format them. A code span keeps its content, as pandoc's own
  # `stringify` does, so `` `clear` `` reads as clear.
  stringify <- function(s) {
    s <- gsub("\\[([^]]*)\\]\\([^)]*\\)", "\\1", s)   # a link keeps its text
    s <- gsub("<[^>]+>", "", s)                      # raw HTML
    s <- gsub("[`*]", "", s)
    trimws(gsub("\\s+", " ", s))
  }

  # For comparing a link's text with a heading: case and closing punctuation
  # are not what makes two names different.
  same_name <- function(s) sub("[.:,;!?]+$", "", tolower(stringify(s)))

  # The identifier pandoc gives a heading that states none, by pandoc's own rule
  # (`inlineListToIdentifier`, with `auto_identifiers`): lowercase, keep letters,
  # digits, `_`, `-` and `.`, join the words with `-`, and drop everything before
  # the first letter. A repeated heading takes `-1`, `-2` on its later copies.
  auto_id <- function(text) {
    s <- tolower(stringify(text))
    s <- gsub("[^\\p{L}\\p{N}_.\\s-]", "", s, perl = TRUE)
    s <- paste(strsplit(trimws(s), "\\s+", perl = TRUE)[[1]], collapse = "-")
    s <- sub("^[^\\p{L}]+", "", s, perl = TRUE)
    if (!nzchar(s)) "section" else s
  }

  # Every place a link can land in one chapter: its headings, each with the
  # text a reader sees, and every other identifier the page carries (a div or
  # a span given `{#id}`, a chunk's label, an `id=` in raw HTML).
  read_targets <- function(f) {
    lines <- readLines(f, warn = FALSE)
    in_chunk <- FALSE; in_yaml <- FALSE
    heads <- data.frame(id = character(0), text = character(0), level = integer(0))
    other <- character(0)
    seen <- list()
    for (i in seq_along(lines)) {
      line <- lines[i]
      if (i == 1L && grepl("^---\\s*$", line)) { in_yaml <- TRUE; next }
      if (in_yaml) { if (grepl("^---\\s*$", line)) in_yaml <- FALSE; next }
      if (grepl("^\\s*```", line)) { in_chunk <- !in_chunk; next }
      if (in_chunk) {
        lab <- sub("^\\s*#\\|\\s*label:\\s*", "", line)
        if (!identical(lab, line)) other <- c(other, trimws(lab))
        next
      }
      m <- regmatches(line, regexec("^(#{1,6})\\s+(.*?)\\s*$", line))[[1]]
      if (length(m)) {
        text <- m[3]
        attrs <- regmatches(text, regexec("\\{([^}]*)\\}\\s*$", text))[[1]]
        explicit <- NA_character_
        if (length(attrs)) {
          text <- trimws(sub("\\{[^}]*\\}\\s*$", "", text))
          given <- regmatches(attrs[2], regexec("#([^\\s}]+)", attrs[2], perl = TRUE))[[1]]
          if (length(given)) explicit <- given[2]
        }
        id <- explicit
        if (is.na(id)) {
          base <- auto_id(text)
          n <- seen[[base]] %||% 0L
          seen[[base]] <- n + 1L
          id <- if (n == 0L) base else paste0(base, "-", n)
        }
        heads <- rbind(heads, data.frame(id = id, text = text, level = nchar(m[2])))
        next
      }
      for (g in regmatches(line, gregexpr("\\{#[^\\s}]+", line, perl = TRUE))[[1]])
        other <- c(other, sub("^\\{#", "", g))
      for (g in regmatches(line, gregexpr("\\bid=\"[^\"]+\"", line, perl = TRUE))[[1]])
        other <- c(other, sub("^id=\"(.*)\"$", "\\1", g))
    }
    list(heads = heads, ids = c(heads$id, other))
  }

  targets <- list()
  targets_of <- function(f) {
    key <- normalizePath(f, mustWork = FALSE)
    if (is.null(targets[[key]])) targets[[key]] <<- read_targets(f)
    targets[[key]]
  }

  lost <- character(0)
  misnamed <- character(0)

  for (k0 in seq_along(qmds)) {
    f <- qmds[k0]
    short <- rel[k0]
    lines <- readLines(f, warn = FALSE)
    # The prose alone, with the code blanked rather than dropped so that a link's
    # line number is still its line. A link's text is hard-wrapped like the rest
    # of the prose, so links are read across line breaks, from the whole text.
    in_chunk <- FALSE
    for (i in seq_along(lines)) {
      if (grepl("^\\s*```", lines[i])) { in_chunk <- !in_chunk; lines[i] <- ""; next }
      if (in_chunk) lines[i] <- ""
    }
    text <- paste(lines, collapse = "\n")
    hits <- gregexpr("(?<!!)\\[((?:[^][]|\\[[^]]*\\])+)\\]\\(([^)\\s]+)(?:\\s+\"[^\"]*\")?\\)",
                     text, perl = TRUE)[[1]]
    if (hits[1] < 0) next
    starts <- attr(hits, "capture.start")
    lens <- attr(hits, "capture.length")
    newlines <- gregexpr("\n", text, fixed = TRUE)[[1]]
    for (k in seq_along(hits)) {
      label <- substr(text, starts[k, 1], starts[k, 1] + lens[k, 1] - 1L)
      href <- substr(text, starts[k, 2], starts[k, 2] + lens[k, 2] - 1L)
      if (grepl("^[a-z]+:", href) || !grepl("#", href, fixed = TRUE)) next
      page <- sub("#.*$", "", href)
      anchor <- sub("^[^#]*#", "", href)
      if (!nzchar(anchor)) next
      dest <- if (nzchar(page)) file.path(dirname(f), page) else f
      if (!grepl("[.]qmd$", dest) || !file.exists(dest)) next   # the render's half
      at <- sum(newlines > 0 & newlines < hits[k]) + 1L
      where <- sprintf("  %s:%d  [%s](%s)", short, at, stringify(label), href)
      t <- targets_of(dest)
      if (!(anchor %in% t$ids)) {
        lost <- c(lost, where)
        next
      }
      # The text names a section of the target when it *is* one of its
      # headings. Most link text is a phrase ("the one every plot carries") and
      # names no section, so it is left alone. A chapter's own title is not a
      # section: "[Play](play.qmd#what-the-page-adds)" names the chapter and
      # sends the reader into it, which is how the book cites a chapter's part.
      sections <- t$heads[t$heads$level >= 2L, , drop = FALSE]
      named <- sections[vapply(sections$text, same_name, "") == same_name(label), , drop = FALSE]
      if (nrow(named) && !(anchor %in% named$id)) {
        reached <- t$heads$text[match(anchor, t$heads$id)]
        misnamed <- c(misnamed, sprintf("%s\n      names \"%s\", reaches %s", where,
          stringify(named$text[1]),
          if (is.na(reached)) paste0("#", anchor) else sprintf("\"%s\"", stringify(reached))))
      }
    }
  }

  if (length(lost) || length(misnamed)) {
    if (length(lost)) {
      cat("FAIL: a link's anchor reaches no section of its chapter\n")
      cat(paste(lost, collapse = "\n"), "\n", sep = "")
      cat("  The render writes the link anyway and exits 0. Point it at a heading's",
          "anchor, or give the target an explicit {#id}.\n")
    }
    if (length(misnamed)) {
      cat("FAIL: a link's text names one section and its anchor reaches another\n")
      cat(paste(misnamed, collapse = "\n"), "\n", sep = "")
      cat("  Make the anchor the named section's, or the text the reached one's.\n")
    }
    stop("check_links: ", length(lost) + length(misnamed), " link(s) reach the wrong place")
  }
  cat("PASS: every link to a section reaches it (", length(qmds), "files )\n")
  invisible(TRUE)
}

`%||%` <- function(a, b) if (is.null(a)) b else a
