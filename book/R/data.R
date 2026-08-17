# book/R/data.R
# The example data frames the book's chapters share — read from book/data/*.csv.
#
# Sourced by book/R/setup.R, in every chapter. It *reads*; it does not build. The frames are built by book/R/make-data.R,
# which is run by hand and is the only file that needs the `gapminder` package
# or R's `datasets`. Spec §20, "The cast is fetched, not shipped", has the
# ruling and the reasons.
#
# The point of the split is that the reader gets the same table the book drew.
# The CSVs are published with the site, so `data(gapminder_2007) + point +
# x(gdp) + y(life)` is reproducible from any of the four languages rather than
# only on the author's machine. See the "The book's data" chapter, which shows
# the same read in R, Python, Julia and JavaScript.
#
# CSV carries no types, so two things are restored here, and both are restored
# by every reader in every language — which is why the chapter shows them
# rather than hiding them:
#
#   * **Declared category order.** Ten columns are factors whose level order is
#     the point: the compass runs N, NE, E …, not alphabetically; the waterfall
#     runs Opening → Closing. Order lost is a plot silently rearranged, which
#     this project has already been bitten by once, in the Julia binding's
#     `Ordered` column.
#   * **Labels that look like numbers.** `census$age` is "0", "5", "10" …,
#     `gm_eras$era` is "1957", and `sessions$session` is "01". Every CSV reader
#     in every language turns those into numbers unless told not to, and "01"
#     comes back as 1. They are read as text on purpose.

# The book is rendered with the working directory set to the chapter's own
# folder, so the path is found rather than assumed: the search walks up until
# `gog-cli/` marks the repository root — the same marker setup.R uses, so the
# two cannot disagree about where they are. The walk starts at "." and goes up
# three levels, which is wider than `book/*/` needs; it stays that way because a
# caller sourcing this from anywhere in the tree is the case it was written for.
.gog_book_data <- local({
  for (up in c(".", "..", "../..", "../../..")) {
    if (dir.exists(file.path(up, "gog-cli")))
      return(normalizePath(file.path(up, "book", "data"), mustWork = TRUE))
  }
  stop("book/R/data.R: cannot find the repository root from ", getwd(),
       "\n  Looked for a `gog-cli/` directory at ., .., ../.. and ../../..")
})

# `chr` names the columns that must not be guessed. Everything else is left to
# read.csv, which gets numbers and text right on its own.
.gog_read <- function(name, chr = character()) {
  cc <- NA
  if (length(chr)) cc <- stats::setNames(rep("character", length(chr)), chr)
  read.csv(file.path(.gog_book_data, paste0(name, ".csv")),
           stringsAsFactors = FALSE, check.names = FALSE,
           colClasses = cc, na.strings = "")
}

# A factor whose level order is declared rather than discovered.
.gog_ordered <- function(x, levels) factor(x, levels = levels)

# -- gapminder ---------------------------------------------------------------
gm_all         <- .gog_read("gm_all")
gapminder_2007 <- .gog_read("gapminder_2007")
gapminder_asia <- .gog_read("gapminder_asia")
gm_continents  <- .gog_read("gm_continents")
gm_europe      <- .gog_read("gm_europe")

gm_eras <- .gog_read("gm_eras", chr = "era")
gm_eras$era <- .gog_ordered(gm_eras$era, c("1957", "2007"))

# -- Single-frame examples ---------------------------------------------------
iris_flowers  <- .gog_read("iris_flowers")
score_band    <- .gog_read("score_band")
medals        <- .gog_read("medals")
actuals       <- .gog_read("actuals")
forecast      <- .gog_read("forecast")
life_bands    <- .gog_read("life_bands")
gdp_threshold <- .gog_read("gdp_threshold")
gdp_rug       <- .gog_read("gdp_rug")
life_rug      <- .gog_read("life_rug")
milestones    <- .gog_read("milestones")
speed_target  <- .gog_read("speed_target")
quarterly     <- .gog_read("quarterly")
recessions    <- .gog_read("recessions")
target_band   <- .gog_read("target_band")
spending      <- .gog_read("spending")
commutes      <- .gog_read("commutes")
tide          <- .gog_read("tide")
day_cycle     <- .gog_read("day_cycle")
maunga_whau   <- .gog_read("maunga_whau")
thermals      <- .gog_read("thermals")
thermal_marks <- .gog_read("thermal_marks")

# -- The frames whose category order is the point ----------------------------
census <- .gog_read("census", chr = "age")
census$age <- .gog_ordered(census$age, as.character(seq(0, 85, by = 5)))

titanic <- .gog_read("titanic")
titanic$class    <- .gog_ordered(titanic$class, c("1st", "2nd", "3rd", "Crew"))
titanic$survived <- .gog_ordered(titanic$survived, c("Yes", "No"))

winds <- .gog_read("winds")
winds$direction <- .gog_ordered(winds$direction,
                                c("N", "NE", "E", "SE", "S", "SW", "W", "NW"))
winds$season <- .gog_ordered(winds$season, c("Summer", "Winter"))

listening <- .gog_read("listening")
listening$genre <- .gog_ordered(listening$genre,
                                c("Folk", "Jazz", "Techno", "Ambient"))

cashflow <- .gog_read("cashflow")
cashflow$step <- .gog_ordered(
  cashflow$step, c("Opening", "Sales", "Refunds", "Costs", "Tax", "Closing"))

sessions <- .gog_read("sessions", chr = "session")
sessions$session <- .gog_ordered(sessions$session, sprintf("%02d", 1:14))

ripples <- .gog_read("ripples")
ripples$tank <- .gog_ordered(ripples$tank, c("Shallow", "Deep"))

quakes_fiji <- .gog_read("quakes_fiji")
.edges <- seq(0, 720, by = 90)
quakes_fiji$slab <- .gog_ordered(
  quakes_fiji$slab,
  paste0(utils::head(.edges, -1), "-", .edges[-1], " km"))
rm(.edges)

# The world's coastlines and borders, one row per vertex. `piece` is the ring a
# vertex belongs to and is what `group()` splits on: a country is not always one
# closed shape, since islands are separate rings and a country wholly inside
# another is a ring of its own. It is text rather than a number because `group`
# takes a category, and a ring number is a name rather than a quantity.
world_borders <- .gog_read(
  "world_borders", chr = c("country", "continent", "piece"))

six_weeks <- .gog_read("six_weeks")
six_weeks$day     <- as.Date(six_weeks$day)
six_weeks$weekday <- .gog_ordered(
  six_weeks$weekday, c("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"))
six_weeks$week <- .gog_ordered(six_weeks$week, paste("Week", 1:6))

# The assertions that used to guard the construction still guard the read: a
# truncated or mis-parsed CSV fails here rather than in a plot.
stopifnot(
  nrow(gm_europe) == 30,
  length(unique(gapminder_asia$country)) == 5,
  nrow(quakes_fiji) == 1000,
  !anyNA(quakes_fiji$slab),
  !anyNA(census$age),
  !anyNA(winds$direction),
  !anyNA(six_weeks$day),
  length(unique(world_borders$country)) == 176,
  # Every ring closes on the vertex it started from, or the outline is drawn
  # with a gap in it and the map quietly looks broken.
  all(vapply(split(world_borders, world_borders$piece), function(p)
    p$lon[1] == p$lon[nrow(p)] && p$lat[1] == p$lat[nrow(p)], logical(1)))
)
