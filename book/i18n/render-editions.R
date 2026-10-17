# book/i18n/render-editions.R
# Render the English book and its translated editions with babelquarto, into
# `book/_editions/`: the English pages at its root and each edition in a folder
# named by its language (`_editions/ko/`), each page linked to the same page in
# the others.
#
# Usage, from the repo root:
#   Rscript book/i18n/render-editions.R [--lang ko] [--site-url URL]
#   Rscript book/i18n/render-editions.R --chapter first-plot[,marks/point]
#
# The first renders the whole of both, about nine minutes, and is what a
# publish needs. The second is for reading a translation while it is reviewed:
# it renders only the named chapters of the edition, in under a minute, into
# the folder the whole render wrote, which it needs to exist.
#
# Four things this does that babelquarto does not, each for a reason:
#
# 1. It renders a copy of `book/`, never `book/` itself. babelquarto deletes the
#    project's output folder before it starts, which would take the English
#    render in `_book/` with it.
# 2. It keeps the HTML format only. babelquarto renders every format a project
#    names, and the PDF would double the time and put a download link in the
#    site.
# 3. It names the repository's root and the edition's language
#    (`GOG_PROJECT_ROOT`, `GOG_BOOK_LANG`), since babelquarto renders from a copy
#    in a temporary folder with no repository above it.
# 4. It writes babelquarto's settings into the copy alone, so the English book's
#    `_quarto.yml` never carries them: the languages, the sidebar links, and
#    `part-<lang>:` beside each part whose page is translated.
#
# babelquarto is pinned at 0.1.0, the version rOpenSci reviewed. The later
# development build writes the translated chapter list into a profile file,
# where Quarto adds it to the English list instead of replacing it.

args <- commandArgs(trailingOnly = TRUE)
arg <- function(name, default) {
  i <- match(name, args)
  if (is.na(i)) default else args[i + 1]
}
lang <- arg("--lang", "ko")
site_url <- arg("--site-url", "")
chapters <- arg("--chapter", "")
chapters <- if (nzchar(chapters)) sub("[.]qmd$", "", strsplit(chapters, ",")[[1]]) else character(0)

if (!requireNamespace("babelquarto", quietly = TRUE) ||
    packageVersion("babelquarto") != "0.1.0")
  stop("render-editions.R needs babelquarto 0.1.0: ",
       "pak::pak(\"ropensci-review-tools/babelquarto@v0.1.0\")", call. = FALSE)

root <- normalizePath(".")
if (!dir.exists(file.path(root, "gog-cli")) || !file.exists(file.path(root, "book", "_quarto.yml")))
  stop("run render-editions.R from the repository's root", call. = FALSE)

# 1. The copy.
stage <- file.path(tempfile("editions"), "book")
dir.create(stage, recursive = TRUE)
skip <- c("_book", "_editions", ".quarto", ".venv", "__pycache__", ".publish-render.log",
          "Rplots.pdf")
for (f in list.files("book", all.files = TRUE, no.. = TRUE)) {
  if (f %in% skip || grepl("_files$|^_fold_probe", f)) next
  file.copy(file.path("book", f), stage, recursive = TRUE)
}

# 2 and 4. The copy's settings.
# Each language's link names it in its own script, as a reader looks for it.
native_name <- function(code) {
  names <- c(ko = "한국어", es = "Español", de = "Deutsch", fr = "Français",
             zh = "中文", ja = "日本語", pt = "Português", ru = "Русский")
  if (code %in% names(names)) names[[code]] else code
}
yes_no <- list(logical = function(x) {
  out <- ifelse(x, "true", "false"); class(out) <- "verbatim"; out
})
config_file <- file.path(stage, "_quarto.yml")
config <- yaml::read_yaml(config_file)
config$format <- config$format["html"]
config$babelquarto <- list(
  mainlanguage = "en",
  languages = list(lang),
  languagelinks = "sidebar",
  # The translate glyph (文A), the one browsers and phones use for "this page in
  # another language"; babelquarto's default, a globe, also reads as "website".
  icon = "bi bi-translate",
  languagecodes = list(list(name = "en", text = "English"),
                       list(name = lang, text = native_name(lang)))
)
part_lang <- function(entry) {
  # A part of one chapter must stay a one-item list: written back as a plain
  # value, Quarto refuses the whole configuration.
  if (is.list(entry) && !is.null(entry$chapters)) entry$chapters <- as.list(entry$chapters)
  if (is.list(entry) && !is.null(entry$part)) {
    translated <- sub("[.]qmd$", paste0(".", lang, ".qmd"), entry$part)
    # A part page not translated yet falls back to its English text under the
    # edition's name, as babelquarto does for a chapter. Without it the part
    # renders under its English name, and babelquarto, finding no page by the
    # edition's name, links neither one to the other.
    if (!file.exists(file.path(stage, translated)))
      file.copy(file.path(stage, entry$part), file.path(stage, translated))
    entry[[paste0("part-", lang)]] <- translated
  }
  entry
}
config$book$chapters <- lapply(config$book$chapters, part_lang)
config$book$appendices <- as.list(config$book$appendices)
yaml::write_yaml(config, config_file, handlers = yes_no)

# 3. The root and the language, read by the book's own scripts.
Sys.setenv(GOG_PROJECT_ROOT = root, GOG_BOOK_LANG = lang)

out <- file.path(root, "book", "_editions")

# The quick path: the named chapters of the edition alone, set up exactly as
# babelquarto sets up its own render of the edition (`render_quarto_lang`), and
# linked to their English pages by babelquarto's own functions.
if (length(chapters)) {
  if (!dir.exists(file.path(out, lang)))
    stop("--chapter needs a whole render first: Rscript book/i18n/render-editions.R", call. = FALSE)
  started <- Sys.time()
  edition <- config
  edition$lang <- lang
  edition$book$`site-url` <- sprintf("%s/%s", sub("/$", "", site_url), lang)
  edition$book$chapters <- lapply(config$book$chapters, babelquarto:::use_lang_chapter,
                                  language_code = lang, book_name = basename(stage),
                                  directory = dirname(stage))
  edition$book$appendices <- lapply(config$book$appendices, babelquarto:::use_lang_chapter,
                                    language_code = lang, book_name = basename(stage),
                                    directory = dirname(stage))
  edition$book$chapters <- lapply(edition$book$chapters, function(e) {
    if (is.list(e) && !is.null(e$chapters)) e$chapters <- as.list(e$chapters)
    e
  })
  edition$book$appendices <- as.list(edition$book$appendices)
  yaml::write_yaml(edition, config_file, handlers = yes_no)
  for (ch in chapters) {
    file <- paste0(ch, ".", lang, ".qmd")
    if (!file.exists(file.path(stage, file)))
      stop("no ", file, " in the book: name a chapter as its path without .qmd, like marks/point",
           call. = FALSE)
    metadata <- list("yes"); names(metadata) <- paste0("lang-", lang)
    withr::with_dir(stage, quarto::quarto_render(input = file, as_job = FALSE,
                                                 metadata = metadata, profile = lang))
    page <- file.path(out, lang, paste0(ch, ".", lang, ".html"))
    dir.create(dirname(page), recursive = TRUE, showWarnings = FALSE)
    file.copy(file.path(stage, "_book", paste0(ch, ".", lang, ".html")), page, overwrite = TRUE)
    for (other in c("en")) {
      babelquarto:::add_links(page, main_language = "en", language_code = other,
                              site_url = site_url, type = "book", config = config,
                              output_folder = out, path_language = lang, project_dir = stage)
    }
    babelquarto:::add_cross_links(page, path_language = lang, main_language = "en",
                                  config = config, site_url = site_url, output_folder = out)
  }
  cat(sprintf("rendered %s in %.1f minutes, into %s\n", paste(chapters, collapse = ", "),
              as.numeric(difftime(Sys.time(), started, units = "mins")), file.path(out, lang)))
  quit(save = "no")
}

started <- Sys.time()
babelquarto::render_book(stage, site_url = site_url, preview = FALSE)
cat(sprintf("rendered English and `%s` in %.1f minutes\n", lang,
            as.numeric(difftime(Sys.time(), started, units = "mins"))))

unlink(out, recursive = TRUE)
dir.create(out)
invisible(file.copy(list.files(file.path(stage, "_book"), full.names = TRUE, all.files = TRUE, no.. = TRUE),
                    out, recursive = TRUE))

# A folder has no page of its own: `ko/` holds `index.ko.html`, so a reader who
# types the folder's address is sent to it.
landing <- file.path(out, lang, "index.html")
if (!file.exists(landing) && file.exists(file.path(out, lang, paste0("index.", lang, ".html"))))
  writeLines(sprintf('<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=index.%s.html"><link rel="canonical" href="index.%s.html">', lang, lang), landing)
cat("editions written to", out, "\n")
