# translate.R — one gog sentence, from the book's R spelling to Julia's
#
# The third translator, and the smallest of the three, because Julia is the one
# target that changes almost nothing. It can overload `+`, `*`, `|` and `/`, so
# the operators survive; it has symbols, so a column keeps the column/value
# distinction R's bare names give. What is left is a colon:
#
#   data(gm) + bar * bin + x(life) | facet(era)      the book's R
#   data(gm) + bar * bin + x(:life) | facet(:era)    the same sentence in Julia
#
# So this is a *re-emit* of R's parse tree with the operators intact, where
# `js-pkg`'s counterpart had to reassociate the tree into four structural words
# and `translate.py` does a handful of local rewrites. Three languages, three
# distances from the original, and the distance is the finding.
#
# Anything the emitter does not recognize is recorded as a **gap** and the
# sentence is dropped from the count rather than guessed at. A coverage number
# produced by guessing is a lie, which is the silent drop this project refuses
# everywhere else.
#
# Two callers, so this file is definitions only and nothing runs on `source()`:
# `run.jl` (the parity harness) and `book/R/tabs.R` (the manual's fourth tab).

# `colour` is exported only to be refused, but it takes its argument the way
# `color` does, so the accessor must still be added or the sentence fails on the
# *column* name and never reaches the refusal being compared.
JL_COLUMN_ATOMS <- c("x", "y", "z", "color", "size", "shape", "opacity",
                     "label", "pattern", "group", "facet", "order", "play", "brush",
                     "colour")
JL_ALL_COLUMN_ARGS <- c("bounds", "partition")
JL_PAIR_KEYWORDS <- c("lower", "upper", "start", "end")

jl_gaps <- new.env(parent = emptyenv())
jl_note_gap <- function(what) assign(what, TRUE, envir = jl_gaps)
jl_reset_gaps <- function() rm(list = ls(jl_gaps), envir = jl_gaps)
jl_seen_gaps <- function() ls(jl_gaps)

# `$` is escaped as well as `\` and `"`, because a Julia string interpolates:
# `title("Under $30,000")` reads `$30` as an expression and fails to parse, which
# is exactly what a Julia user would hit and exactly what they would write `\$`
# to fix. Found when a book title first carried a dollar sign; nothing about the
# grammar, everything about the host language's string syntax — which is the
# class of difference this harness exists to catch.
#
# The `$` pass is `fixed = TRUE`, which makes the *replacement* literal as well
# as the pattern — so it takes the two characters `\$` and not the `\\$` the
# other two passes need, where the replacement is still read for backreferences.
# Getting that wrong emits `\\$`, which Julia reads as a backslash followed by an
# interpolation and fails to parse in exactly the same place.
jl_quote <- function(s)
  paste0('"', gsub("$", "\\$", gsub('"', '\\\\"', gsub("\\\\", "\\\\\\\\", s)),
                   fixed = TRUE), '"')

# `:gdp`, or `Symbol("life exp")` when the name is not a Julia identifier.
# Julia identifiers are Unicode, so a Hangeul column name is `:지역` and not sent
# through the escape hatch — the same correction the JavaScript emitter needed.
jl_symbol <- function(name) {
  if (grepl("^[\\p{L}_][\\p{L}\\p{N}_!]*$", name, perl = TRUE)) paste0(":", name)
  else paste0("Symbol(", jl_quote(name), ")")
}

# `end` is reserved syntax in Julia, so `bounds(end = x)` needs the language's own
# escape for a name it cannot otherwise write. Keeping the argument's *name* the
# same in all four bindings is worth the four characters; inventing a Julia-only
# synonym would not be.
jl_keyword <- function(name) if (name == "end") "var\"end\"" else name

# ---- values -------------------------------------------------------------

jl_value <- function(v) {
  if (is.character(v)) return(jl_quote(v))
  if (is.logical(v))   return(if (is.na(v)) "missing" else if (v) "true" else "false")
  if (is.numeric(v))   return(as.character(v))
  if (is.null(v))      return("nothing")
  if (is.name(v))      return(as.character(v))          # a bare identifier value
  if (is.call(v)) {
    fn <- deparse(v[[1]])
    parts <- as.list(v)[-1]
    # A refusal has to survive its container. `NA_character_` is how every
    # branch here says "I cannot spell this", but `paste()` renders NA as the
    # two characters `NA`, and Julia's word is `missing` — so an element this
    # emitter refused came out the other side as a literal it has no name for.
    # `data.frame` below checks its columns; `c` did not, and
    # `c(target_band$lower, target_band$upper)` in `marks/zone.qmd` reached the
    # book as `(sales = [NA, NA],)`: a tab that renders, reports success, and
    # cannot run. Declining is the correct answer, and it has to be returned.
    if (fn == "c") {
      items <- vapply(parts, jl_value, "")
      if (any(is.na(items))) return(NA_character_)
      return(paste0("[", paste(items, collapse = ", "), "]"))
    }
    if (fn == "-" && length(parts) == 1)
      return(paste0("-", jl_value(parts[[1]])))
    if (fn == "exp" && length(parts) == 1 && identical(parts[[1]], 1))
      return("ℯ")                                   # Julia's own ℯ
    if (fn == "data.frame") {
      nms <- names(parts)
      if (is.null(nms) || any(!nzchar(nms))) {
        jl_note_gap("data.frame with unnamed columns"); return(NA_character_)
      }
      cells <- vapply(seq_along(parts), function(i) {
        inner <- jl_value(parts[[i]])
        if (is.na(inner)) return(NA_character_)
        # A column is a vector at every length — the wire invariant a one-row
        # frame broke once already (`df_to_wire`, and the `I()` that fixed it).
        if (!startsWith(inner, "[")) inner <- paste0("[", inner, "]")
        paste0(nms[i], " = ", inner)
      }, "")
      if (any(is.na(cells))) return(NA_character_)
      # The trailing comma is not decoration: `(x = [1])` is a parenthesized
      # assignment in Julia, and only `(x = [1],)` is a one-column table.
      return(paste0("(", paste(cells, collapse = ", "), ",)"))
    }
    jl_note_gap(paste0("value call `", fn, "()`"))
    return(NA_character_)
  }
  jl_note_gap(paste0("value of type ", class(v)[1]))
  NA_character_
}

# ---- atoms --------------------------------------------------------------

jl_atom <- function(e) {
  fn <- deparse(e[[1]])
  parts <- as.list(e)[-1]
  nms <- names(parts); if (is.null(nms)) nms <- rep("", length(parts))

  every <- fn %in% JL_ALL_COLUMN_ARGS
  out <- character(); seen <- 0L

  for (i in seq_along(parts)) {
    arg <- parts[[i]]; nm <- nms[i]
    if (nzchar(nm)) {
      if ((every || nm %in% JL_PAIR_KEYWORDS) && is.name(arg)) {
        out <- c(out, paste0(jl_keyword(nm), " = ", jl_symbol(as.character(arg))))
      } else {
        v <- jl_value(arg); if (is.na(v)) return(NA_character_)
        out <- c(out, paste0(jl_keyword(nm), " = ", v))
      }
    } else {
      seen <- seen + 1L
      is_column <- (every || (seen == 1L && fn %in% JL_COLUMN_ATOMS)) && is.name(arg)
      if (is_column) {
        out <- c(out, jl_symbol(as.character(arg)))
      } else {
        v <- jl_value(arg); if (is.na(v)) return(NA_character_)
        out <- c(out, v)
      }
    }
  }

  paste0(fn, "(", paste(out, collapse = ", "), ")")
}

# ---- wrapping ------------------------------------------------------------
#
# The sentence is rebuilt from R's parse tree, so whatever line breaks the
# manual typed are gone by the time this text exists, and nothing put any back.
# Measured across the rendered book before this existed: 355 of 466 Julia blocks
# were wider than 80 columns and the widest was 521, against zero R blocks over
# 96 — R keeps its author's line breaks and Julia lost them. Wide blocks scroll
# sideways in the HTML, so a reader saw the half that fitted.
#
# A break is only ever placed *after* an operator that already continues the
# expression: Julia reads a line ending in `+`, `|` or `/` as unfinished, and a
# newline inside parentheses is free. So the wrapped text parses exactly as the
# one-liner did, and `run.jl` proves it by evaluating this and comparing the
# drawing with R's.
jl_break_points <- function(text) {
  chars <- strsplit(text, "", fixed = TRUE)[[1]]
  n <- length(chars)
  pos <- integer(0)
  depth <- integer(0)
  here <- 0L
  in_str <- FALSE
  esc <- FALSE
  for (i in seq_len(n)) {
    ch <- chars[[i]]
    if (esc) { esc <- FALSE; next }
    if (in_str) {
      if (ch == "\\") esc <- TRUE else if (ch == "\"") in_str <- FALSE
      next
    }
    if (ch == "\"") { in_str <- TRUE; next }
    if (ch %in% c("(", "[", "{")) { here <- here + 1L; next }
    if (ch %in% c(")", "]", "}")) { here <- here - 1L; next }
    if (ch %in% c("+", "|", "/") && i > 1L && i < n &&
        chars[[i - 1L]] == " " && chars[[i + 1L]] == " ") {
      pos <- c(pos, i)
      depth <- c(depth, here)
    }
  }
  list(pos = pos, depth = depth)
}

# Of the breaks that fit, take the **shallowest**, and the furthest of those.
# Filling each line as full as possible instead would cut a plot in half at
# whatever `+` happened to land on column 74, where breaking at the `|` between
# two plots gives back the shape the manual's R had. If nothing fits, the first
# break past the edge beats a line nothing can break.
jl_wrap <- function(text, width = 74L, indent = "  ") {
  pts <- jl_break_points(text)
  total <- nchar(text)
  if (total <= width || !length(pts$pos)) return(text)
  lines <- character(0)
  start <- 1L
  prefix <- ""
  repeat {
    if (nchar(prefix) + (total - start + 1L) <= width) {
      lines <- c(lines, paste0(prefix, substr(text, start, total)))
      break
    }
    room <- width - nchar(prefix)
    fits <- which(pts$pos >= start & (pts$pos - start + 1L) <= room)
    if (length(fits)) {
      shallow <- fits[pts$depth[fits] == min(pts$depth[fits])]
      cut <- max(pts$pos[shallow])
    } else {
      later <- pts$pos[pts$pos >= start]
      if (!length(later)) {
        lines <- c(lines, paste0(prefix, substr(text, start, total)))
        break
      }
      cut <- later[[1]]
    }
    lines <- c(lines, paste0(prefix, substr(text, start, cut)))
    start <- cut + 1L
    while (start <= total && substr(text, start, start) == " ") start <- start + 1L
    if (start > total) break
    prefix <- indent
  }
  paste(lines, collapse = "\n")
}

# ---- the tree, with its operators kept ----------------------------------

jl_expr <- function(e) {
  if (is.call(e)) {
    fn <- deparse(e[[1]])
    if (fn %in% c("+", "*", "|", "/") && length(e) == 3) {
      left <- jl_expr(e[[2]]); right <- jl_expr(e[[3]])
      if (is.na(left) || is.na(right)) return(NA_character_)
      return(paste0(left, " ", fn, " ", right))
    }
    # **The parentheses are kept.** They used to be unwrapped, on the reading
    # that Julia's precedence matches R's — true of `+` and `*`, and false of
    # exactly the sentence composition introduced: `a / (b | c)` unwrapped to
    # `a / b | c`, which is `(a / b) | c` and a different page. Keeping them is
    # neutral everywhere else, since the tree the parentheses describe is the
    # tree R itself parsed.
    if (fn == "(") {
      inner <- jl_expr(e[[2]])
      if (is.na(inner)) return(NA_character_)
      return(paste0("(", inner, ")"))
    }
    return(jl_atom(e))
  }
  if (is.name(e)) return(as.character(e))
  jl_value(e)
}

# One R sentence → one Julia sentence.
#
# Returns `list(julia =, blocked =)`. `blocked` distinguishes the two ways a
# translation can be absent: a sentence *about the host language* (the R
# chapter's pipes) from one the emitter could not handle, which is a defect.
translate_julia <- function(source) {
  # R's pipes are a host-language idiom the R chapter exists to document. Julia
  # has a `|>`, but it is function application with different semantics — it
  # would not spell these sentences, it would spell different ones — so they are
  # declined rather than mangled, exactly as the other two translators decline
  # the same seven.
  if (grepl("\\|>", source) || grepl("%>%", source))
    return(list(julia = NA_character_, blocked = "R pipe — a host-language idiom"))

  # `save_gif()` names a file, and naming a file is host bookkeeping rather than
  # grammar: R writes `file.path(tempdir(), …)` and Julia spells the same thing
  # its own way. The two the book documents also bind their plot on the line
  # above, so what the extractor records is one line short of standing alone.
  # Declining costs no coverage — a parity run compares the picture a sentence
  # draws, and this call writes a file instead of returning one.
  if (grepl("\\bsave_gif[[:space:]]*\\(", source))
    return(list(julia = NA_character_,
                blocked = "save_gif names a file — host bookkeeping, not a sentence"))

  # R's own extractor on a table, as in `df[order(df$pop), ]`, which the R
  # chapter's masked-names section documents. That is host arithmetic rather than
  # a sentence, so there is nothing here for Julia to spell. Matched as syntax (a
  # name followed by `[`) and not by the characters, because `$` also appears
  # inside a legitimate title string that must keep translating.
  if (grepl("[A-Za-z0-9._)\"][[:space:]]*\\[", source))
    return(list(julia = NA_character_, blocked = "R subsetting — a host-language idiom"))

  # An R **formula**, as in `aggregate(life ~ continent, df, mean)`. `~` is R
  # syntax with no counterpart in the other three languages: it captures an
  # unevaluated expression, which is the same host-language category as the pipe
  # and the extractor above. The R chapter uses one to show `mean` reaching base
  # R rather than gog. Matched on a bare `~` after the comments come off, since
  # nothing else in the grammar writes one.
  if (grepl("~", gsub("#[^\n]*", "", source)))
    return(list(julia = NA_character_, blocked = "R formula — a host-language idiom"))

  parsed <- tryCatch(parse(text = source), error = function(e) NULL)
  if (is.null(parsed) || !length(parsed))
    return(list(julia = NA_character_, blocked = "did not parse as R"))

  out <- character()
  for (expr in as.list(parsed)) {
    # A table the chunk defines for itself keeps its *name*, because the sentence
    # below is about to use it. Only the spec assignment (`p <- data(…) + …`) is
    # droppable, where the name is R's bookkeeping rather than content.
    # `jl_value()` already refuses a table built by R computation. That refusal
    # is a *decline*, not a defect, and it says so — `translate.py` blocks the
    # same tables with the same sentence. It was reported as an unexplained gap
    # until 2026-07-28, which made `tabs.R` count it as an emitter failure and
    # warn on every render: 12 sentences across 6 chapters, none of them
    # actually broken. Nothing is wrong with the emitter when a chunk writes
    # `exp(-(0:5)) * 100`; that is R arithmetic, a host-language idiom exactly
    # like the pipe declined above, and another language would build the table
    # its own way.
    if (is.call(expr) && deparse(expr[[1]]) %in% c("<-", "=") &&
        is.call(expr[[3]]) && identical(deparse(expr[[3]][[1]]), "data.frame")) {
      literal <- jl_value(expr[[3]])
      if (is.na(literal))
        return(list(julia = NA_character_,
                    blocked = "table computed in R, not written out as literal columns"))
      out <- c(out, paste0(deparse(expr[[2]]), " = ", literal))
      next
    }
    if (is.call(expr) && deparse(expr[[1]]) %in% c("<-", "=")) expr <- expr[[3]]
    # `render_svg(<sentence>)` is the book's `error: true` idiom — the host's
    # render call, which every binding spells its own way. Not an atom.
    if (is.call(expr) && deparse(expr[[1]]) == "render_svg" && length(expr) == 2)
      expr <- expr[[2]]
    text <- jl_expr(expr)
    if (is.na(text)) return(list(julia = NA_character_, blocked = NA_character_))
    out <- c(out, jl_wrap(text))
  }
  list(julia = paste(out, collapse = "\n"), blocked = NA_character_)
}

# Every sentence in the corpus, translated, written where a caller asked for it.
write_julia_translations <- function(corpus, out_path) {
  sentences <- jsonlite::fromJSON(file.path(corpus, "sentences.json"),
                                  simplifyDataFrame = FALSE)
  jl_reset_gaps()
  results <- lapply(sentences, function(s) {
    got <- translate_julia(s$source)
    list(id = s$id,
         julia = if (is.na(got$julia)) NULL else got$julia,
         blocked = if (is.na(got$blocked)) NULL else got$blocked)
  })
  jsonlite::write_json(results, out_path, auto_unbox = TRUE, null = "null",
                       pretty = FALSE)
  invisible(list(total = length(results), gaps = jl_seen_gaps()))
}
