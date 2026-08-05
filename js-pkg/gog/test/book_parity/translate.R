# translate.R — one gog sentence, from the book's R spelling to JavaScript's
#
# The counterpart of `py-pkg/gog/tests/book_parity/translate.py`, and written in
# a different language on purpose. Python's translation is a handful of local
# rewrites, which regular expressions do correctly. This one is not: JavaScript
# cannot overload `+`, `*`, `|` or `/` (spec §8), so the sentence has to be
# *reassociated* rather than rewritten in place — and that needs the tree. R's
# own parser supplies it, which means the precedence here is R's actual
# precedence rather than a regex that agrees with it until it doesn't.
#
#   `+`  becomes the comma       data(gm) + point    plot(data(gm), point)
#   `*`  becomes layer(...)      bar * bin           layer(bar, bin)
#   `|`  becomes across(...)     | facet(era)        across(col.era)
#   `/`  becomes down(...)       / facet(era)        down(col.era)
#
# A column is `col.<name>` — the same mandatory accessor Python has, for the
# same reason: JavaScript has no bare names either, and in this grammar a string
# is how a *value* is spelled (spec §8, "JavaScript's surface"). A **named**
# argument joins one trailing options object, the only shape JavaScript has for
# `name = value`; positional arguments stay positional.
#
# Anything the emitter does not recognize is recorded as a **gap** and the
# sentence is dropped from the count rather than guessed at. A coverage number
# produced by guessing is a lie, which is the silent drop this project refuses
# everywhere else.
#
# Two callers, and this file is deliberately definitions only so both can have
# it: `run.mjs` (the parity harness) and `book/R/tabs.R` (the manual's third
# language tab). One emitter, so a tab cannot show syntax the harness never ran.

# Which argument of which atom names a column — the same split `translate.py`
# makes, restated so the two translators cannot disagree about what a column is.
# `colour` is exported only to be refused, but it takes its argument the way
# `color` does, so the accessor must still be added or the sentence fails on the
# *column* name and never reaches the refusal being compared.
JS_COLUMN_ATOMS <- c("x", "y", "z", "color", "size", "shape", "opacity",
                     "label", "pattern", "group", "facet", "order", "play", "brush",
                     "colour")
JS_ALL_COLUMN_ARGS <- c("bounds", "partition")
JS_PAIR_KEYWORDS <- c("lower", "upper", "start", "end")

js_gaps <- new.env(parent = emptyenv())
js_note_gap <- function(what) assign(what, TRUE, envir = js_gaps)
js_reset_gaps <- function() rm(list = ls(js_gaps), envir = js_gaps)
js_seen_gaps <- function() ls(js_gaps)

js_quote <- function(s)
  paste0('"', gsub('"', '\\\\"', gsub("\\\\", "\\\\\\\\", s)), '"')

# `col.gdp`, or `col["life exp"]` when the name is not a JavaScript identifier.
# The escape hatch is Law 8: enforce well-formedness hard, never forbid the
# ugly-but-legal.
#
# A JavaScript identifier is **Unicode**, not ASCII — `col.지역` is legal, and the
# manual has a chapter that names its columns in Hangeul. Testing for ASCII would
# send every one of them through the escape hatch, which reads as though the name
# were malformed when it is simply not English.
js_column <- function(name) {
  if (grepl("^[\\p{L}_$][\\p{L}\\p{N}_$]*$", name, perl = TRUE)) paste0("col.", name)
  else paste0("col[", js_quote(name), "]")
}

# ---- values -------------------------------------------------------------

js_value <- function(v) {
  if (is.character(v)) return(js_quote(v))
  if (is.logical(v))   return(if (is.na(v)) "null" else if (v) "true" else "false")
  if (is.numeric(v))   return(as.character(v))
  if (is.null(v))      return("null")
  if (is.name(v))      return(as.character(v))          # a bare identifier value
  if (is.call(v)) {
    fn <- deparse(v[[1]])
    parts <- as.list(v)[-1]
    # A refusal has to survive its container. `NA_character_` is how every
    # branch here says "I cannot spell this", but `paste()` renders NA as the
    # two characters `NA`, which JavaScript has no word for — so an element this
    # emitter refused came out the other side as a literal. `data.frame` below
    # checks its columns; `c` did not, and
    # `c(target_band$lower, target_band$upper)` in `marks/zone.qmd` reached the
    # book as `{ sales: [NA, NA] }`: a tab that renders, reports success, and
    # cannot run. Declining is the correct answer, and it has to be returned.
    if (fn == "c") {
      items <- vapply(parts, js_value, "")
      if (any(is.na(items))) return(NA_character_)
      return(paste0("[", paste(items, collapse = ", "), "]"))
    }
    if (fn == "-" && length(parts) == 1)
      return(paste0("-", js_value(parts[[1]])))
    if (fn == "exp" && length(parts) == 1 && identical(parts[[1]], 1))
      return("Math.E")
    if (fn == "data.frame") {
      nms <- names(parts)
      if (is.null(nms) || any(!nzchar(nms))) {
        js_note_gap("data.frame with unnamed columns"); return(NA_character_)
      }
      cells <- vapply(seq_along(parts), function(i) {
        inner <- js_value(parts[[i]])
        if (is.na(inner)) return(NA_character_)
        # A column is an array at every length — the wire invariant a one-row
        # frame broke once already (`df_to_wire`, and the `I()` that fixed it).
        if (!startsWith(inner, "[")) inner <- paste0("[", inner, "]")
        paste0(nms[i], ": ", inner)
      }, "")
      if (any(is.na(cells))) return(NA_character_)
      return(paste0("{ ", paste(cells, collapse = ", "), " }"))
    }
    js_note_gap(paste0("value call `", fn, "()`"))
    return(NA_character_)
  }
  js_note_gap(paste0("value of type ", class(v)[1]))
  NA_character_
}

# ---- atoms --------------------------------------------------------------

# `x(gdp, scale = "log")` becomes `x(col.gdp, { scale: "log" })`. A positional
# column argument becomes `col.<name>`; every *named* argument joins one
# trailing options object.
js_atom <- function(e) {
  if (is.name(e)) return(as.character(e))               # a bare mark: point, bar
  if (!is.call(e)) {
    js_note_gap(paste0("bare ", class(e)[1], " where an atom belongs"))
    return(NA_character_)
  }

  fn <- deparse(e[[1]])
  parts <- as.list(e)[-1]
  nms <- names(parts); if (is.null(nms)) nms <- rep("", length(parts))

  every <- fn %in% JS_ALL_COLUMN_ARGS
  positional <- character(); options <- character(); seen <- 0L

  for (i in seq_along(parts)) {
    arg <- parts[[i]]; nm <- nms[i]
    if (nzchar(nm)) {
      if ((every || nm %in% JS_PAIR_KEYWORDS) && is.name(arg)) {
        options <- c(options, paste0(nm, ": ", js_column(as.character(arg))))
      } else {
        v <- js_value(arg); if (is.na(v)) return(NA_character_)
        options <- c(options, paste0(nm, ": ", v))
      }
    } else {
      seen <- seen + 1L
      is_column <- (every || (seen == 1L && fn %in% JS_COLUMN_ATOMS)) && is.name(arg)
      if (is_column) {
        positional <- c(positional, js_column(as.character(arg)))
      } else {
        v <- js_value(arg); if (is.na(v)) return(NA_character_)
        positional <- c(positional, v)
      }
    }
  }

  if (length(options))
    positional <- c(positional, paste0("{ ", paste(options, collapse = ", "), " }"))
  paste0(fn, "(", paste(positional, collapse = ", "), ")")
}

# ---- the operator tree --------------------------------------------------

# A `*` chain collects into one `layer(...)`. The tighter binding shows up as
# nesting, which is a stronger signal than a precedence rule the reader has to
# already know.
js_collect_mult <- function(e) {
  if (is.call(e) && identical(deparse(e[[1]]), "*"))
    return(c(js_collect_mult(e[[2]]), js_collect_mult(e[[3]])))
  list(e)
}

# The facet side of `|` and `/`. `a | facet(x) / facet(y)` parses with `/`
# tighter, so the right operand of `|` can itself be a `/` — the crossed grid.
js_facets <- function(e, direction) {
  if (is.call(e)) {
    fn <- deparse(e[[1]])
    if (fn == "/")
      return(c(js_facets(e[[2]], direction), js_facets(e[[3]], "down")))
    if (fn == "facet") {
      inner <- as.list(e)[-1]
      named <- names(inner)
      if (is.null(named)) named <- rep("", length(inner))
      # The column is the argument written bare; `wrap` is the only setting
      # `facet()` takes, and JavaScript spells a setting as a trailing object.
      bare <- inner[named == ""]
      wrap <- if ("wrap" %in% named) inner[[which(named == "wrap")]] else NULL
      if (length(bare) == 1 && is.name(bare[[1]])) {
        column <- js_column(as.character(bare[[1]]))
        if (is.null(wrap))
          return(paste0(direction, "(", column, ")"))
        return(paste0(direction, "(", column, ", { wrap: ", deparse(wrap), " })"))
      }
    }
  }
  js_flatten(e)
}

# ---- wrapping ------------------------------------------------------------
#
# JavaScript spells the four operators as four words, so a sentence arrives as
# one nested call however the manual's R was laid out, and nothing put the line
# breaks back. Measured across the rendered book before this existed: 388 of 466
# JavaScript blocks were wider than 80 columns and the widest was 523, against
# zero R blocks over 96. Wide blocks scroll sideways in the HTML, so a reader
# saw the half that fitted.
#
# The break points are the commas, because a newline after a comma inside a call
# is always valid JavaScript, whatever the nesting. `run.mjs` compiles this text
# and compares the drawing with R's, so a break in a place JavaScript will not
# take fails loudly rather than quietly changing a plot.
js_break_points <- function(text) {
  chars <- strsplit(text, "", fixed = TRUE)[[1]]
  pos <- integer(0)
  depth <- integer(0)
  here <- 0L
  in_str <- FALSE
  quote_ch <- ""
  esc <- FALSE
  for (i in seq_along(chars)) {
    ch <- chars[[i]]
    if (esc) { esc <- FALSE; next }
    if (in_str) {
      if (ch == "\\") esc <- TRUE else if (ch == quote_ch) in_str <- FALSE
      next
    }
    if (ch %in% c("\"", "'", "`")) { in_str <- TRUE; quote_ch <- ch; next }
    if (ch %in% c("(", "[", "{")) { here <- here + 1L; next }
    if (ch %in% c(")", "]", "}")) { here <- here - 1L; next }
    if (ch == ",") { pos <- c(pos, i); depth <- c(depth, here) }
  }
  list(pos = pos, depth = depth)
}

# Of the breaks that fit, take the **shallowest**, and the furthest of those, so
# a page breaks between its plots before a plot breaks between its atoms. If
# nothing fits, the first break past the edge beats a line nothing can break.
js_wrap <- function(text, width = 74L, indent = "  ") {
  pts <- js_break_points(text)
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

js_flatten <- function(e) {
  if (is.call(e)) {
    fn <- deparse(e[[1]])
    if (fn == "+") return(c(js_flatten(e[[2]]), js_flatten(e[[3]])))
    if (fn == "|") return(c(js_flatten(e[[2]]), js_facets(e[[3]], "across")))
    if (fn == "/") return(c(js_flatten(e[[2]]), js_facets(e[[3]], "down")))
    if (fn == "*") {
      pieces <- vapply(js_collect_mult(e), js_atom, "")
      if (any(is.na(pieces))) return(NA_character_)
      return(paste0("layer(", paste(pieces, collapse = ", "), ")"))
    }
    if (fn == "(") return(js_flatten(e[[2]]))
  }
  js_atom(e)
}

# Is this the right-hand side of a *facet* operator rather than a composition?
#
# `plot | facet(g)` splits one plot; `plot | plot` arranges two. R tells them
# apart by the operand's type at run time and this has only the source text, so
# it asks the one question the text can answer: does the right side name
# `facet()` — on its own, or as the `facet(a) / facet(b)` pair the crossed grid
# is written with.
js_is_facet <- function(e) {
  if (!is.call(e)) return(FALSE)
  fn <- deparse(e[[1]])
  if (fn == "facet") return(TRUE)
  if (fn == "(") return(js_is_facet(e[[2]]))
  if (fn %in% c("|", "/") && length(e) == 3) return(js_is_facet(e[[3]]))
  FALSE
}

# Is this expression a *composition* — `|` or `/` between two plots?
js_composition <- function(e) {
  if (!is.call(e)) return(FALSE)
  fn <- deparse(e[[1]])
  if (fn == "(") return(js_composition(e[[2]]))
  fn %in% c("|", "/") && length(e) == 3 && !js_is_facet(e[[3]])
}

# Does a page appear where R can put one and JavaScript cannot?
#
# R's refusals for a page — `(a | b) + title(…)`, `(a | b) | facet(g)` — are
# about *operators applied to a page*, and this binding has neither: a page is a
# value returned by `beside()`/`below()`, so there is no `+` to misapply and no
# `facet()` to mis-join. Those sentences are declined the way the R chapter's
# pipes are, rather than mistranslated into a `plot(…)` that draws.
js_page_misused <- function(e) {
  if (!is.call(e)) return(FALSE)
  fn <- deparse(e[[1]])
  if (fn == "+" && length(e) == 3 &&
      (js_has_composition(e[[2]]) || js_has_composition(e[[3]]))) return(TRUE)
  if (fn %in% c("|", "/") && length(e) == 3 && js_is_facet(e[[3]]) &&
      js_has_composition(e[[2]])) return(TRUE)
  any(vapply(as.list(e)[-1], js_page_misused, logical(1)))
}

# A parenthesized group used as an operand of `+`.
#
# R refuses this shape: parentheses do not group marks, so everything inside them
# would be dropped. **JavaScript never had the shape to refuse.** Its sentence is
# a flat argument list, `plot(a, b, c)`, so the parentheses have nothing to
# translate into and simply vanish — leaving a different sentence, which is
# legal, which then draws. That is not a disagreement between the bindings; it is
# a sentence with no JavaScript form, the same category as an R pipe.
#
# Parentheses around whole *plots* are the opposite case and must keep
# translating: `(…) | (…)` becomes `across(plot(…), plot(…))`, and composition is
# exactly what parentheses are for. So the test is the operator they sit under,
# `+` and not `|` or `/`, rather than the parentheses themselves.
js_plus_group <- function(e) {
  if (!is.call(e)) return(FALSE)
  if (identical(deparse(e[[1]]), "+") && length(e) == 3) {
    for (side in list(e[[2]], e[[3]])) {
      if (is.call(side) && identical(deparse(side[[1]]), "(")) return(TRUE)
    }
  }
  any(vapply(as.list(e)[-1], js_plus_group, logical(1)))
}

js_has_composition <- function(e) {
  if (!is.call(e)) return(FALSE)
  if (js_composition(e)) return(TRUE)
  any(vapply(as.list(e)[-1], js_has_composition, logical(1)))
}

# One R sentence → one JavaScript sentence, `plot(…)` or a page of them.
#
# `beside()`/`below()` are what this binding spells `|`/`/` between two plots
# with, exactly as `across()`/`down()` spell them between a plot and a facet.
js_sentence <- function(expr) {
  if (is.call(expr) && deparse(expr[[1]]) == "(") return(js_sentence(expr[[2]]))
  if (is.call(expr) && deparse(expr[[1]]) %in% c("|", "/") && length(expr) == 3 &&
      !js_is_facet(expr[[3]])) {
    word <- if (deparse(expr[[1]]) == "|") "beside" else "below"
    left <- js_sentence(expr[[2]])
    right <- js_sentence(expr[[3]])
    if (is.na(left) || is.na(right)) return(NA_character_)
    return(paste0(word, "(", left, ", ", right, ")"))
  }
  items <- js_flatten(expr)
  if (any(is.na(items))) return(NA_character_)
  paste0("plot(", paste(items, collapse = ", "), ")")
}

# One R sentence → one JavaScript sentence.
#
# Returns `list(js =, blocked =)`. `blocked` distinguishes the two ways a
# translation can be absent: a sentence *about the host language* (the R
# chapter's pipes, which should have no JavaScript spelling) from a sentence the
# emitter could not handle, which is a defect and reports `js = NA` with the gap
# recorded.
translate_js <- function(source) {
  # The R pipes are a host-language idiom the R chapter exists to document;
  # JavaScript has none to answer them with, so they are declined rather than
  # mangled — exactly what `translate.py` does with the same seven sentences.
  if (grepl("\\|>", source) || grepl("%>%", source))
    return(list(js = NA_character_, blocked = "R pipe — a host-language idiom"))

  # `save_gif()` names a file, and naming a file is host bookkeeping rather than
  # grammar: R writes `file.path(tempdir(), …)` and JavaScript spells the same
  # thing its own way. The two the book documents also bind their plot on the
  # line above, so what the extractor records is one line short of standing
  # alone. Declining costs no coverage — a parity run compares the picture a
  # sentence draws, and this call writes a file instead of returning one.
  if (grepl("\\bsave_gif[[:space:]]*\\(", source))
    return(list(js = NA_character_,
                blocked = "save_gif names a file — host bookkeeping, not a sentence"))

  # R's own extractor on a table, as in `df[order(df$pop), ]`, which the R
  # chapter's masked-names section documents. That is host arithmetic rather than
  # a sentence, so JavaScript has nothing to spell it with. Matched as syntax (a
  # name followed by `[`) and not by the characters, because `$` also appears
  # inside a legitimate title string that must keep translating.
  if (grepl("[A-Za-z0-9._)\"][[:space:]]*\\[", source))
    return(list(js = NA_character_, blocked = "R subsetting — a host-language idiom"))

  parsed <- tryCatch(parse(text = source), error = function(e) NULL)
  if (is.null(parsed) || !length(parsed))
    return(list(js = NA_character_, blocked = "did not parse as R"))

  out <- character()
  for (expr in as.list(parsed)) {
    if (js_page_misused(expr))
      return(list(js = NA_character_,
                  blocked = "an operator applied to a page — JavaScript has no `+` or `facet()` to apply"))
    if (js_plus_group(expr))
      return(list(js = NA_character_,
                  blocked = "parentheses grouping marks — JavaScript's argument list has no parentheses to drop"))
    # A table the chunk defines for itself keeps its *name*, because the sentence
    # below is about to use it. Only the spec assignment (`p <- data(…) + …`) is
    # droppable, where the name is R's bookkeeping rather than content.
    # `js_value()` already refuses a table built by R computation. That refusal
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
      literal <- js_value(expr[[3]])
      if (is.na(literal))
        return(list(js = NA_character_,
                    blocked = "table computed in R, not written out as literal columns"))
      out <- c(out, paste0("const ", deparse(expr[[2]]), " = ", literal, ";"))
      next
    }
    if (is.call(expr) && deparse(expr[[1]]) %in% c("<-", "=")) expr <- expr[[3]]
    # `render_svg(<sentence>)` is the book's `error: true` idiom — the host's
    # render call, which every binding spells its own way. Not an atom, and not
    # part of the surface.
    if (is.call(expr) && deparse(expr[[1]]) == "render_svg" && length(expr) == 2)
      expr <- expr[[2]]
    sentence <- js_sentence(expr)
    if (is.na(sentence)) return(list(js = NA_character_, blocked = NA_character_))
    out <- c(out, js_wrap(sentence))
  }
  list(js = paste(out, collapse = "\n"), blocked = NA_character_)
}

# Every sentence in the corpus, translated, written where a caller asked for it.
# `run.mjs` calls this; nothing runs on `source()`.
write_js_translations <- function(corpus, out_path) {
  sentences <- jsonlite::fromJSON(file.path(corpus, "sentences.json"),
                                  simplifyDataFrame = FALSE)
  js_reset_gaps()
  results <- lapply(sentences, function(s) {
    got <- translate_js(s$source)
    list(id = s$id,
         js = if (is.na(got$js)) NULL else got$js,
         blocked = if (is.na(got$blocked)) NULL else got$blocked)
  })
  jsonlite::write_json(results, out_path, auto_unbox = TRUE, null = "null",
                       pretty = FALSE)
  invisible(list(total = length(results), gaps = js_seen_gaps()))
}
