# Does the PDF filter escape exactly the way Pandoc escapes?
#
# `gog-syntax.lua` throws Pandoc's tokens away and writes the `Highlighting`
# block itself, so it has to escape LaTeX's special characters by hand. That is
# the one part of it that cannot be checked by reading: inside `Highlighting` the
# environment is a `Verbatim` with `commandchars=\\\{\}`, so strictly only three
# characters are special, and yet Pandoc escapes twelve and leaves `$` alone.
# A rule derived from what *should* be special would be wrong in both directions.
#
# So this does not reason about it. It renders the same code twice, once through
# Pandoc alone and once through the filter, strips every `\XxxTok{...}` and
# `\gogXxx{...}` wrapper off both, and compares what is left. If the two escaped
# strings are identical, the escaping is right by construction, whatever the rule
# turns out to be.
#
# The probes below are deliberately unpleasant: every character Pandoc treats
# specially, a column name with an underscore, a Windows path, a regular
# expression, and the three column spellings. Each one still has to be a gog
# specification, or the filter declines it and the comparison proves nothing,
# which is why every probe names a table.
#
# The tenth guard, and the only one that shells out. It needs `quarto` on PATH
# for the bundled pandoc, and skips rather than failing where there is none:
# a machine with no Quarto cannot build the PDF either, so there is nothing
# there for this to be wrong about.
#
# Sourced by `r-pkg/gog/tests/test_basic.R`, which runs from the repo root, so
# it takes the book's directory the way its nine siblings do.

check_latex_escaping <- function(book = "book") {
  fail <- function(...) stop("gog: ", ..., call. = FALSE)

  if (!nzchar(Sys.which("quarto"))) {
    cat("SKIP: quarto is not on PATH - the LaTeX escaping is unchecked\n")
    return(invisible(NULL))
  }
  here <- setwd(book)
  on.exit(setwd(here), add = TRUE)
  if (!file.exists("grammar.qmd") || !file.exists("gog-syntax.lua")) {
    fail("check_latex_escaping: no grammar.qmd or gog-syntax.lua in ", book)
  }

  probes <- c(
    'data(gapminder_2007) + point + x(gdp) + y(life) + color(continent)',
    'data(my_table) + bar * bin + x(life) + style(color = "#ff0000")',
    'data(t) + point + x(a) + style(pattern = "b\\\\c") + title("100% & $5")',
    'data(t) + point + x(a) + title("{braces} ^caret ~tilde <lt> >gt")',
    "data(t) + point + x(a) + title(\"'quoted' and \\\"double\\\"\")",
    'data(t) + zone * bin + x(a) + y(b) + style(border_color = "#00_ff")',
    'query(con, "SELECT count(*) FROM data WHERE x > 5 & y < 3") + point + x(a)',
    'data(t) + point + x(col.gdp) + y(col.life) + color(col.continent)',
    'data(t) + point + x(:gdp) + y(:life) + color(:continent)',
    'plot(data(t), layer(bar, bin), x(col.life), style({ color: "tomato" }))',
    'data(t) + point + x(a, scale = "log") + y_label("50% of $100 {net}")',
    'data(a_b_c) + text + x(a) + y(b) + label(name) + x_label("x_1 ~ x_2")',
    # Chunk output, which is what a refusal is. It carries no language, so the
    # filter must decline it.
    #
    # The message quotes a whole specification back at the reader, which is what
    # gog's diagnostics do: they say what to do, not only what went wrong. That
    # is deliberate here. An earlier version of this probe named atoms but no
    # table, so the *specification* gate declined it and the language gate was
    # never exercised: breaking the language test on purpose changed nothing and
    # the assertion below looked like it worked. Naming a table is what makes
    # this probe about the one gate it claims to be about.
    'gog: bin needs a continuous column, and continent is a category.\n  Use data(gapminder_2007) + bar * count + x(continent) instead.'
  )

  languages <- c("r", "r", "r", "r", "r", "r", "r", "python", "julia", "js",
                 "r", "r", "")

  # One document, so this is two pandoc invocations rather than twenty-four.
  work <- tempfile("gogtex"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)

  body <- unlist(Map(function(code, lang) {
    c(paste0("```", lang), code, "```", "")
  }, probes, languages), use.names = FALSE)
  writeLines(body, file.path(work, "probe.md"))

  latex_of <- function(with_filter) {
    # Quarto's own pandoc, not whichever one is on PATH. The escaping is the
    # thing under test, and two pandoc builds are two answers to it; the book is
    # rendered by this one.
    args <- c("quarto", "pandoc", shQuote(file.path(work, "probe.md")),
              "--to", "latex")
    if (with_filter) {
      args <- c(args, "--lua-filter", shQuote(normalizePath("gog-syntax.lua")))
    }
    out <- suppressWarnings(system(paste(args, collapse = " "),
                                   intern = TRUE, ignore.stderr = FALSE))
    if (!is.null(attr(out, "status")) && attr(out, "status") != 0) {
      fail("pandoc failed on the probe document")
    }
    out
  }

  # The filter reads `grammar.qmd` from the working directory, and pandoc is
  # invoked from here, so the kernel is found the same way a render finds it.
  plain <- latex_of(FALSE)
  painted <- latex_of(TRUE)

  # Everything between the environment's begin and end, one block at a time.
  blocks_of <- function(lines) {
    starts <- grep("^\\\\begin\\{Highlighting\\}", lines)
    ends <- grep("^\\\\end\\{Highlighting\\}", lines)
    if (length(starts) != length(ends)) fail("unbalanced Highlighting blocks")
    Map(function(a, b) lines[(a + 1):(b - 1)], starts, ends)
  }

  # `\FunctionTok{data}` and `\gogTable{data}` both become `data`, so that what
  # is left is the escaped text alone and the two sides can be compared.
  #
  # This counts braces rather than matching a pattern, and the difference is not
  # academic. A token's content is escaped text, and three of the escapes carry
  # braces of their own: `\textbackslash{}`, `\textgreater{}`, `\^{}`. A regular
  # expression that stops at the first `}` peels those apart and reports a
  # mismatch on both sides of a comparison that actually agreed. Two of these
  # probes were written to produce exactly that, and the first version of this
  # function failed all six of the ones with a brace in them.
  #
  # `\{` and `\}` are literal text and must not count toward the depth, which is
  # the other half of the same trap.
  strip <- function(line) {
    chars <- strsplit(line, "", fixed = TRUE)[[1]]
    n <- length(chars)
    out <- character(0)
    i <- 1L
    while (i <= n) {
      ch <- chars[i]
      if (ch != "\\" || i == n) {
        out <- c(out, ch); i <- i + 1L; next
      }
      if (chars[i + 1L] %in% c("{", "}")) {          # an escaped brace: text
        out <- c(out, ch, chars[i + 1L]); i <- i + 2L; next
      }
      j <- i + 1L
      while (j <= n && grepl("^[A-Za-z]$", chars[j])) j <- j + 1L
      name <- paste(chars[seq.int(i + 1L, j - 1L)], collapse = "")
      wrapper <- nzchar(name) && j <= n && chars[j] == "{" &&
        (grepl("Tok$", name) || grepl("^gog", name))
      if (!wrapper) {
        out <- c(out, ch); i <- i + 1L; next
      }
      depth <- 1L
      k <- j + 1L
      inner <- character(0)
      while (k <= n && depth > 0L) {
        c2 <- chars[k]
        if (c2 == "\\" && k < n && chars[k + 1L] %in% c("{", "}")) {
          inner <- c(inner, c2, chars[k + 1L]); k <- k + 2L; next
        }
        if (c2 == "{") depth <- depth + 1L
        if (c2 == "}") {
          depth <- depth - 1L
          if (depth == 0L) { k <- k + 1L; break }
        }
        inner <- c(inner, c2); k <- k + 1L
      }
      peeled <- strip(paste(inner, collapse = ""))
      out <- c(out, strsplit(peeled, "", fixed = TRUE)[[1]])
      i <- k
    }
    paste(out, collapse = "")
  }

  a <- blocks_of(plain)
  b <- blocks_of(painted)
  if (length(a) != length(b)) {
    fail("pandoc wrote ", length(a), " blocks and the filter wrote ", length(b))
  }

  bad <- 0L
  for (i in seq_along(a)) {
    left <- vapply(a[[i]], strip, character(1), USE.NAMES = FALSE)
    right <- vapply(b[[i]], strip, character(1), USE.NAMES = FALSE)
    if (!identical(left, right)) {
      bad <- bad + 1L
      cat("MISMATCH on probe ", i, "\n", sep = "")
      cat("  pandoc: ", paste(left, collapse = "\\n"), "\n", sep = "")
      cat("  filter: ", paste(right, collapse = "\\n"), "\n", sep = "")
    }
  }

  # A refusal is prose the engine wrote, not a specification, and its message
  # names atoms: "gog: `bin` needs a continuous column". Painting those would
  # color the engine's own words as though a reader could type them, and the
  # tinted panel already carries what a refusal means.
  #
  # It holds by construction rather than by intent: chunk output arrives with no
  # language on it, so the filter declines it. That is worth an assertion, since
  # widening the language test is exactly the change that would break it.
  verbatim_of <- function(lines) {
    starts <- grep("^\\\\begin\\{verbatim\\}", lines)
    ends <- grep("^\\\\end\\{verbatim\\}", lines)
    if (!length(starts)) return(character(0))
    unlist(Map(function(a, b) lines[a:b], starts, ends), use.names = FALSE)
  }
  if (!identical(verbatim_of(plain), verbatim_of(painted))) {
    fail("the filter changed a block that was not a specification")
  }
  if (!length(verbatim_of(painted))) {
    fail("no chunk output in the probes, so the refusal check proved nothing")
  }

  # The filter must actually have painted something, or a test that only proves
  # the escaping matches would also pass on a filter that did nothing at all.
  painted_words <- sum(vapply(b, function(block) {
    sum(lengths(regmatches(block, gregexpr("\\\\gog[A-Za-z]+\\{", block))))
  }, integer(1)))
  if (painted_words == 0L) fail("the filter painted nothing; the test is vacuous")

  if (bad) fail(bad, " of ", length(a), " probes escape differently from pandoc")
  cat(sprintf(
    "gog: %d probes escape exactly as pandoc does, with %d words painted\n",
    length(a), painted_words))
  invisible(TRUE)
}

if (sys.nframe() == 0L) check_latex_escaping(".")
