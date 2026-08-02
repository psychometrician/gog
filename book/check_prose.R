# book/check_prose.R
# The book has one voice, and this is the half of it a machine can hold.
#
# Every convention in this book has drifted at least once, and always the same
# way: not by anyone deciding, but *by chapter*. A writer holds one file in their
# head for an afternoon and spells things consistently inside it. `design-laws.qmd`
# ran 16-0 on `GOG` while `marks/zone.qmd` ran 1-7. Title Case sits in five
# headings, all in the first half of `transforms.qmd`. Em dashes were swept out of
# the teaching chapters in one pass and left in the reference chapters, so
# `index.qmd` had 1 and `combinations.qmd` had 40. Nobody reading either file
# alone would ever see the split, which is why these are checked rather than
# merely written down.
#
# What is checked here is only what is **true or false about a line**: a bold run
# is longer than eight words or it is not, an em dash is present or it is not.
# Sentence length is deliberately absent, and belongs to `book/readability.py`,
# which is a report and must stay one. A threshold on words per sentence gets
# satisfied by splitting sentences in half rather than by rewriting them, which is
# the metric improving while the prose gets worse.
#
# Scope is the book's prose and any English the book *shows* a reader: chunk
# comments, plot titles, axis labels and chunk options all reach the page. It is
# not the four packages. A diagnostic string in `r-pkg/gog/R/spec.R` is the
# package's voice and follows the package's conventions; on 2026-08-02 a session
# rewrote six of `query()`'s refusals to remove an em dash and then reverted
# itself, because the same character was already written 36 times across
# `atoms.R`, `render.R` and `spec.R`.
#
# Run from the repo root; sourced by r-pkg/gog/tests/test_basic.R.

check_prose <- function(book_dir = "book") {
  qmds <- list.files(book_dir, pattern = "[.]qmd$", recursive = TRUE, full.names = TRUE)
  qmds <- qmds[!grepl("/_book/", qmds, fixed = TRUE)]

  # Idiom. The list is a probe rather than a boundary: a phrase meaning something
  # other than the sum of its words is an idiom whether or not it is named here.
  # It is the translation test that makes this a rule instead of a preference —
  # the book is planned in every major language, and an idiom does not survive
  # into any of them. It becomes nonsense, or 26 translators invent 26 different
  # replacements.
  idioms <- c(
    "earns its keep", "earn their keep", "pays off", "the giveaway",
    "under the hood", "out of the box", "rule of thumb", "at the end of the day",
    "boils down", "in a nutshell", "hand in hand", "bread and butter",
    "low-hanging", "heavy lifting", "moving parts", "sweet spot",
    "silver bullet", "best of both worlds", "from scratch", "bells and whistles",
    "apples to apples", "cuts both ways", "no free lunch",
    "elephant in the room", "tip of the iceberg", "second nature",
    "load-bearing", "the expert's convenience"
  )

  # `index.qmd` quotes an imagined fluent expert saying "the difficulty earns its
  # keep". That sentence is the one the preface is arguing *against*, and removing
  # its idiom would remove the thing being shown. The rules govern prose the book
  # writes, never text it quotes.
  idiom_exempt <- list("index.qmd" = "difficulty earns its keep")

  # Two places transcribe an engine diagnostic word for word, and a diagnostic is
  # the *package's* sentence, not the book's. The book has to quote it exactly or
  # it is claiming output the reader will not get, so the em dash stays and the
  # engine keeps its own punctuation. `legality.rs` emits both.
  dash_exempt <- list(
    "channels.qmd"   = "has no opacity feature",
    "transforms.qmd" = "a share is a share of a"
  )
  dash_ok <- function(short, line) {
    ex <- dash_exempt[[short]]
    !is.null(ex) && grepl(ex, line, fixed = TRUE)
  }

  # `—` is also a *glyph* on the combinations grids, where it sits beside `●` and
  # `✅` and means "the mark has no such feature". A symbol is not punctuation, so
  # those uses stay, and they are recognized by shape rather than by filename: the
  # whole content of a string in code, or the first thing inside a legend label,
  # or a comment defining the symbol. Anything else on the line is still checked.
  ungl <- function(s) {
    s <- gsub("\"—\"", "\"\"", s)          # "—", the glyph as a code value
    s <- gsub("\\*\\*—", "**", s)          # **— none**, a legend label
    s <- gsub("^\\s*#\\s+—\\s", "# ", s) # #   —  the mark has no such feature
    s <- gsub("\\|\\s*—\\s*(?=\\|)", "| ", s, perl = TRUE)  # | — | a cell that is the glyph
    s
  }

  # The Nine Laws carry official names, set in spec §4 and repeated in the
  # working agreement. `## Law 5: Explicit Over Implicit` is that name, not Title
  # Case drift, and rewriting it here would desync the chapter from the law.
  law_heading <- "^#+\\s*Law\\s+[0-9]"

  # Title Case is found by capitalization, so the proper nouns have to be named or
  # every one of them is a false positive. Keep this list short and concrete: it
  # is cheaper to add a name here than to weaken the rule into uselessness.
  proper <- c("R", "Python", "Julia", "JavaScript", "GOG", "LOESS", "SVG", "PDF",
              "HTML", "CSS", "SQL", "CSV", "JSON", "Quarto", "Quarto's", "Posit",
              "Arrow", "Anthropic", "Wilkinson", "Playfair", "Herschel", "Bertin",
              "Tufte", "Sejong", "Hangeul", "Hunminjeongeum", "Mercator",
              "Korean", "English", "American", "Law", "Part", "Jupyter",
              "RStudio", "Windows", "macOS", "Linux", "CRAN", "PyPI", "ISO",
              "Cartesian", "Continuous", "Categorical", "Nine", "Laws",
              "R's", "Wilkinson's", "Hangeul's", "GOG's", "Sejong's")

  MAX_HEADING <- 8
  MAX_BOLD <- 8

  # Strip inline code before counting anything. A code span is one name however
  # many spaces are inside it, and `` `—` `` in the combinations legend is a glyph
  # in a table rather than punctuation in a sentence.
  # Lowercase on purpose. An uppercase placeholder reads as a capitalized word to
  # the Title Case rule below, and every heading naming an atom becomes a hit.
  strip_code <- function(s) gsub("`[^`]*`", "code", s)

  nwords <- function(s) {
    s <- strip_code(s)
    s <- gsub("[*_]", "", s)
    s <- trimws(gsub("\\s+", " ", s))
    if (!nzchar(s)) return(0L)
    length(strsplit(s, " ", fixed = TRUE)[[1]])
  }

  bad_bold <- character(0)
  bad_dash <- character(0)
  bad_head <- character(0)
  bad_case <- character(0)
  bad_call <- character(0)
  bad_idiom <- character(0)

  for (f in qmds) {
    lines <- readLines(f, warn = FALSE)
    short <- sub("^.*book/", "", f)
    in_chunk <- FALSE
    in_yaml <- FALSE
    where <- function(i) sprintf("  %s:%d  %s", short, i, trimws(lines[i]))

    # A bold run is tracked across lines, because prose here is hard-wrapped at
    # about 80 characters and a bolded sentence is usually longer than that. The
    # first version of this check read one line at a time and silently missed
    # every wrapped one, which is the same line-wrap blind spot that hid three of
    # eleven hits in the 2026-08-01 rename.
    b_open <- FALSE; b_start <- NA_integer_; b_text <- ""; b_item <- FALSE

    for (i in seq_along(lines)) {
      line <- lines[i]

      # YAML front matter carries the chapter title, which is prose a reader sees.
      if (i == 1L && grepl("^---\\s*$", line)) { in_yaml <- TRUE; next }
      if (in_yaml) {
        if (grepl("^---\\s*$", line)) in_yaml <- FALSE
        else if (grepl("—", line)) bad_dash <- c(bad_dash, where(i))
        next
      }

      if (grepl("^\\s*```", line)) { in_chunk <- !in_chunk; next }

      # Inside a chunk, every em dash reaches the reader one way or another: in a
      # comment they read, in a `title()` string drawn into the SVG, or in a
      # `#| fig-cap` rendered as a caption.
      if (in_chunk) {
        if (grepl("—", ungl(line)) && !dash_ok(short, line))
          bad_dash <- c(bad_dash, where(i))
        next
      }

      if (grepl("^\\s*:::\\s*\\{?\\.callout", line)) {
        bad_call <- c(bad_call, where(i))
        next
      }

      # --- Headings ---------------------------------------------------------
      if (grepl("^#+\\s+\\S", line)) {
        h <- sub("^#+\\s+", "", line)
        h <- gsub("\\{#[^}]*\\}", "", h)   # explicit anchors are not words
        n <- nwords(h)
        # A heading is prose a reader sees, so it is checked for the em dash too.
        # The first version returned before this point and never looked.
        if (grepl("—", ungl(strip_code(h))) && !dash_ok(short, line))
          bad_dash <- c(bad_dash, where(i))
        # Two sentences in a heading is the same defect as an over-long one: the
        # argument has climbed out of the paragraph and into the label.
        # A capital after the stop is what marks a second sentence. Requiring it
        # keeps "Plot-level vs. layer-level data" out of the report.
        if (n > MAX_HEADING || grepl("[.!?]\\s+[A-Z]", h))
          bad_head <- c(bad_head, sprintf("  %s:%d  [%dw] %s", short, i, n, trimws(h)))
        if (!grepl(law_heading, line)) {
          # A word opening a second sentence is capitalized by grammar, not by
          # Title Case, so it is neutralized before the count.
          h2 <- gsub("([.!?])\\s+[A-Z]", "\\1 x", strip_code(h))
          words <- strsplit(trimws(gsub("[^A-Za-z' ]", " ", h2)), "\\s+")[[1]]
          words <- words[nzchar(words)]
          if (length(words) > 1L) {
            capd <- words[-1][grepl("^[A-Z]", words[-1]) & !(words[-1] %in% proper)]
            if (length(capd))
              bad_case <- c(bad_case, sprintf("  %s:%d  %s   (%s)", short, i,
                                              trimws(h), paste(capd, collapse = ", ")))
          }
        }
        next
      }

      prose <- strip_code(line)

      # --- Em dash ----------------------------------------------------------
      if (grepl("—", ungl(prose)) && !dash_ok(short, line)) bad_dash <- c(bad_dash, where(i))

      # --- Idiom ------------------------------------------------------------
      low <- tolower(line)
      for (p in idioms) {
        if (grepl(p, low, fixed = TRUE)) {
          ex <- idiom_exempt[[short]]
          if (!is.null(ex) && grepl(ex, low, fixed = TRUE)) next
          bad_idiom <- c(bad_idiom, sprintf("  %s:%d  \"%s\"", short, i, p))
        }
      }

      # --- Bolded sentences -------------------------------------------------
      # A short bold run-in label may open a *list item*, with the terminal
      # period inside the bold. That is a layout device, and it is the only
      # carve-out: a bold label opening an ordinary paragraph is emphasis, and
      # goes. Anything longer, or anywhere else, is a sentence wearing bold, and
      # a bolded sentence reads as a box, which is what the 33 callouts were
      # removed for.
      if (!nzchar(trimws(line))) {           # a blank line ends a paragraph, so
        b_open <- FALSE; b_text <- ""        # an unmatched `**` cannot run away
      } else {
        # `strsplit` drops trailing empty fields, so a line *ending* in `**`
        # comes back one segment short and the closing delimiter is never seen.
        # The run then stays open and swallows the next line. Count the
        # delimiters and pad instead of trusting the split.
        ndelim <- if (grepl("**", line, fixed = TRUE))
          length(gregexpr("**", line, fixed = TRUE)[[1]]) else 0L
        segs <- strsplit(line, "**", fixed = TRUE)[[1]]
        if (length(segs) < ndelim + 1L)
          segs <- c(segs, rep("", ndelim + 1L - length(segs)))
        nseg <- length(segs)
        for (j in seq_len(nseg)) {
          if (b_open) b_text <- paste0(b_text, segs[j])
          if (j < nseg) {                    # a `**` delimiter follows
            if (b_open) {
              n <- nwords(b_text)
              punct <- grepl("[.!?]\\s+[A-Z]", b_text) ||
                (grepl("[.!?]$", trimws(b_text)) && n > 3L)
              if ((n > MAX_BOLD || punct) && !(b_item && n <= MAX_BOLD))
                bad_bold <- c(bad_bold, sprintf("  %s:%d  [%dw] **%s**",
                                                short, b_start, n, trimws(b_text)))
              b_open <- FALSE; b_text <- ""
            } else {
              b_open <- TRUE; b_start <- i; b_text <- ""
              # A run-in label is the first bold on a list-item line.
              b_item <- (j == 1L) && grepl("^\\s*([-*+]|[0-9]+\\.)\\s+$", segs[1])
            }
          }
        }
        if (b_open) b_text <- paste0(b_text, " ")   # the run crosses a line break
      }
    }
  }

  total <- length(bad_bold) + length(bad_dash) + length(bad_head) +
    length(bad_case) + length(bad_call) + length(bad_idiom)

  report <- function(items, headline, advice) {
    if (!length(items)) return(invisible(NULL))
    cat(headline, "\n", sep = "")
    cat(paste(items, collapse = "\n"), "\n", sep = "")
    cat("  ", advice, "\n", sep = "")
  }

  if (total) {
    report(bad_bold, "FAIL: a whole sentence is set in bold",
           "Bold introduces a term; it does not emphasize a sentence. Set it plain.")
    report(bad_dash, "FAIL: em dash in text a reader sees",
           "Use a comma, a colon, a semicolon, parentheses, or two sentences.")
    report(bad_head, "FAIL: heading is a sentence, not a label",
           "A short noun phrase, eight words at most. The argument goes in the paragraph.")
    report(bad_case, "FAIL: heading is in Title Case",
           "Sentence case: 'Kernel density estimate', not 'Kernel Density Estimate'.")
    report(bad_call, "FAIL: callout box",
           "Weave the point into the prose, or let the plot show it.")
    report(bad_idiom, "FAIL: idiom does not translate",
           "Say the literal thing. This book is planned in every major language.")
    stop("check_prose: ", total, " prose inconsistency(ies)")
  }

  cat("PASS: prose is consistent (", length(qmds), "chapters )\n")
  invisible(TRUE)
}
