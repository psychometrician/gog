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
  text_only <- "^\\s*(py|jl|js)_(error|show)\\(|^\\s*(render_svg|save_gif|print|cat|identical|peek|nprint|kable|mark_options|str|nrow|head)\\("

  missing <- character(0)
  shape <- character(0)
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
      # render_svg() and save_gif() hand the picture to a string or a file, and
      # identical() prints a verdict; none of them puts a plot on the page.
      if (grepl("\\b(render_svg|save_gif|identical)\\(", code)) next
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
      } else if (need) {
        why <- if (first) "the chapter's first plot" else paste("new:", paste(new, collapse = ", "))
        missing <- c(missing, sprintf("%s  %s", where, why))
      }
      seen <- union(seen, el)
      first <- FALSE
    }
  }

  if (length(missing) || length(shape)) {
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
         length(shape), " sentence(s) off shape")
  }
  cat("PASS: every plot that introduces an element carries its sentence (",
      n_glossed, "sentences under", n_plots, "plots )\n")
  invisible(TRUE)
}
