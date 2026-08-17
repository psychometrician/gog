# check_sentence_order.R — do the book's plots keep the order it claims they do?
#
# `operators.qmd` states it outright: "why does every plot in this book follow
# the same one: *data, mark, positions, refinements*?" and closes the section
# with "the payoff of keeping one order everywhere is that once your eye learns
# it, every plot in the book, and every plot you write, parses the same way."
#
# On 2026-07-28 that was false in 24 places. The measurement: 527 single-plot
# specifications, 449 canonical, 78 not. Most of the exceptions were legitimate
# and are excluded below. Twenty-four were drift, and the tell was that chapters
# disagreed *with themselves* — `marks/index.qmd` wrote `data + bar * bin +
# x(life)` at one heading and `data + x + y + point` four headings later,
# `polar.qmd` wrote the sunburst both ways on one page, and `data.qmd`'s own
# opening example is canonical while two later ones were not.
#
# Reordering them changed nothing on the page: every affected chapter's SVG came
# back byte-identical, which is the same fact `operators.qmd` demonstrates when
# it draws the identical histogram twice with `color` moved. That is *why* the
# convention needs a checker rather than a test of behavior. Nothing breaks when
# it is violated. The picture is right, the sentence just stops scanning like
# the others, and no reader can report a bug they only feel as friction.
#
# What is checked, and what is deliberately not:
#
#   * Only **single-layer** specifications, meaning one `data()` and one mark.
#     With two marks, a position written *before* them is plot-scoped and
#     reaches both, which is a scope decision rather than a style one and is the
#     subject of `encoding-scope.qmd`. `operators.qmd`'s own layering example
#     writes `data + x + y + point + ... + data + rule`, correctly.
#   * Faceted and composed expressions (`|`, `/`) are skipped: their shape is
#     decided by the operator, not by this convention.
#   * One exact exemption, listed with its reason: the pair in `operators.qmd`
#     that proves channels commute has to write the second one out of order, or
#     it demonstrates nothing.
#
# Run from the repo root; sourced by r-pkg/gog/tests/test_basic.R.

check_sentence_order <- function(book = "book") {
  fail <- function(...) stop(..., call. = FALSE)

  MARKS <- c("point", "line", "bar", "area", "step", "interval", "box",
             "ribbon", "text", "path", "rule", "zone", "surface", "edge")
  POSITIONS <- c("x", "y", "z")

  # The plots that must be out of order, and why. Both are in `operators.qmd`,
  # both sit beside a canonical twin under `layout-ncol: 2`, and both exist to
  # show that moving a channel changes nothing. Putting either in order would
  # delete the evidence it was written to provide. Any addition here needs the
  # same kind of reason: the plot's subject is the ordering itself.
  EXEMPT <- c(
    # "Which parts of the order are yours to choose" — the Free row.
    "data(gapminder_2007) + x(gdp) + point + y(life)",
    # "The order of `+`" — the same histogram with `color` moved ahead of `x`.
    "data(iris_flowers) + bar * bin + color(species) + x(petal_length)"
  )

  # Split on `+` at depth zero, so a `+` inside `style(...)` or a string does
  # not cut a term in half.
  split_terms <- function(s) {
    out <- character(); depth <- 0L; cur <- ""
    for (ch in strsplit(s, "")[[1]]) {
      if (ch %in% c("(", "[", "{")) depth <- depth + 1L
      if (ch %in% c(")", "]", "}")) depth <- depth - 1L
      if (ch == "+" && depth == 0L) { out <- c(out, cur); cur <- "" }
      else cur <- paste0(cur, ch)
    }
    trimws(c(out, cur))[nzchar(trimws(c(out, cur)))]
  }
  classify <- function(t) {
    head <- sub("^\\(*", "", strsplit(trimws(t), "[ (*]")[[1]][1])
    if (identical(head, "data")) return("D")
    base <- strsplit(trimws(strsplit(t, "\\*")[[1]][1]), "[ (]")[[1]][1]
    base <- gsub("[()]", "", base)
    if (base %in% MARKS) return("M")
    if (head %in% POSITIONS) return("P")
    "R"
  }

  qmd <- c(list.files(book, pattern = "\\.qmd$", full.names = TRUE),
           unlist(lapply(file.path(book, c("marks", "parts", "cookbook", "bindings")),
                         list.files, pattern = "\\.qmd$", full.names = TRUE)))
  problems <- character(); checked <- 0L

  for (f in qmd) {
    ln <- readLines(f, warn = FALSE)
    fences <- grep("^```", ln)
    if (length(fences) < 2) next
    for (k in seq(1, length(fences) - 1, by = 2)) {
      if (!grepl("^```\\{r\\}", ln[fences[k]])) next
      body <- ln[(fences[k] + 1):(fences[k + 1] - 1)]
      if (any(grepl("error: true|include: false", body))) next
      body <- body[!grepl("^\\s*#", body)]
      # Strip a trailing comment, but only a `#` outside quotes — colors are
      # written `style(color = "#9e9e9e")`. Without this the continuation join
      # stops at the first commented line, and `encoding-scope.qmd`'s two-mark
      # example (`point + color(continent) +  # …` then `line`) reads as a
      # single-layer plot with its positions in the wrong place.
      body <- vapply(body, function(l) {
        chars <- strsplit(l, "")[[1]]; inq <- FALSE; cut <- NA_integer_
        for (n in seq_along(chars)) {
          if (chars[n] == '"') inq <- !inq
          else if (chars[n] == "#" && !inq) { cut <- n; break }
        }
        if (is.na(cut)) l else substr(l, 1, cut - 1L)
      }, character(1), USE.NAMES = FALSE)
      body <- body[nzchar(trimws(body))]
      # Join continuation lines: a statement continues while the line ends in an
      # operator or a comma.
      i <- 1L
      while (i <= length(body)) {
        if (!grepl("^data\\(", body[i])) { i <- i + 1L; next }
        j <- i
        while (j < length(body) && grepl("[+*,]\\s*$", body[j])) j <- j + 1L
        stmt <- paste(trimws(body[i:j]), collapse = " ")
        i <- j + 1L
        if (grepl("\\|", stmt) || grepl("\\)\\s*/", stmt)) next
        if (stmt %in% EXEMPT) next
        terms <- split_terms(stmt)
        kinds <- vapply(terms, classify, character(1), USE.NAMES = FALSE)
        if (sum(kinds == "M") != 1L || sum(kinds == "D") != 1L) next  # multi-layer
        checked <- checked + 1L
        if (!grepl("^DM*P*R*$", paste(kinds, collapse = "")))
          problems <- c(problems, sprintf(
            "%s: %s\n      is %s, wanted data, mark, positions, refinements",
            sub(paste0("^", book, "/"), "", f), substr(stmt, 1, 68),
            paste(kinds, collapse = "")))
      }
    }
  }

  if (length(problems)) {
    for (p in problems) message("  ", p)
    fail(sprintf("FAIL: %d plots break the sentence order operators.qmd claims they keep",
                 length(problems)))
  }
  message(sprintf("check_sentence_order: OK (%d single-layer plots read data, mark, positions, refinements)",
                  checked))
  invisible(TRUE)
}
