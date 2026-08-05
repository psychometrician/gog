# Does the highlighter paint the vocabulary the book declares?
#
# A gog specification is colored by kind in both editions, and the two editions
# do it differently because the formats differ. `gog-syntax.html` paints in the
# browser, so that it adds to Pandoc's tokens and keeps the panel and the copy
# button Quarto builds around a block. `gog-syntax.lua` paints at render time,
# because a PDF has no browser and nothing to preserve.
#
# The Lua reads `grammar.qmd` itself and so cannot drift from it. The browser
# cannot: a highlighter that waits on a second request paints the page twice and
# a reader watches it happen, so it holds a copy of the kernel.
#
# The cost of that copy is drift. A new mark lands in the kernel table, nothing
# in the browser knows about it, and it renders as a column name for however
# long it takes somebody to notice a word that is not colored. Nothing else
# would fail: the chapter is correct, the plot draws, the build exits 0.
#
# So the copy checks itself, on every render, in five directions.
#
#   1. Every word the book declares is painted, under the book's own kind.
#   2. Every word the highlighter paints is one the book declares.
#   3. JavaScript's words for the operators match what the package exports.
#   4. The PDF filter spells those same operator words.
#   5. The two agree about which kind takes which ink.
#
# The third is separate because the kernel table writes the operators as the
# symbols they are, `+ * | /`, and JavaScript cannot overload any of them, so it
# spells them as words. There is no row in the book to check those against. The
# export list is the authority instead, which is the right one anyway: a
# changelog naming something an installed copy does not have is the same defect
# as a chapter naming a transform that does not exist.
#
# The fifth is the one that is easy to forget and impossible to see. Both files
# map eleven kinds onto five inks, and if the two mappings disagree then a
# specification is one color on the page and another in print, in a book whose
# claim is that they are one book. Nothing in either render can notice that.
#
# This runs as a pre-render, so a mismatch stops the render rather than
# publishing a page that quietly stopped coloring half a sentence.

local({
  fail <- function(...) stop("gog: ", ..., call. = FALSE)

  # Run by both projects, as a pre-render, with the project directory as the
  # working directory. `book/` and `blog/` sit at the same depth, so the package
  # sources are `../` from either; the chapter is here in one and one directory
  # over in the other, and the highlighter is local to both because the blog
  # stages a copy of it beside the stylesheet.
  painter <- "gog-syntax.html"
  grammar <- Filter(file.exists,
                    c("grammar.qmd", file.path("..", "book", "grammar.qmd")))[1]
  filter <- Filter(file.exists,
                   c("gog-syntax.lua", file.path("..", "book", "gog-syntax.lua")))[1]
  exports <- file.path("..", "js-pkg", "gog", "src", "index.js")
  if (is.na(grammar)) fail("the kernel check cannot find grammar.qmd")
  if (is.na(filter)) fail("the kernel check cannot find gog-syntax.lua")
  for (f in c(painter, exports)) {
    if (!file.exists(f)) fail("the kernel check cannot find ", f)
  }

  # -- what the book declares ----------------------------------------------
  #
  # The kernel block is a table, one row per kind, and an atom is delimited by
  # backticks rather than inferred from the spacing, so a note in a cell cannot
  # be mistaken for a name. A word may carry the marker for designed-but-undrawn
  # outside its backticks, which leaves the name itself unchanged here.
  book <- readLines(grammar, warn = FALSE)
  start <- grep("^## The kernel", book)
  if (!length(start)) fail("grammar.qmd has no '## The kernel' section")
  block <- book[start:length(book)]
  block <- block[seq_len(which(!grepl("^\\|", block) & nzchar(trimws(block)) &
                               seq_along(block) > 2)[1])]
  block <- block[grepl("^\\|", block)]

  declared <- list()
  for (ln in block) {
    label <- regmatches(ln, regexpr("\\*\\*[A-Za-z]+\\*\\*", ln))
    if (!length(label)) next
    kind <- sub("s$", "", tolower(gsub("\\*", "", label)))
    words <- gsub("`", "", regmatches(ln, gregexpr("`[^`]+`", ln))[[1]])
    words <- words[grepl("^[a-z][a-z_0-9]*$", words)]
    if (length(words)) declared[[kind]] <- unique(c(declared[[kind]], words))
  }
  if (!length(declared$mark) || !length(declared$transform)) {
    fail("could not read the kernel block in grammar.qmd")
  }

  # -- what the highlighter paints ------------------------------------------
  #
  # Whole-line comments come off first. Every comment inside the object is one,
  # and dropping only those means a `//` cannot be mistaken for the start of a
  # comment where it is really part of a word.
  js <- readLines(painter, warn = FALSE)
  js <- js[!grepl("^\\s*//", js)]
  js <- paste(js, collapse = "\n")
  object <- regmatches(js, regexpr("var KERNEL = \\{[^}]*\\}", js))
  if (!length(object)) fail("gog-syntax.html has no `var KERNEL = { ... }`")

  painted <- list()
  # A kind's words may be written as several literals joined by `+`, because one
  # line of seventeen transforms does not fit inside the margin.
  entries <- regmatches(object, gregexpr(
    "[a-z]+:\\s*\"[^\"]*\"(\\s*\\+\\s*\"[^\"]*\")*", object))[[1]]
  for (entry in entries) {
    kind <- sub(":.*$", "", entry)
    words <- gsub("\"", "", regmatches(entry, gregexpr("\"[^\"]*\"", entry))[[1]])
    words <- unlist(strsplit(paste(words, collapse = " "), "\\s+"))
    painted[[kind]] <- unique(words[nzchar(words)])
  }
  if (!length(painted)) fail("could not read `KERNEL` in gog-syntax.html")

  # -- 1 and 2: the book and the highlighter, kind by kind -------------------
  problems <- character()
  for (kind in sort(union(names(declared), setdiff(names(painted), "operator")))) {
    missing <- setdiff(declared[[kind]], painted[[kind]])
    extra <- setdiff(painted[[kind]], declared[[kind]])
    if (length(missing)) {
      problems <- c(problems, sprintf(
        "  the book declares %s that gog-syntax.html does not paint: %s",
        kind, paste(missing, collapse = " ")))
    }
    if (length(extra)) {
      problems <- c(problems, sprintf(
        "  gog-syntax.html paints %s the book does not declare: %s",
        kind, paste(extra, collapse = " ")))
    }
  }

  # -- 3: the words JavaScript spells the operators with ---------------------
  index <- paste(readLines(exports, warn = FALSE), collapse = "\n")
  line <- regmatches(index, regexpr(
    "export\\s*\\{[^}]*\\}\\s*from\\s*\"\\./spec\\.js\"", index))
  if (!length(line)) fail("index.js no longer exports from ./spec.js")
  spelled <- trimws(unlist(strsplit(
    gsub("^export\\s*\\{|\\}.*$", "", line), ",")))
  # The classes are not words a sentence contains, and `data`, `query` and
  # `facet` are already declared by the book, so what is left is exactly the set
  # that exists because JavaScript cannot overload an operator.
  spelled <- spelled[grepl("^[a-z][a-z_0-9]*$", spelled)]
  spelled <- setdiff(spelled, unlist(declared, use.names = FALSE))

  missing <- setdiff(spelled, painted$operator)
  extra <- setdiff(painted$operator, spelled)
  if (length(missing)) {
    problems <- c(problems, sprintf(
      "  the JavaScript package exports operator words gog-syntax.html does not paint: %s",
      paste(missing, collapse = " ")))
  }
  if (length(extra)) {
    problems <- c(problems, sprintf(
      "  gog-syntax.html paints operator words the JavaScript package does not export: %s",
      paste(extra, collapse = " ")))
  }

  # -- 4 and 5: the PDF filter says the same thing ---------------------------
  #
  # The Lua reads the kernel out of `grammar.qmd` itself, so its *words* cannot
  # drift and are not checked here. What it holds of its own are the two things
  # the chapter does not state: the operator words JavaScript spells, and which
  # of the five inks each kind takes. Both have a counterpart in the browser
  # pass, and a disagreement between them prints one specification two ways in
  # one book.
  lua <- paste(readLines(filter, warn = FALSE), collapse = "\n")

  lua_spelled <- regmatches(lua, regexpr(
    "local SPELLED\\s*=\\s*\"[^\"]*\"", lua))
  if (!length(lua_spelled)) fail("gog-syntax.lua has no `local SPELLED`")
  # Two anchored substitutions rather than one alternation. `^.*"` is greedy and
  # runs to the *last* quote on the line, which deletes the words it was meant to
  # keep and reports every one of them as missing.
  lua_spelled <- sub("^[^\"]*\"", "", lua_spelled)
  lua_spelled <- unlist(strsplit(trimws(sub("\"$", "", lua_spelled)), "\\s+"))

  missing <- setdiff(spelled, lua_spelled)
  extra <- setdiff(lua_spelled, spelled)
  if (length(missing)) {
    problems <- c(problems, sprintf(
      "  the JavaScript package exports operator words gog-syntax.lua does not paint: %s",
      paste(missing, collapse = " ")))
  }
  if (length(extra)) {
    problems <- c(problems, sprintf(
      "  gog-syntax.lua paints operator words the JavaScript package does not export: %s",
      paste(extra, collapse = " ")))
  }

  # Both files map kind to ink in a block called `INK`. The names differ in case
  # only, because one writes a CSS class and the other a LaTeX command.
  ink_of <- function(text, pattern) {
    block <- regmatches(text, regexpr(pattern, text))
    if (!length(block)) return(NULL)
    pairs <- regmatches(block, gregexpr("[a-z]+\\s*[:=]\\s*\"?[A-Za-z]+\"?", block))[[1]]
    kinds <- tolower(trimws(sub("[:=].*$", "", pairs)))
    inks <- tolower(gsub("\"", "", trimws(sub("^[^:=]*[:=]", "", pairs))))
    stats::setNames(inks, kinds)
  }
  js_ink <- ink_of(paste(readLines(painter, warn = FALSE), collapse = "\n"),
                   "var INK = \\{[^}]*\\}")
  lua_ink <- ink_of(lua, "local INK = \\{[^}]*\\}")
  if (is.null(js_ink)) fail("gog-syntax.html has no `var INK = { ... }`")
  if (is.null(lua_ink)) fail("gog-syntax.lua has no `local INK = { ... }`")

  for (kind in sort(union(names(js_ink), names(lua_ink)))) {
    # `[[` on a named vector raises rather than returning NULL for a name that is
    # not there, and a kind missing from one of the two files is exactly the case
    # this loop exists to report.
    a <- if (kind %in% names(js_ink)) js_ink[[kind]] else NULL
    b <- if (kind %in% names(lua_ink)) lua_ink[[kind]] else NULL
    if (is.null(a) || is.null(b) || !identical(a, b)) {
      problems <- c(problems, sprintf(
        "  %s is painted %s in HTML and %s in the PDF",
        kind, if (is.null(a)) "nowhere" else a,
        if (is.null(b)) "nowhere" else b))
    }
  }

  if (length(problems)) {
    fail("the highlighter and the kernel have drifted apart.\n",
         paste(problems, collapse = "\n"),
         "\n  `KERNEL` and `INK` live in book/gog-syntax.html;",
         " `SPELLED` and `INK` in book/gog-syntax.lua.")
  }

  cat(sprintf("gog: the highlighter paints all %d words of the kernel\n",
              length(unique(unlist(painted, use.names = FALSE)))))
})
