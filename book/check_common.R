# book/check_common.R
# What more than one book guard needs: how a heading is read, the identifier
# pandoc gives it, and which `.qmd` files are the English book.
#
# Sourced by the guards that use it (each sources it if `english_qmds` is not
# defined yet) and by r-pkg/gog/tests/test_basic.R. Run from the repo root.

# A translated chapter sits beside its English source with the language's
# two-letter code before the extension: `first-plot.ko.qmd`. The English book is
# every other `.qmd`. Every guard reads the English book only; translated files
# have their own check, which compares each with the English it came from.
is_translation <- function(path) grepl("[.][^./]{2}[.]qmd$", path)

# The English `.qmd` files under `dir`, without build output and withheld drafts
# (a path part that starts with `_`).
english_qmds <- function(dir = "book", full.names = TRUE, recursive = TRUE) {
  rel <- list.files(dir, pattern = "[.]qmd$", recursive = recursive)
  rel <- rel[!grepl("(^|/)_", rel) & !is_translation(rel)]
  if (full.names) file.path(dir, rel) else rel
}

# What a reader sees of a heading or of a link's text: the words, without the
# marks that format them. A code span keeps its content, as pandoc's own
# `stringify` does, so `` `clear` `` reads as clear.
stringify <- function(s) {
  s <- gsub("\\[([^]]*)\\]\\([^)]*\\)", "\\1", s)   # a link keeps its text
  s <- gsub("<[^>]+>", "", s)                      # raw HTML
  s <- gsub("[`*]", "", s)
  trimws(gsub("\\s+", " ", s))
}

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

# A heading line split into its parts: level, the words, and the `{…}` block
# after them (its identifier, if it states one). `NULL` for a line that is not
# a heading.
parse_heading <- function(line) {
  m <- regmatches(line, regexec("^(#{1,6})\\s+(.*?)\\s*$", line))[[1]]
  if (!length(m)) return(NULL)
  text <- m[3]
  attrs <- regmatches(text, regexec("\\{([^}]*)\\}\\s*$", text))[[1]]
  id <- NA_character_
  if (length(attrs)) {
    text <- trimws(sub("\\{[^}]*\\}\\s*$", "", text))
    given <- regmatches(attrs[2], regexec("#([^\\s}]+)", attrs[2], perl = TRUE))[[1]]
    if (length(given)) id <- given[2]
  }
  list(level = nchar(m[2]), text = text, id = id,
       attrs = if (length(attrs)) attrs[2] else NA_character_)
}

# A heading line's words alone, without its `{…}` block, so a guard that reads
# a heading's text reads it the same with or without an identifier.
heading_text <- function(line) {
  h <- parse_heading(line)
  if (is.null(h)) line else paste(strrep("#", h$level), h$text)
}

# The line numbers of a `.qmd`'s prose: not front matter, not inside a fence.
prose_lines <- function(lines) {
  keep <- logical(length(lines))
  in_yaml <- FALSE; in_chunk <- FALSE
  for (i in seq_along(lines)) {
    line <- lines[i]
    if (i == 1L && grepl("^---\\s*$", line)) { in_yaml <- TRUE; next }
    if (in_yaml) { if (grepl("^---\\s*$", line)) in_yaml <- FALSE; next }
    if (grepl("^\\s*```", line)) { in_chunk <- !in_chunk; next }
    if (!in_chunk) keep[i] <- TRUE
  }
  which(keep)
}
