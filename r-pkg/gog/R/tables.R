# tables.R — the book's example tables, fetched by name.
#
# Not a word of the grammar, and deliberately so. This is the same category as
# `render_svg`: something the binding needs and the vocabulary does not, which
# is why `book/check_vocabulary.R` excludes both from the kernel block.
#
# It exists because every example in the manual begins with a table, and a
# reader who wants to run one should not have to write a CSV reader first. R
# gets off lightly here (`read.csv` does the work in one call), but JavaScript
# has no CSV parser in its standard library, so the same function there is
# thirty-three lines of quote handling — needed because one country in
# `gapminder_2007.csv` is "Congo, Dem. Rep." and its name holds a comma.
# Asking a reader to copy that before drawing anything is what moved this out
# of the book and into the four packages.
#
# The tables are not shipped. They are fetched from the book's own site, so one
# copy serves all four languages and nothing goes stale inside a tarball.
#
# The name carries the package's, and that is the whole of why it is no longer
# `book_table()`. This package and `god` are built to be loaded together, so
# `gog_table()` and `god_table()` stand side by side at a prompt and read as one
# idea in two spellings. They still differ by the one letter that separates the
# two projects everywhere else, so neither masks the other; what changed is that
# the difference is now a visible parallel rather than one name picked from an
# unrelated word.
#
# The old name is gone rather than deprecated. An alias would have been the
# careful move on a package with a readership, and this one does not have one
# yet: the window where a rename costs nobody anything is open now and closes
# for good. Two spellings of one function is a debt Law 3 would have carried
# until someone finally removed it, so it was not taken on.

book_data_url <- "https://psychometrician.github.io/gog-book/data/"

#' Read one of the book's example tables
#'
#' Fetches a table published beside the manual and returns it ready to plot.
#' The full list of names is in the book's data chapter.
#'
#' @param name The table's name without the extension, such as
#'   `"gapminder_2007"`.
#' @param text Columns that must stay text. A CSV records what a value is and
#'   never what kind of thing it is, so a column of `01`, `02`, `03` comes back
#'   as the numbers 1, 2, 3 unless it is named here.
#' @return A data frame.
#' @examples
#' \dontrun{
#' gapminder_2007 <- gog_table("gapminder_2007")
#' data(gapminder_2007) + point + x(gdp) + y(life)
#'
#' # `session` holds labels that look like numbers, so it stays text.
#' sessions <- gog_table("sessions", text = "session")
#' }
#' @export
gog_table <- function(name, text = character()) {
  if (!is.character(name) || length(name) != 1L || is.na(name)) {
    stop("gog: `gog_table()` takes one table name, as in ",
         "`gog_table(\"gapminder_2007\")`. The names are listed in the ",
         "book's data chapter.", call. = FALSE)
  }
  classes <- rep("character", length(text))
  names(classes) <- text
  utils::read.csv(paste0(book_data_url, name, ".csv"), colClasses = classes)
}
