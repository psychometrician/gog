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
#   * **Declared category order.** Each column declared below with
#     `.gog_ordered()` is a factor whose level order is the point: the compass
#     runs N, NE, E …, not alphabetically; the waterfall runs Opening → Closing. Order lost is a plot silently rearranged, which
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
gm_asia        <- .gog_read("gm_asia")
gm_europe      <- .gog_read("gm_europe")
gm_populous    <- .gog_read("gm_populous")
asia_2007      <- .gog_read("asia_2007")

# -- What the extensions compute (coverage.qmd) ------------------------------
petal_brackets <- .gog_read("petal_brackets")
petal_stars    <- .gog_read("petal_stars")
lung_survival  <- .gog_read("lung_survival")
lung_at_risk   <- .gog_read("lung_at_risk")
news_counts    <- .gog_read("news_counts")
news_members   <- .gog_read("news_members")
news_links     <- .gog_read("news_links")
nyc_horizon    <- .gog_read("nyc_horizon")
gm_europe_cdf  <- .gog_read("gm_europe_cdf")

gm_eras <- .gog_read("gm_eras", chr = "era")
gm_eras$era <- .gog_ordered(gm_eras$era, c("1957", "2007"))

# Twelve Asian countries, in the three shapes the comparing-categories recipes
# draw. `rank` is text already ("1st"), and declared so that the bump chart's
# order does not rest on the file's row order; `year` in the two-year frame is a
# label, as `era` is above.
gm_twelve      <- .gog_read("gm_twelve")
gm_twelve$rank <- .gog_ordered(gm_twelve$rank, paste0(1:12, c("st", "nd", "rd", rep("th", 9))))
gm_twelve_2007 <- .gog_read("gm_twelve_2007")
gm_twelve_ends <- .gog_read("gm_twelve_ends", chr = "year")
gm_twelve_ends$year <- .gog_ordered(gm_twelve_ends$year, c("1952", "2007"))

# -- Single-frame examples ---------------------------------------------------
iris_flowers  <- .gog_read("iris_flowers")
score_band    <- .gog_read("score_band")
medals        <- .gog_read("medals")
actuals       <- .gog_read("actuals")
forecast      <- .gog_read("forecast")
life_bands    <- .gog_read("life_bands")
gdp_threshold <- .gog_read("gdp_threshold")
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
nutrients     <- .gog_read("nutrients")
thermals      <- .gog_read("thermals")
thermal_marks <- .gog_read("thermal_marks")
policy_rates  <- .gog_read("policy_rates")
inventory     <- .gog_read("inventory")

# -- The frames whose category order is the point ----------------------------
census <- .gog_read("census", chr = "age")
census$age <- .gog_ordered(census$age, as.character(seq(0, 85, by = 5)))

trade_partners <- .gog_read("trade_partners")

# A month of a household's money as links, the layered flow's table.
budget <- .gog_read("budget")

titanic <- .gog_read("titanic")
titanic$class    <- .gog_ordered(titanic$class, c("1st", "2nd", "3rd", "Crew"))
titanic$survived <- .gog_ordered(titanic$survived, c("Yes", "No"))

winds <- .gog_read("winds")
winds$direction <- .gog_ordered(winds$direction,
                                c("N", "NE", "E", "SE", "S", "SW", "W", "NW"))
winds$season <- .gog_ordered(winds$season, c("Summer", "Winter"))

# Florence Nightingale's table of the British army's deaths in the Crimean War,
# April 1854 to March 1856. Her year of the war starts in April, as her diagram
# does, and her three causes keep the order of her legend.
nightingale <- .gog_read("nightingale")
nightingale$month <- .gog_ordered(nightingale$month,
  c("Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec", "Jan", "Feb", "Mar"))
nightingale$cause <- .gog_ordered(nightingale$cause, c("Disease", "Wounds", "Other"))

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

# Two public records rather than tables written for the book, downloaded by
# fetch-data.R: NOAA's positions of every storm that reached hurricane strength
# from 1980 to 2025, and the US Geological Survey's earthquakes of 2011. The
# months run in calendar order and the categories in the order of the scale,
# and a CSV records neither.
cyclones <- .gog_read("cyclones")
cyclones$month    <- .gog_ordered(cyclones$month, month.abb)
cyclones$category <- .gog_ordered(cyclones$category, c("1 or 2", "3 or 4", "5"))

quakes_2011 <- .gog_read("quakes_2011")
quakes_2011$week <- as.Date(quakes_2011$week)
quakes_2011$age  <- .gog_ordered(
  quakes_2011$age, c("this week", "1 week ago", "2 weeks ago", "3 weeks ago"))

# The counties of the lower 48 states and DC, one row per vertex as in
# `world_borders`, with each county's 2023 poverty rate on every row of its
# outline, so a map needs no join. From the Census Bureau, by fetch-data.R.
us_counties <- .gog_read("us_counties", chr = c("county", "piece"))

# Ohio's 88 counties once for each of three presidential elections, with the
# percent of each county's voting-age citizens who voted on every row of its
# outline. From the Election Assistance Commission and the Census Bureau, by
# fetch-data.R. `election` is text, because a facet strip names it.
ohio_turnout <- .gog_read("ohio_turnout", chr = c("county", "piece", "election"))

six_weeks <- .gog_read("six_weeks")
six_weeks$day     <- as.Date(six_weeks$day)
six_weeks$weekday <- .gog_ordered(
  six_weeks$weekday, c("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"))
six_weeks$week <- .gog_ordered(six_weeks$week, paste("Week", 1:6))

# -- The chapters' own small tables ------------------------------------------
# Each of these was typed into the chunk that draws it until the chapter lost
# its Python, Julia and JavaScript tabs over it: a table written inside a chunk
# is a table only R can spell. Ten stayed behind in their chapters, where the
# typing is the lesson rather than the setup.
channel_sales     <- .gog_read("channel_sales")
team_trend        <- .gog_read("team_trend")
world_median      <- .gog_read("world_median")
flight            <- .gog_read("flight")
cities            <- .gog_read("cities")
routes            <- .gog_read("routes")
equator           <- .gog_read("equator")
population_spikes <- .gog_read("population_spikes")
capitals          <- .gog_read("capitals")
far_north         <- .gog_read("far_north")
departments       <- .gog_read("departments")
tenure            <- .gog_read("tenure")
coefs             <- .gog_read("coefs")
scrambled         <- .gog_read("scrambled")
botswana_arrow    <- .gog_read("botswana_arrow")
botswana_label    <- .gog_read("botswana_label")
spiral            <- .gog_read("spiral")
healthy_band      <- .gog_read("healthy_band")
income_note       <- .gog_read("income_note")
sales_box         <- .gog_read("sales_box")
prevailing_winds  <- .gog_read("prevailing_winds")
span_early        <- .gog_read("span_early")
span_middle       <- .gog_read("span_middle")
span_late         <- .gog_read("span_late")
target_edges      <- .gog_read("target_edges")
slump             <- .gog_read("slump")
banded            <- .gog_read("banded")
receipts          <- .gog_read("receipts")
depth_readings    <- .gog_read("depth_readings")
octaves           <- .gog_read("octaves")
decay             <- .gog_read("decay")
medal_repeats     <- .gog_read("medal_repeats")
drawdown          <- .gog_read("drawdown")
mixed_signs       <- .gog_read("mixed_signs")

# The two that carry a moment rather than a number, restored the way
# `six_weeks$day` is. A CSV holds the text; the class is put back here, and
# `monitoring` is read in UTC because that is the zone it was written in.
revenue <- .gog_read("revenue")
revenue$day <- as.Date(revenue$day)

monitoring <- .gog_read("monitoring")
monitoring$at <- as.POSIXct(monitoring$at, tz = "UTC")

# The assertions that used to guard the construction still guard the read: a
# truncated or mis-parsed CSV fails here rather than in a plot.
stopifnot(
  nrow(gm_europe) == 30,
  nrow(gm_populous) == 10,
  nrow(gm_twelve) == 144, nrow(gm_twelve_2007) == 12, nrow(gm_twelve_ends) == 24,
  !anyNA(gm_twelve$rank), !anyNA(gm_twelve_ends$year),
  length(unique(gapminder_asia$country)) == 5,
  length(unique(gm_asia$country)) == 33,
  nrow(quakes_fiji) == 1000,
  !anyNA(quakes_fiji$slab),
  !anyNA(census$age),
  !anyNA(winds$direction),
  !anyNA(six_weeks$day),
  !anyNA(revenue$day),
  !anyNA(monitoring$at),
  inherits(monitoring$at, "POSIXct"),
  nrow(population_spikes) == 120,
  length(unique(cyclones$storm)) == 2174,
  !anyNA(cyclones$month), !anyNA(cyclones$category),
  sum(quakes_2011$age == "this week") == 2701,
  !anyNA(quakes_2011$week), !anyNA(quakes_2011$age),
  length(unique(world_borders$country)) == 176,
  # Every ring closes on the vertex it started from, or the outline is drawn
  # with a gap in it and the map quietly looks broken.
  all(vapply(split(world_borders, world_borders$piece), function(p)
    p$lon[1] == p$lon[nrow(p)] && p$lat[1] == p$lat[nrow(p)], logical(1)))
)
