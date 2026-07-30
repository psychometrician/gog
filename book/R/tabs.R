# book/R/tabs.R — every sentence in the book, in both spellings
#
# One book, not three, with the code in language tabs over the same example. Three books would be three example-drift surfaces,
# and the plot is identical across languages anyway (same engine, same
# specification), so the plan is to **render once and show the snippets**.
#
# This is that, and it touches no chapter. A knitr *source hook* sees each
# chunk's code on its way to the page, so a chunk that is a gog sentence can be
# re-emitted as a tabset: the R the author wrote, and the Python the same
# sentence spells. The plot below it is unchanged, drawn once by R.
#
# Neither the Python nor the JavaScript is hand-written here, which is the point.
# They come from the two translators whose output is checked end-to-end against
# the engine for all 493 sentences in this book — 386 byte-identical plots, 93
# word-identical refusals, no disagreements, the same tally in both languages. So
# a tab cannot document syntax a binding refuses, which is the guard §4.6 asks
# for, discharged by construction rather than by a second check.
#
# The two translators are written in different languages for a reason, and it
# shows up here as one being cheaper than the other. Python's is a handful of
# local rewrites, which regular expressions do correctly — but it is Python, so
# it costs a subprocess and a temp-file driver. JavaScript's has to *reassociate*
# R's operator tree (it cannot overload `+`, `*`, `|` or `/`, spec §8), so it is
# written in R and R's own parser supplies the precedence — which means this file
# can simply call it.
#
# Two things are deliberately left alone. A chunk that is not a sentence keeps
# its ordinary rendering, and a chunk marked `error: true` stays R-only: its
# subject is the *message*, and the message shown would be R's. How those read
# in Python has its own page (`bindings/python.qmd`).

tab_translator <- file.path(proj_root, "py-pkg", "gog", "tests", "book_parity")
tab_js_emitter <- file.path(proj_root, "js-pkg", "gog", "test", "book_parity",
                            "translate.R")
tab_has_js <- file.exists(tab_js_emitter)
if (tab_has_js) source(tab_js_emitter, local = TRUE)

tab_jl_emitter <- file.path(proj_root, "jl-pkg", "GrammarOfGraphics", "test",
                            "book_parity", "translate.R")
tab_has_jl <- file.exists(tab_jl_emitter)
if (tab_has_jl) source(tab_jl_emitter, local = TRUE)

# Is this chunk a gog sentence rather than R around one?
#
# Deliberately strict. `data(` and a `+` is the shape of every sentence in the
# book, and requiring both keeps `knitr::kable(...)` and `head(df)` out — the
# second matters because it is *valid Python* and would otherwise earn a tab
# that says nothing true.
is_sentence <- function(code) {
  grepl("data\\(", code) && grepl("\\+", code) &&
    !grepl("<-", code) && !grepl("library\\(|source\\(|::", code)
}

# A chunk often sets a small table up before saying its sentence, and 39 of the
# book's chunks do. Testing `is_sentence()` against the *whole* chunk made every
# one of them render R-only: the sentence was translatable, the `<-` above it was
# the disqualification, and readers of the other three languages never saw those
# pages at all. The sentences were never untested — the parity corpus records all
# 513 either way — so this was a hole in what the book *showed*, not in what it
# checked, which is why nothing failed and it lasted.
#
# A table definition is admissible beside a sentence because all three
# translators can spell one: `octaves <- data.frame(freq = c(55, 110))` becomes a
# dict, an object, and a NamedTuple. A table *computed* in R (`exp(-(0:5)) * 100`)
# is refused by the translators themselves and reported as a gap, never guessed.
is_table_def <- function(code) {
  grepl("^\\s*[A-Za-z._][A-Za-z0-9._]*\\s*<-\\s*data\\.frame\\s*\\(", code) &&
    !grepl("library\\(|source\\(|::", code)
}

# One Python process per chapter, not one per sentence. The hook asks for a
# translation the first time it needs one, and gets the whole file's worth back.
tab_cache <- new.env(parent = emptyenv())

tab_translate <- function(sources) {
  driver <- tempfile(fileext = ".py")
  on.exit(unlink(driver), add = TRUE)
  writeLines(c(
    "import json, sys",
    paste0("sys.path.insert(0, ", shQuote(tab_translator, type = "sh"), ")"),
    "from translate import translate",
    "out = []",
    "for source in json.load(sys.stdin):",
    "    try:",
    "        python, _rules, blocked = translate(source)",
    "    except Exception as error:",
    "        python, blocked = None, 'translator failed: %s' % error",
    "    ok = False",
    "    if python and not blocked:",
    "        try:",
    # `exec`, not `eval`. A translation that will not compile is still not
    # shown — a tab nobody can run is worse than a tab that is not there — but
    # a chunk may now be a table definition *and* a sentence, and
    # `octaves = {...}` is a statement. Under `eval` it raised SyntaxError and
    # the Python tab vanished while JavaScript and Julia rendered fine, which
    # is exactly how this was caught: a three-tab bar reading "R JavaScript
    # Julia" on a chunk all three had translated.
    "            compile(python, '<tab>', 'exec')",
    "            ok = True",
    "        except SyntaxError:",
    "            ok = False",
    # `blocked` distinguishes the two ways a tab can be absent: a sentence
    # about the host language (the R chapter's pipes, which *should* have no
    # Python tab) from a sentence the translator could not handle (a defect).
    "    out.append({'python': python or '', 'ok': ok, 'blocked': blocked or ''})",
    "json.dump(out, sys.stdout)"
  ), driver)

  errors <- tempfile()
  on.exit(unlink(errors), add = TRUE)
  # `as.character()`, not a list: `toJSON()` on a list of length-1 vectors
  # writes `[["…"]]`, and the driver then hands a *list* to a function that
  # takes a string. It fails per sentence, silently, and the only symptom is
  # tabs that never appear.
  result <- suppressWarnings(system2(
    py_exe, shQuote(driver),
    input = jsonlite::toJSON(as.character(sources)),
    stdout = TRUE, stderr = errors
  ))
  if (!is.null(attr(result, "status"))) {
    warning("book: the language-tab translator failed, so this chapter is ",
            "R-only.\n", paste(readLines(errors, warn = FALSE), collapse = "\n"),
            call. = FALSE)
    return(NULL)
  }
  jsonlite::fromJSON(paste(result, collapse = ""), simplifyDataFrame = FALSE)
}

# The JavaScript half, and it is three lines because the emitter is R. No
# subprocess, no driver, no JSON round trip — the same shape of answer as
# `tab_translate` returns, so the caller cannot tell which language did the work.
tab_translate_js <- function(sources) {
  if (!tab_has_js) return(NULL)
  lapply(sources, function(source) {
    got <- tryCatch(
      translate_js(source),
      error = function(e) list(js = NA_character_, blocked = NA_character_)
    )
    list(js = if (is.na(got$js)) "" else got$js,
         ok = !is.na(got$js),
         blocked = if (is.na(got$blocked)) "" else got$blocked)
  })
}

# And the Julia half, which is the same three lines for the same reason.
tab_translate_jl <- function(sources) {
  if (!tab_has_jl) return(NULL)
  lapply(sources, function(source) {
    got <- tryCatch(
      translate_julia(source),
      error = function(e) list(julia = NA_character_, blocked = NA_character_)
    )
    list(julia = if (is.na(got$julia)) "" else got$julia,
         ok = !is.na(got$julia),
         blocked = if (is.na(got$blocked)) "" else got$blocked)
  })
}

# A chunk is not always one sentence. `operators.qmd` puts two side by side
# under `layout-ncol: 2` to show that word order does not change the picture, so
# a chunk is split into expressions the way `extract.R` splits one, and each is
# translated on its own. Joining first and translating after produces a
# parenthesized pair that is not valid Python, which is how this was found.
tab_split <- function(code) {
  parsed <- tryCatch(parse(text = code, keep.source = TRUE), error = function(e) NULL)
  if (is.null(parsed) || !length(parsed)) return(code)
  refs <- attr(parsed, "srcref")
  vapply(seq_along(parsed),
         function(i) paste(as.character(refs[[i]]), collapse = "\n"),
         character(1))
}

# Does this chunk earn a tabset?
#
# One predicate, consulted by both `tab_prime()` (which decides what to
# translate) and the source hook (which decides what to show). They used to test
# separately, and the hook's copy was the stricter of the two — so relaxing the
# primer alone would have translated 26 chunks and displayed none of them.
#
# The chunk-level part is the cheap half of `is_sentence()`: `data(` and a `+`,
# which keeps `kable(...)` and `head(df)` out. The `<-` judgment moves down to
# the individual statements, because a table definition legitimately carries one.
is_tabbable <- function(code) {
  if (!nzchar(code)) return(FALSE)
  if (!grepl("data\\(", code) || !grepl("\\+", code)) return(FALSE)
  if (grepl("library\\(|source\\(|::", code)) return(FALSE)
  pieces <- tab_split(code)
  # Every piece must be translatable, and at least one must be a sentence: a
  # chunk of nothing but table definitions draws no plot and earns no tab.
  all(vapply(pieces, function(p) is_sentence(p) || is_table_def(p), logical(1))) &&
    any(vapply(pieces, is_sentence, logical(1)))
}

# Every sentence in the chapter being knitted, translated in one go.
tab_prime <- function() {
  input <- knitr::current_input()
  if (is.null(input) || !file.exists(input)) return(invisible(NULL))

  lines <- readLines(input, warn = FALSE)
  starts <- grep("^```\\{r", lines)
  ends <- grep("^```\\s*$", lines)

  chunks <- list()
  for (start in starts) {
    stop_at <- ends[ends > start][1]
    if (is.na(stop_at)) next
    body <- lines[(start + 1):(stop_at - 1)]
    options <- body[grepl("^#\\|", body)]
    # A `layout-` chunk is a side-by-side comparison, and Quarto owns the
    # arrangement of that cell: a tabset inside it fights the layout and is
    # dropped. Those stay R-only, said here rather than discovered as a gap.
    if (any(grepl("layout", options))) next
    body <- body[!grepl("^#\\|", body)]
    code <- trimws(paste(trimws(body, which = "right"), collapse = "\n"))
    if (!is_tabbable(code)) next
    chunks[[length(chunks) + 1]] <- list(code = code, pieces = tab_split(code))
  }
  if (!length(chunks)) return(invisible(NULL))

  sources <- unlist(lapply(chunks, `[[`, "pieces"))
  translated <- tab_translate(sources)
  translated_js <- tab_translate_js(sources)
  translated_jl <- tab_translate_jl(sources)
  if (is.null(translated) && is.null(translated_js) && is.null(translated_jl))
    return(invisible(NULL))

  # One language's translation of one chunk: the joined text if every piece of
  # it translated, `NULL` if any did not. A chunk is not always one sentence
  # (`operators.qmd` puts two side by side), and half a tab is worse than none.
  joined <- function(all_parts, at, chunk, field) {
    if (is.null(all_parts)) return(list(text = NULL, missed = FALSE))
    parts <- all_parts[at + seq_along(chunk$pieces)]
    if (all(vapply(parts, function(p) isTRUE(p$ok), logical(1)))) {
      return(list(
        text = paste(vapply(parts, `[[`, character(1), field), collapse = "\n"),
        missed = FALSE
      ))
    }
    # `blocked` distinguishes the two ways a tab can be absent: a sentence about
    # the host language (the R chapter's pipes, which *should* have no tab in
    # another language) from a sentence the translator could not handle, which
    # is a defect.
    list(text = NULL,
         missed = !any(vapply(parts, function(p) nzchar(p$blocked), logical(1))))
  }

  languages <- c(python = "Python", julia = "Julia", js = "JavaScript")
  missed <- list(python = character(), js = character(), julia = character())
  at <- 0L
  for (chunk in chunks) {
    outcomes <- list(
      python = joined(translated, at, chunk, "python"),
      js = joined(translated_js, at, chunk, "js"),
      julia = joined(translated_jl, at, chunk, "julia")
    )
    at <- at + length(chunk$pieces)

    texts <- lapply(outcomes, `[[`, "text")
    if (any(!vapply(texts, is.null, logical(1)))) {
      assign(chunk$code, texts, envir = tab_cache)
    }
    for (lang in names(languages)) {
      if (isTRUE(outcomes[[lang]]$missed)) {
        missed[[lang]] <- c(missed[[lang]], chunk$code)
      }
    }
  }
  # A sentence that does not translate loses a tab and nothing says so, which is
  # the silent drop this project refuses everywhere else. A sentence the
  # translator *declines* is not that: it reports why, and is passed over quietly.
  for (lang in names(languages)) {
    if (length(missed[[lang]])) {
      warning("book: ", length(missed[[lang]]), " sentence(s) in ", basename(input),
              " have no ", languages[[lang]], " tab. First: ",
              substr(gsub("\\s+", " ", missed[[lang]][1]), 1, 70), call. = FALSE)
    }
  }
  invisible(NULL)
}

# The hook. Quarto has already installed one, so the original is kept and used
# for everything this does not claim.
tab_default <- knitr::knit_hooks$get("source")

knitr::knit_hooks$set(source = function(x, options) {
  code <- trimws(paste(x, collapse = "\n"))

  # Which chunks are claimed at all. The format is deliberately *not* one of these
  # tests any more (see below); these three are the same in every format.
  #
  # A chunk marked `error: true` stays R-only because its subject is the *message*,
  # and a message is worded in the caller's own syntax — showing four spellings of a
  # sentence beside one binding's refusal would be four claims and one piece of
  # evidence. A laid-out chunk (`layout-ncol`) is left alone because the layout is
  # over *plots*, and re-emitting its source as four blocks would put them in the
  # grid. And a chunk that is not a gog sentence has nothing to spell twice.
  laid_out <- any(nzchar(unlist(options[grepl("^layout", names(options))])))
  if (isTRUE(options$error) || laid_out || !is_tabbable(code)) {
    return(tab_default(x, options))
  }

  if (!exists("tab_primed", envir = tab_cache)) {
    assign("tab_primed", TRUE, envir = tab_cache)
    tab_prime()
  }
  if (!exists(code, envir = tab_cache)) return(tab_default(x, options))
  entry <- get(code, envir = tab_cache)

  # **Print shows all four spellings too, and always could have** (built
  # 2026-07-26; §4.6's "print is single-language" is superseded and says so).
  #
  # The thing that made print look impossible was reading one obstacle as two. A
  # `panel-tabset` really is HTML-only — but this hook rewrites the **source**
  # display and nothing else, so the plot below is emitted by knitr exactly once
  # whatever this returns. *Four syntaxes over one plot was already the structure.*
  # What flattened badly in print was not the four code blocks; it was the `##`
  # headings the tabset needs to label its tabs, which in a book are real
  # subsections and would have put four of them in the table of contents on each of
  # ~385 examples. Print needs no tabs, so it needs no headings, so the objection
  # does not reach it: a bold label is not a section.
  #
  # So the two formats now differ only in the *container* — a tabset the reader
  # clicks, or four labeled blocks they read down — and no longer in what the book
  # says. That matters more than it looks: a print reader was seeing one binding
  # under a cover promising every language, and the fix cost four lines.
  # **The order is R, Python, Julia, JavaScript, and it is a reading order rather
  # than an alphabet.** The first three are the same sentence three times — the
  # operators are identical and only the capture idiom moves (a bare name, `col.x`,
  # `:x`), so a reader who knows one can check the next two at a glance. JavaScript
  # is the one that reads differently, because it cannot overload `+ * | /` and
  # spells them as four words (spec §8). Putting it last means the three that rhyme
  # sit together and the odd one out arrives after the pattern is established,
  # instead of interrupting it. Set here, so the tabs, the print blocks and the
  # warnings below cannot disagree; the chapter order in `_quarto.yml` matches.
  spelling <- function(name, language, source) {
    if (is.null(source)) return("")
    paste0("**", name, "**\n\n```", language, "\n", source, "\n```\n\n")
  }
  if (!knitr::is_html_output()) {
    return(paste0(
      "\n",
      spelling("R", "r", code),
      spelling("Python", "python", entry$python),
      spelling("Julia", "julia", entry$julia),
      spelling("JavaScript", "js", entry$js)
    ))
  }

  tab <- function(name, language, source) {
    if (is.null(source)) return("")
    paste0("## ", name, "\n\n```", language, "\n", source, "\n```\n\n")
  }

  paste0(
    "\n::: {.panel-tabset}\n\n",
    tab("R", "r", code),
    tab("Python", "python", entry$python),
    tab("Julia", "julia", entry$julia),
    tab("JavaScript", "js", entry$js),
    ":::\n"
  )
})
