# book/check_glosses.R
# Every plot that says something new says it in English too.
#
# Under a plot the book prints the sentence read aloud, in italic:
#
#   *"Given gapminder Asia: lines, x is year, y is life, color by country."*
#
# It is the approachability mechanism (spec §20) and an honest test of the
# grammar: a specification that cannot be read aloud cleanly is a grammar bug.
# The rule for where one goes is per chapter. The chapter's first plot carries
# one, and so does every later plot that uses an element no earlier plot in the
# chapter used: a mark, a channel, a transform, a space, a facet or composition
# operator, a scale option. A plot that only changes a column, a table, a title
# or the orientation repeats a sentence the reader has already read, so it
# carries none. A refusal draws nothing and carries none.
#
# The rule held for 400 plots on the day it was written down, and it will drift
# the way every prose rule here has drifted: a new plot lands with a new channel
# and no sentence, the render is clean, and nobody notices for a session. So it
# is checked at the source, like the titles and the refusals. This guard reads
# each chapter's chunks in order, keeps the set of elements seen so far, and
# fails on a plot that introduces one without a sentence directly after it. It
# also checks the sentence's shape, since the whole point is that every one
# reads the same way: it starts *"Given <table>: and ends with a period inside
# the quotes; a composition of plots on one table names it once.
#
# Elements are atoms and, since every argument has a spoken form, the named
# arguments and the positional number a transform takes: `bin(20)` after a bare
# `bin` is new, and so is `speed =` after a bare `play`. A value change is not:
# `bin(30)` after `bin(20)` repeats the sentence. The guard requires a
# sentence; it never forbids one.
#
# **One rule of the sentence's grammar is checked against the code: number.**
# A continuous mark takes an article, "a line", and turns plural once a channel
# splits it, "lines". What splits it is `group` on any
# column, or `color` or `pattern` on a column of categories, written before the
# marks, where it reaches every layer, or after this mark. A sentence that says
# "a line" over five drawn lines teaches the reader that where a channel is
# written does not matter, which is the one thing `encoding-scope.qmd` exists to
# say otherwise, and that is where it happened: "points and also a line derived
# by smooth" under five trends. The column's type is read from the book's shared
# tables; a table the chapter builds itself is skipped rather than guessed. Only
# this direction is checked: a density over a categorical position also draws one
# shape per category, a violin, and the code alone cannot tell that from one
# stroke, so a plural is never refused.
#
# **A second rule is checked the same way: scope.** In a plot of two or more
# layers, a channel written after one layer's mark belongs to that layer alone,
# and the sentence says so with a participle: "points colored by continent",
# "points sized by population", against the plot-wide "color by continent". The
# two forms are how a reader hears where the channel was written, which is the
# one thing that decides what it reaches. On 2026-10-03, 25 sentences used the
# participle and 18 did not, in 12 chapters, so the rule was being followed
# chapter by chapter rather than kept. `color`, `size`, `pattern` and `shape`
# are checked. `shape` reads "shaped by" (ruled 2026-10-03): "shaped by" also
# means *influenced by*, but after a mark's name, "points shaped by species",
# it can only mean the drawn shape, and one pattern for the four channels is one
# rule for a translator to learn. `label` reads "label by" everywhere, after the
# `text` mark it always follows.

check_glosses <- function(book_dir = "book") {
  qmds <- list.files(book_dir, pattern = "[.]qmd$", recursive = TRUE, full.names = TRUE)
  qmds <- qmds[!grepl("/_book/|/parts/", qmds)]

  marks <- c("point", "line", "bar", "area", "step", "interval", "box", "ribbon",
             "text", "path", "rule", "zone", "surface", "edge")
  channels <- c("x", "y", "z", "color", "size", "shape", "opacity", "pattern",
                "label", "group", "order", "play", "brush", "palette")
  transforms <- c("bin", "count", "proportion", "density", "smooth", "mean",
                  "median", "sum", "max", "min", "range", "deviation", "quantile",
                  "confidence", "stack", "dodge", "jitter", "repel", "bounds",
                  "layout", "flow", "cluster", "partition")
  spaces <- c("polar", "map", "globe", "network", "nest", "space")
  # Chunks whose output is text, a table, or a file, never a plot on the page.
  text_only <- "^\\s*(py|jl|js)_(error|show)\\(|^\\s*(render_svg|save_gif|save_svg|print|cat|identical|peek|nprint|kable|mark_options|str|nrow|head)\\("

  missing <- character(0)
  shape <- character(0)
  number <- character(0)
  n_number <- 0L
  scope <- character(0)
  n_scope <- 0L
  participle <- c(color = "colored", size = "sized", pattern = "patterned",
                  shape = "shaped")

  # The shared tables, for the type of a column a channel names.
  tables <- new.env()
  sys.source(file.path(book_dir, "R", "data.R"), envir = tables)
  continuous <- list(line = c("a line", "lines"), area = c("an area", "areas"),
                     ribbon = c("a ribbon", "ribbons"),
                     step = c("a step outline", "step outlines"))
  # Transforms that draw a picture of their own, where one shape per row or per
  # flow is the mark's reading whatever splits it.
  picture <- c("flow", "layout", "cluster", "partition", "bounds")
  name_of <- function(e) if (is.name(e)) as.character(e) else if (is.call(e)) as.character(e[[1]])[1] else ""
  lead <- function(e) { while (is.call(e) && name_of(e) == "*") e <- e[[2]]; e }
  chain <- function(e) if (is.call(e) && name_of(e) == "*") c(chain(e[[2]]), chain(e[[3]])) else list(e)
  is_facet <- function(e) is.call(e) && (name_of(e) == "facet" ||
    (name_of(e) %in% c("|", "/", "+", "(") && any(vapply(as.list(e)[-1], is_facet, TRUE))))
  # The terms of one plot in the order written, or NULL for a page of plots.
  terms_of <- function(e) {
    if (is.call(e)) {
      fn <- name_of(e)
      if (fn == "+" && length(e) == 3) return(c(terms_of(e[[2]]), terms_of(e[[3]])))
      if (fn == "(") return(terms_of(e[[2]]))
      if (fn %in% c("|", "/") && length(e) == 3)
        return(if (is_facet(e[[3]])) terms_of(e[[2]]) else NULL)
    }
    list(e)
  }
  # Each continuous layer, in order, and whether a channel splits it: TRUE,
  # FALSE, or NA when a column's type cannot be read.
  splits_of <- function(sentence) {
    ts <- terms_of(sentence)
    if (is.null(ts)) return(list())
    plot_table <- NA; pending <- NA; plot_ch <- list(); layers <- list()
    for (t in ts) {
      h <- name_of(lead(t))
      if (h %in% c("data", "query")) {
        tb <- if (is.call(t) && length(t) >= 2 && is.name(t[[2]])) as.character(t[[2]]) else NA
        if (!length(layers) && is.na(plot_table)) plot_table <- tb else pending <- tb
      } else if (h %in% marks) {
        layers[[length(layers) + 1]] <- list(mark = h, ch = list(),
          transforms = vapply(chain(t)[-1], name_of, ""),
          table = if (!is.na(pending)) pending else plot_table)
        pending <- NA
      } else if (h %in% c("color", "group", "pattern") && is.call(t) && length(t) >= 2 &&
                 is.name(t[[2]])) {
        ch <- list(kind = h, col = as.character(t[[2]]))
        if (!length(layers)) plot_ch[[length(plot_ch) + 1]] <- ch
        else layers[[length(layers)]]$ch[[length(layers[[length(layers)]]$ch) + 1]] <- ch
      }
    }
    out <- list()
    for (L in layers) {
      if (!L$mark %in% names(continuous)) next
      split <- if (any(L$transforms %in% picture)) NA else FALSE
      for (ch in c(plot_ch, L$ch)) {
        if (is.na(split)) break
        if (ch$kind == "group") { split <- TRUE; next }
        df <- if (!is.na(L$table) && exists(L$table, envir = tables, inherits = FALSE))
          get(L$table, envir = tables) else NULL
        if (!is.data.frame(df) || !ch$col %in% names(df)) { split <- NA; break }
        v <- df[[ch$col]]
        if (is.character(v) || is.factor(v) || is.logical(v)) split <- TRUE
      }
      out[[length(out) + 1]] <- list(mark = L$mark, split = split)
    }
    out
  }
  # The channels of a plot of two or more layers that are written after a mark,
  # and so belong to that mark's layer alone.
  scoped_of <- function(sentence) {
    ts <- terms_of(sentence)
    if (is.null(ts)) return(list())
    heads <- vapply(ts, function(t) name_of(lead(t)), "")
    if (sum(heads %in% marks) < 2) return(list())
    out <- list()
    after_mark <- FALSE
    for (k in seq_along(ts)) {
      t <- ts[[k]]
      if (heads[k] %in% marks) { after_mark <- TRUE; next }
      if (after_mark && heads[k] %in% names(participle) && is.call(t) &&
          length(t) >= 2 && is.name(t[[2]]))
        out[[length(out) + 1]] <- list(kind = heads[k], col = as.character(t[[2]]))
    }
    out
  }
  n_plots <- 0L
  n_glossed <- 0L

  for (f in qmds) {
    lines <- readLines(f, warn = FALSE)
    opens <- which(grepl("^```\\{r", lines))
    seen <- character(0)
    first <- TRUE
    for (o in opens) {
      rest <- lines[(o + 1):length(lines)]
      k <- which(rest == "```")[1]
      if (is.na(k)) next
      cl <- o + k
      body <- if (cl - o > 1) lines[(o + 1):(cl - 1)] else character(0)
      opts <- body[grepl("^#\\|", body)]
      code <- body[!grepl("^#\\|", body)]
      code <- code[!grepl("^\\s*#", code)]
      code <- paste(trimws(code), collapse = " ")
      # The bindings chapters hide the R chunk and print the other language's
      # code themselves, so a hidden chunk that draws through the helpers counts.
      helper <- any(grepl("^\\s*(py|jl|js)_plot\\(", body))
      if (any(grepl("include: false|eval: false|error: true", opts))) next
      if (any(grepl("echo: false", opts)) && !helper) next
      if (grepl(text_only, code)) next
      # render_svg(), save_gif() and save_svg() hand the picture to a string or a
      # file, and identical() prints a verdict; none of them puts a plot on the page.
      if (grepl("\\b(render_svg|save_gif|save_svg|identical)\\(", code)) next
      # The bindings chapters hand the sentence to a helper as a string.
      if (grepl("^\\s*(py|jl|js)_plot\\(", code)) {
        m <- regmatches(code, regexpr('"(\\\\.|[^"\\\\])*"', code))
        if (length(m)) code <- gsub('\\\\"', '"', substr(m, 2, nchar(m) - 1))
      }
      # Scale options are read before the strings go, since the value is a string.
      raw <- code
      code <- gsub('"(\\\\.|[^"\\\\])*"', '""', code)
      code <- gsub("'(\\\\.|[^'\\\\])*'", "''", code)
      # A column can be called `step` or `count`. Arguments are emptied before
      # marks are looked for, so only the bare word between operators counts,
      # and a transform counts only after `*` (or inside JavaScript's layer()).
      # plot(), layer(), beside() and below() are JavaScript's spelling of the
      # operators, and a mark sits inside them, so those four keep their arguments.
      bare <- code
      for (i in 1:3)
        bare <- gsub("\\b(?!plot\\(|layer\\(|beside\\(|below\\()([a-z_.]+)\\(([^()]*)\\)", "\\1()", bare, perl = TRUE)
      has_mark <- grepl(sprintf("\\b(%s)\\b", paste(marks, collapse = "|")), bare)
      has_table <- grepl("\\b(data|query)\\(", code)
      if (!(has_mark && has_table)) next

      n_plots <- n_plots + 1L
      el <- character(0)
      for (w in marks) if (grepl(sprintf("\\b%s\\b", w), bare)) el <- c(el, w)
      for (w in c(channels, spaces))
        if (grepl(sprintf("(?<![:.$\\w])%s\\(", w), code, perl = TRUE)) el <- c(el, w)
      for (w in transforms)
        if (grepl(sprintf("\\*\\s*%s\\b|layer\\([^)]*\\b%s\\b", w, w), code)) el <- c(el, w)
      if (grepl("\\|\\s*facet\\(|\\bacross\\(", code)) el <- c(el, "facet columns")
      if (grepl("/\\s*facet\\(|\\bdown\\(", code)) el <- c(el, "facet rows")
      if (grepl("\\bwrap\\s*=", code)) el <- c(el, "wrap")
      if (grepl("\\)\\s*\\|\\s*\\(|\\|\\s*\\(|\\bbeside\\(", code)) el <- c(el, "beside")
      if (grepl("\\)\\s*/\\s*\\(|/\\s*\\(|\\bbelow\\(", code)) el <- c(el, "above")
      if (grepl('scale\\s*=\\s*"log"|scale:\\s*\'log\'', raw)) el <- c(el, "log scale")
      if (grepl('scale\\s*=\\s*"category"', raw)) el <- c(el, "category scale")
      if (grepl("\\blimits\\s*=", raw)) el <- c(el, "limits")
      if (grepl("\\bfree\\s*=\\s*TRUE|free:\\s*true", raw)) el <- c(el, "free")
      if (grepl("\\baxis\\s*=\\s*(FALSE|false)|axis:\\s*false", raw)) el <- c(el, "axis left out")
      if (lengths(regmatches(code, gregexpr("\\b(data|query)\\(", code))) > 1) el <- c(el, "second table")
      # Named arguments with a spoken form, and the positional number a
      # transform takes (bin(20), density(2), jitter(0.5), range(0.1, 0.9)).
      args <- c("width", "bandwidth", "adjust", "compare", "reach", "baseline",
                "speed", "whiskers", "base", "tick_count", "preserve", "turn",
                "tilt", "levels", "tiling", "share", "cross", "start", "at")
      # Only inside a gog call: `factor(levels = )` and `file.path()` are R.
      atoms <- paste(c(marks, channels, transforms, spaces, "facet"), collapse = "|")
      for (a in args)
        if (grepl(sprintf("\\b(%s)\\([^()]*\\b%s\\s*[=:]", atoms, a), raw)) el <- c(el, paste0("arg ", a))
      for (t in c("bin", "density", "jitter", "range", "confidence", "quantile"))
        if (grepl(sprintf("\\b%s\\(\\s*[0-9.]", t), raw)) el <- c(el, paste0("arg ", t, " number"))

      new <- setdiff(el, seen)
      need <- first || length(new) > 0
      # The sentence, if any, is the first non-blank line after the chunk.
      j <- cl + 1
      while (j <= length(lines) && !nzchar(trimws(lines[j]))) j <- j + 1
      # The first chapter introduces the convention with "Read it aloud:" on
      # the same line, which is the one lead-in a sentence may have.
      present <- j <= length(lines) && grepl('^\\*"|^[A-Z][^*]{0,40}: \\*"', lines[j])
      where <- sprintf("  %s:%d", sub("^.*book/", "", f), o)
      if (present) {
        n_glossed <- n_glossed + 1L
        e <- j
        while (e <= length(lines) && !grepl('"\\*', lines[e])) e <- e + 1
        text <- paste(lines[j:min(e, length(lines))], collapse = " ")
        text <- sub('^[^*]*(\\*")', "\\1", text)
        text <- sub('"\\*.*$', '"*', text)
        # Every sentence names its table first. A composition of plots on one
        # table says it once and continues "beside the same"; the one fragment
        # allowed is the first chapter's "…and color by continent." which
        # deliberately extends the sentence just read.
        ok <- grepl('^\\*"(Given |…)', text) && grepl('\\."\\*$', text)
        if (!ok) shape <- c(shape, sprintf("%s  %s", where, substr(text, 1, 90)))
        # Number: a continuous mark a channel splits is spoken in the plural.
        if (!helper) {
          exprs <- tryCatch(parse(text = body[!grepl("^#\\|", body)]), error = function(e) NULL)
          layers <- if (length(exprs)) tryCatch(splits_of(exprs[[length(exprs)]]),
                                                error = function(e) list()) else list()
          count <- c(line = 0L, area = 0L, ribbon = 0L, step = 0L)
          for (L in layers) {
            count[L$mark] <- count[L$mark] + 1L
            if (!isTRUE(L$split)) next
            n_number <- n_number + 1L
            forms <- continuous[[L$mark]]
            at <- function(form) {
              m <- gregexpr(sprintf("\\b%s\\b", form), text)[[1]]
              as.integer(m[m > 0])
            }
            said <- rbind(data.frame(at = at(forms[1]), plural = rep(FALSE, length(at(forms[1])))),
                          data.frame(at = at(forms[2]), plural = rep(TRUE, length(at(forms[2])))))
            said <- said[base::order(said$at), , drop = FALSE]  # gog's own order() is attached
            if (nrow(said) >= count[L$mark] && !said$plural[count[L$mark]])
              number <- c(number, sprintf("%s  a %s split by a channel, spoken as \"%s\"",
                                          where, L$mark, forms[1]))
          }
          # Scope: a channel that belongs to one layer is spoken as a participle.
          scoped <- if (length(exprs)) tryCatch(scoped_of(exprs[[length(exprs)]]),
                                                error = function(e) list()) else list()
          for (sc in scoped) {
            n_scope <- n_scope + 1L
            want <- sprintf("%s by %s", participle[[sc$kind]], gsub("_", " ", sc$col))
            if (!grepl(want, text, fixed = TRUE))
              scope <- c(scope, sprintf("%s  `%s(%s)` belongs to one layer: say \"%s\"",
                                        where, sc$kind, sc$col, want))
          }
        }
      } else if (need) {
        why <- if (first) "the chapter's first plot" else paste("new:", paste(new, collapse = ", "))
        missing <- c(missing, sprintf("%s  %s", where, why))
      }
      seen <- union(seen, el)
      first <- FALSE
    }
  }

  if (length(number)) {
    cat("FAIL: a continuous mark a channel splits is spoken in the singular\n")
    cat(paste(number, collapse = "\n"), "\n")
    cat("  A color or pattern of categories, or any group, written before the marks or\n")
    cat("  after this one splits it: \"lines\", \"areas\", \"ribbons\", \"step outlines\".\n")
  }
  if (length(scope)) {
    cat("FAIL: a channel that belongs to one layer is spoken as if it reached the plot\n")
    cat(paste(scope, collapse = "\n"), "\n")
    cat("  Written after a mark, in a plot of two or more layers, it reaches that layer\n")
    cat("  alone: \"points colored by continent\", \"points sized by population\".\n")
  }
  if (length(missing) || length(shape) || length(number) || length(scope)) {
    if (length(missing)) {
      cat("FAIL: a plot introduces an element and has no read-aloud sentence after it\n")
      cat(paste(missing, collapse = "\n"), "\n")
      cat("  Put the sentence directly after the chunk, in the book's form:\n")
      cat("  *\"Given gapminder 2007: points, x is gdp, y is life, color by continent.\"*\n")
    }
    if (length(shape)) {
      cat("FAIL: a read-aloud sentence does not have the book's shape\n")
      cat(paste(shape, collapse = "\n"), "\n")
      cat("  It starts *\"Given <table>: and ends with a period inside the quotes.\n")
    }
    stop("check_glosses: ", length(missing), " plot(s) without a sentence, ",
         length(scope), " layer channel(s) spoken for the plot, ",
         length(shape), " sentence(s) off shape, ", length(number),
         " split mark(s) spoken in the singular")
  }
  cat("PASS: every plot that introduces an element carries its sentence (",
      n_glossed, "sentences under", n_plots, "plots;", n_number,
      "split marks spoken in the plural;", n_scope,
      "layer channels spoken as participles )\n")
  invisible(TRUE)
}
