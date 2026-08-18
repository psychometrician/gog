# book/R/make-data.R — the generator. Run it by hand; nothing sources it.
#
#     Rscript book/R/make-data.R
#
# This file *builds* the book's example frames and writes them to `book/data/`
# as CSV. `book/R/data.R`, which the chapters and the notebook do source, only
# reads those CSVs back. The split is spec §20, "The cast is fetched, not
# shipped": one canonical CSV per frame, published with the book, so a reader in
# any of the four languages can fetch the same table the manual drew.
#
# Until 2026-07-28 this file *was* `data.R` and ran on every render, which is
# why a reader who installed gog could not reproduce the manual's first
# sentence: `gapminder_2007` existed only inside an `include: false` chunk on
# the author's machine.
#
# Two dependencies live here and nowhere else, which is the point of the split:
# `gapminder`, and R's own `datasets` for iris/volcano/quakes. The book no
# longer needs either at render time.
#
# A second copy of `medals` living in the notebook would drift from the prose
# that describes it, and the drift would be silent — which is the same failure
# `gapminder_asia` already had once, below.

# -- gapminder (2007 snapshot, readable column names) -----------------------
library(gapminder, warn.conflicts = FALSE, quietly = TRUE)

gm_all <- as.data.frame(gapminder::gapminder)
gm_all$continent  <- as.character(gm_all$continent)
gm_all$country    <- as.character(gm_all$country)
names(gm_all)[names(gm_all) == "gdpPercap"] <- "gdp"
names(gm_all)[names(gm_all) == "lifeExp"]   <- "life"
names(gm_all)[names(gm_all) == "pop"]       <- "population"

gapminder_2007 <- gm_all[gm_all$year == 2007, ]

# -- Two eras, side by side (dodge / grouped examples) ----------------------
# A second *categorical* dimension over the same continents, so a color split
# has something to set beside itself. `era` is a factor, not the numeric `year`,
# so the split is discrete (a palette, not a ramp) — exactly what `dodge` sorts
# out into side-by-side groups.
gm_eras <- gm_all[gm_all$year %in% c(1957, 2007), ]
gm_eras$era <- factor(gm_eras$year)

# -- Three continents' life expectancy over time (ribbon / spread bands) ----
# A ribbon's `range` reduces the many countries in a continent-year to a low and
# a high, so a band needs several observations per (year, group) — a lone series
# would collapse to a zero-width band. Three continents keep the split legible.
gm_continents <- gm_all[gm_all$continent %in% c("Americas", "Europe", "Asia"), ]

# -- Europe in 2007 (the dot plot: one continuous column at small n) --------
# Thirty rows, which is the size the dot plot exists for — few enough that a
# histogram's bin width invents or hides structure and a density estimate is
# guessing, yet every observation still fits on the page as its own dot. The
# whole-world frame (142) makes a handsome dot plot too, but it cannot make the
# argument, because at 142 rows the summaries are fine.
gm_europe <- gapminder_2007[gapminder_2007$continent == "Europe", ]
stopifnot(nrow(gm_europe) == 30)

# -- Five Asian countries over time (line chart examples) -------------------
# gapminder spells it "Korea, Rep."; asking for "South Korea" matched nothing
# and this frame quietly held four countries while claiming five.
asia_five <- c("China", "India", "Japan", "Korea, Rep.", "Indonesia")
gapminder_asia <- gm_all[gm_all$country %in% asia_five, ]
stopifnot(length(unique(gapminder_asia$country)) == length(asia_five))

# -- iris (3-D scatter: three comparable measures + a category) -------------
# The textbook 3-D dataset: three flower measurements on one scale (cm) so no
# axis dwarfs another, and a species that separates cleanly in space. gapminder
# would skew a linear z — one 1.3-billion-person bar and everything else on the
# floor — which is a lesson for a later log-z, not a first 3-D plot. Renamed to
# bare, plain column names so the atoms read `x(petal_length)`, not backticks.
iris_flowers <- data.frame(
  sepal_length = iris$Sepal.Length,
  sepal_width  = iris$Sepal.Width,
  petal_length = iris$Petal.Length,
  species      = as.character(iris$Species)
)

# -- A pre-computed measurement band (ribbon * bounds / dash examples) -------
# What a scoring model hands you: per raw score, an expected true score and a
# conditional SEM band around it -- already computed upstream, so the plot only
# *draws* it (gog is not the stats package). A stand-in CSEM that bulges at
# mid-scale and tightens at the extremes, as real ones do.
.score <- 0:20
.csem  <- 1.5 * sqrt(pmax(.score * (20 - .score) / 20, 0.3))
score_band <- data.frame(
  score    = .score,
  expected = .score,
  lower    = .score - 1.96 * .csem,
  upper    = .score + 1.96 * .csem
)
rm(.score, .csem)

# -- Medal counts (bar chart examples) --------------------------------------
medals <- data.frame(
  country = c("USA", "China", "Great Britain", "Russia", "Germany"),
  gold    = c(46.0, 38.0, 29.0, 19.0, 17.0),
  silver  = c(37.0, 31.0, 17.0, 18.0, 10.0),
  bronze  = c(38.0, 22.0, 19.0, 9.0, 15.0)
)

# -- Two cities by five-year age band (population pyramid) -------------------
# Illustrative rather than the census: two inner-city profiles with the shape
# those actually have — a spike through the twenties where people move in for
# work and study, a dip through the school years, and a long thin tail. Written
# out as literals so the book and the three parity harnesses count the same
# people.
#
# `population` is **positive here**, for both cities. The pyramid's mirror comes
# from negating one of them, and that negation is host arithmetic done in the
# chapter, where the reader can see it — the same call `cashflow` makes about a
# running total. Shipping the column pre-negated would hide the one step in the
# recipe worth explaining.
#
# `age` is a factor because a band is a name, not a number: "0" is the label of
# everyone under five, not the position 0. Alphabetical, the bands read 0, 10,
# 15, 5, which is a pyramid with its floors shuffled.
.bands <- as.character(seq(0, 85, by = 5))
census <- data.frame(
  age  = factor(rep(.bands, 2), levels = .bands),
  city = rep(c("Busan", "Seoul"), each = length(.bands)),
  population = c(
    # Busan, the smaller of the two, and the one the chapter negates
    5200, 3100, 2600, 9000, 38500, 38900, 26000, 15200, 8700,
    6800, 6300, 5800, 5100, 4200, 3400, 2200, 1300, 900,
    # Seoul
    9300, 5100, 3800, 7600, 32000, 45800, 38300, 25200, 15800,
    12800, 11000, 10900, 10400, 9100, 7500, 6400, 3900, 2600
  )
)
rm(.bands)

# -- Multi-table example (actuals + forecast) --------------------------------
set.seed(42)
actuals <- data.frame(
  year  = 2019:2023,
  sales = c(120.0, 135.0, 128.0, 152.0, 168.0)
)
forecast <- data.frame(
  year  = 2024:2026,
  sales = c(180.0, 195.0, 210.0)
)

# -- Wind observations (polar examples) --------------------------------------
# The rosa ventorum is the oldest circular graphic there is, and Wilkinson's own
# example for polar coordinates (§9.1.6). Compass direction is the textbook
# variable a straight axis misreports: on a line, north-west and north-north-west
# sit at opposite ends with the whole compass between them, when they are
# neighbors. `direction` is a factor so its eight points keep compass order
# rather than alphabetical; `bearing` is the same observation in degrees, for the
# cases that want a measured angle instead of a named one.
set.seed(1443)
.compass  <- c("N", "NE", "E", "SE", "S", "SW", "W", "NW")
.n_by_dir <- c(14, 22, 41, 33, 18, 47, 63, 26)
winds <- data.frame(
  direction = factor(rep(.compass, .n_by_dir), levels = .compass),
  bearing   = rep(seq(0, 315, by = 45), .n_by_dir) + runif(sum(.n_by_dir), -22, 22),
  speed     = round(rep(c(9, 11, 14, 12, 8, 17, 21, 13), .n_by_dir) +
                      rnorm(sum(.n_by_dir), 0, 2.5), 1),
  season    = factor(sample(c("Summer", "Winter"), sum(.n_by_dir), replace = TRUE))
)
winds$bearing <- winds$bearing %% 360
rm(.compass, .n_by_dir)

# -- One day, around the clock (periodic-axis examples) ----------------------
# Hourly readings that come back to where they started: the count at 24:00 *is*
# the count at 00:00, and both endpoints are present because both were measured.
day_cycle <- data.frame(
  hour  = 0:24,
  trips = c(31, 18, 11,  8,  9, 17, 44, 92, 141, 118, 96, 88,
            94, 90, 85, 97, 126, 158, 149, 112, 84, 66, 55, 42, 31)
)

# The same day, sampled rather than counted: a tide gauge reading every three
# hours. It reaches neither midnight, which is the ordinary case — most periodic
# data is a sample of its cycle rather than a complete turn of it. `day_cycle`'s
# two endpoints used to be the only way to close a circle, and `x(hour, limits =
# c(0, 24))` is now the general one, so this table exists to show the difference
# rather than to work around it.
tide <- data.frame(
  hour   = seq(1, 22, by = 3),
  height = c(2.1, 4.4, 3.6, 1.2, 0.9, 2.8, 4.7, 3.9)
)

# -- Six weeks of daily orders (calendar-axis examples) ----------------------
# Promoted from a scales.qmd local so the cookbook and the scales chapter read
# the same table — a second copy would drift silently (see `medals` above).
six_weeks <- data.frame(
  day    = as.Date("2024-03-01") + 0:41,
  orders = round(20 + 8 * sin(0:41 / 5) + (0:41 %% 7))
)

# -- Weekly plays by genre (the streamgraph) --------------------------------
# Four series over half a year, each rising and falling at a different time.
# That last property is the whole reason this table exists rather than one of
# the others: a stacked area is hardest to read when the bands take turns, since
# every band above the floor then carries the movement of the ones below it as
# well as its own, and `stack(baseline = "wiggle")` is the layout that answers
# exactly that. `gm_all`'s populations only ever grow, so all three baselines
# draw nearly the same picture there and would demonstrate nothing.
#
# Deterministic: a fixed seed, and the noise is small next to the humps, so the
# shape a reader sees is the formula's rather than one draw's.
set.seed(7)
.weeks <- 1:26
.hump <- function(peak, height) {
  round(pmax(2, height * exp(-((.weeks - peak)^2) / 60) + 6 + rnorm(26, 0, 1.2)), 1)
}
listening <- data.frame(
  week  = rep(.weeks, 4),
  genre = factor(rep(c("Folk", "Jazz", "Techno", "Ambient"), each = 26),
                 levels = c("Folk", "Jazz", "Techno", "Ambient")),
  plays = c(.hump(4, 34), .hump(11, 26), .hump(18, 40), .hump(23, 22))
)
rm(.weeks, .hump)

# Two *categorical* readings of the same date, for the calendar heatmap — the
# tile plot's plainest case, where the cells are a grid someone already lives by.
# Derived here rather than in the chapter so the chunk shows the grammar and not
# a paragraph of date arithmetic; the arithmetic is the host's job either way,
# which is the same division `x(sin(t))` is refused under.
#
# Both are **factors**, because both have an order that is not alphabetical and
# is not the order they happen to appear in. Monday..Sunday is the whole point of
# a calendar's rows, and "Week 2" sorts before "Week 10" only if you say so.
.wd <- format(six_weeks$day, "%a")
six_weeks$weekday <- factor(.wd, levels = c("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"))
.wk <- paste("Week", (as.integer(six_weeks$day - min(six_weeks$day)) %/% 7) + 1)
six_weeks$week <- factor(.wk, levels = unique(.wk))
rm(.wd, .wk)
stopifnot(nlevels(six_weeks$weekday) == 7, nlevels(six_weeks$week) == 6)

# -- Thresholds to draw a `rule` at -----------------------------------------
# A rule's position is a *column*, never a bare number, which is the rule every
# atom that takes a column keeps. What it buys is here: one table draws three
# reference lines at once, and a second column colors them.
#
# Each frame holds **one** of the plot's position columns and nothing else, and
# that is not tidiness — it is how a rule knows which axis it sits on. Handed a
# table with both, the grammar has nothing to read the orientation off and
# refuses (see the Rule chapter's last section).
life_bands <- data.frame(
  life = c(60.0, 70.0, 80.0),
  band = c("Low", "Middle", "High")
)

# The same idea on the other axis: a single income threshold, so the rule stands
# up where the last one lay down. $10,000 a head is roughly where gapminder's
# 2007 cloud stops climbing steeply and starts flattening.
gdp_threshold <- data.frame(gdp = 10000.0)

# A rug is not a different mark, only a different reach, so its table is the same
# shape as a threshold's: one position column, one row per observation. Two
# frames, not one, because which axis a rule lands on is read off which of the
# plot's position columns its table holds — a table holding `gdp` and `life` at
# once answers both, and is refused for saying nothing about which is meant. The
# layer can now say so itself (`rule + x(gdp)`), so that refusal is the *silent*
# case rather than the only one; two single-column frames stay the tidier way in.
gdp_rug  <- data.frame(gdp  = gapminder_2007$gdp)
life_rug <- data.frame(life = gapminder_2007$life)

# -- A note to write on a plot (per-layer position examples) ------------------
# Deliberately spelled in its own vocabulary: `at`/`value` where the base table
# says `gdp`/`life`. That is the whole point of the example — the note is on the
# same two axes, in the same units, and differs only in what its columns are
# called, which is what a second table nearly always looks like when you did not
# write it yourself.
milestones <- data.frame(
  at    = c(10000.0, 40000.0),
  value = c(72.0, 80.0),
  note  = c("income takes off", "the long plateau")
)

# A target to ring the wind rose with (polar examples). One column, `speed`,
# which is the *radial* axis there — so the rule spans the turn and closes.
speed_target <- data.frame(speed = 15.0)

# -- Zones to shade (rectangle / highlight examples) -------------------------
# A zone's sides are *columns*, so one row is one rectangle and one table draws
# several — the same property that lets one `rule` table draw several lines.
#
# `recessions` bounds only the **domain** axis, so the zones span the panel's
# full height however the measure axis is scaled. That is what a `ribbon` cannot
# do: it would have to be told a top and a bottom, and any number you pick either
# falls short of the panel or widens the axis to reach it.
quarterly <- data.frame(
  year  = 2005:2023,
  sales = c(95, 104, 118, 112, 88, 97, 110, 121, 133,
            140, 152, 160, 171, 178, 186, 131, 141, 168, 190)
)
recessions <- data.frame(
  start = c(2007.9, 2020.0),
  end   = c(2009.5, 2020.9),
  slump = c("Financial crisis", "Pandemic")
)

# The measure pair on its own: a target corridor spanning the full width.
target_band <- data.frame(lower = 150.0, upper = 175.0)

# -- A rectangle that is the data, not the highlight (finance examples) ------
# The tables above shade a region *behind* a plot. These two are the other
# reading of the same mark: the rectangle is the measurement, and there is
# nothing underneath it.
#
# `cashflow` carries only the **changes**, never the running total, and that is
# the point rather than an omission. gog accumulates *within* a position — one
# pile per slot, which is `stack` — and has no transform that accumulates
# *across* positions, so the running total is arithmetic the host does before
# the plot begins, exactly as a correlation matrix is. The chapter shows the one
# line that does it.
#
# The step is a factor so the axis keeps the order the money moved in; sorted
# alphabetically a waterfall is not wrong, it is meaningless.
#
# `total` marks the two bars that stand on the floor rather than on the bar
# before them. It is a column rather than a rule the plot infers, because which
# steps are subtotals is a fact about the accounts and nothing in the numbers
# says it.
cashflow <- data.frame(
  step  = factor(c("Opening", "Sales", "Refunds", "Costs", "Tax", "Closing"),
                 levels = c("Opening", "Sales", "Refunds", "Costs", "Tax", "Closing")),
  delta = c(120.0, 45.0, -18.0, -32.0, -14.0, 0.0),
  total = c(TRUE, FALSE, FALSE, FALSE, FALSE, TRUE)
)

# Fourteen trading sessions, open/high/low/close. Illustrative rather than
# logged — a random walk with the four numbers in the relation they always hold
# (`low` <= min(open, close), `high` >= max) — but written out as literals so the
# book and the three parity harnesses draw the same fourteen candles.
#
# The session is a factor for the reason the step is: a categorical axis puts the
# sessions at even spacing, which is what a trader's chart does with the weekend.
sessions <- data.frame(
  session = factor(sprintf("%02d", 1:14), levels = sprintf("%02d", 1:14)),
  open  = c(100.39, 99.41, 101.02, 102.09, 105.03, 106.28, 103.97,
            103.59, 107.00, 110.20, 111.22, 111.25, 111.94, 111.85),
  close = c(100.45, 99.72, 103.11, 102.01, 104.85, 105.77, 106.74,
            103.88, 109.35, 112.52, 112.28, 110.74, 114.20, 113.49),
  high  = c(100.66, 99.93, 104.19, 102.98, 105.71, 106.36, 107.77,
            104.98, 109.62, 113.27, 112.37, 111.86, 115.16, 114.66),
  low   = c(99.98, 98.66, 100.53, 101.60, 103.86, 104.94, 103.58,
            103.06, 106.68, 110.04, 110.12, 109.89, 111.25, 110.75)
)

# -- A household budget, as a tree (sunburst / icicle examples) --------------
# Every other table here is flat: one row is one observation, and each column
# says something about that row. This one is a **tree** flattened into a table,
# because a table is the only shape gog has for one. A row is a leaf, something
# money was actually spent on, and its first three columns are the path from the
# root down to it, so `Housing / Utilities / Energy` and `Housing / Rent` are
# branches of different lengths off the same trunk.
#
# `detail` is NA where a branch stops at the second level, and that is the
# ordinary case rather than a gap in the data: rent has no sub-parts worth
# naming. The chapter reads it as "no third ring here" and leaves the outer ring
# blank across those wedges, which is what gives a real sunburst its ragged rim.
#
# **Only the leaves carry an amount.** Every interior total is the sum of the
# leaves below it, computed in the chapter, because a table carrying both would
# hold two sources of truth for one number with nothing to say which won.
#
# Row order is drawing order. gog reads no meaning into a tree, having none, so
# the sweep round the circle is the order the rows arrive in: grouped by branch,
# and largest first inside each branch, which is the arrangement that puts the
# big wedges together instead of scattering them round the dial. Sorting the
# table differently redraws the plot, and nothing warns you, because to the
# engine these are twelve unrelated rows.
spending <- data.frame(
  group  = c(rep("Housing", 4), rep("Food", 2), rep("Transport", 3),
             rep("Leisure", 3)),
  item   = c("Rent", "Utilities", "Utilities", "Repairs",
             "Groceries", "Eating out",
             "Car", "Car", "Season ticket",
             "Holidays", "Hobbies", "Subscriptions"),
  detail = c(NA, "Energy", "Water", NA,
             NA, NA,
             "Fuel", "Insurance", NA,
             NA, NA, NA),
  amount = c(980, 140, 35, 120,
             420, 185,
             130, 75, 110,
             160, 90, 45)
)

# -- How four cities get to work (the mosaic) --------------------------------
# Two categories and a count, which is the shape a mosaic is read from: `city`
# divides the width in proportion to how many people were surveyed there, and
# `mode` divides each of those columns by how that city splits.
#
# The sample sizes are deliberately uneven — 500 in Millbrook against 1,540 in
# Ashford — because that is the whole reason a mosaic exists rather than four
# stacked bars side by side. Equal-width bars would show the four splits and say
# nothing about how much each one is worth knowing; here the narrow column is
# narrow, and a reader can see at a glance that Millbrook's striking cycling
# share rests on a third of Ashford's evidence.
#
# The smallest column is still wide enough to hold its own name, which is not a
# detail: the chapter labels the columns with a `text` layer, and a label wider
# than the column it names is the plot arguing with itself.
#
# One row per cell, and every count on a leaf: an interior total would be a
# second source of truth for a number the partition already computes.
commutes <- data.frame(
  city = rep(c("Ashford", "Brightwell", "Calder", "Millbrook"), each = 4),
  mode = rep(c("Car", "Bus or tram", "Bicycle", "On foot"), 4),
  people = c(
    980, 340, 110, 110,   # Ashford — a big, car-shaped city
    420, 380, 130,  70,   # Brightwell — buses carry nearly as many as cars
    260, 120, 240, 130,   # Calder
    120,  60, 210, 110    # Millbrook — small, and mostly on two wheels
  )
)

# -- Two gliders circling up one thermal (3-D route examples) ----------------
# A `path` in `space` is a *route*, and the reading order is the table's order,
# which here is time. Illustrative rather than logged: two gliders share a
# thermal half a turn apart and climb at slightly different rates, so the coils
# thread through each other instead of sitting side by side. That is the case a
# depth sort has to get right, and the case a per-stroke sort gets wrong.
.turns <- seq(0, 3.25 * 2 * pi, length.out = 170)
thermals <- rbind(
  # The wide one, circling the edge of the lift and climbing slowly.
  data.frame(
    east     = 330 * cos(.turns),
    north    = 330 * sin(.turns),
    altitude = 900 + .turns * 58,
    glider   = "Alpha"
  ),
  # The tight one, centered better on the core: a smaller circle, a faster climb.
  # Different radii as well as different phases, so the two coils genuinely pass
  # in front of and behind each other rather than sitting in parallel rings.
  data.frame(
    east     = 165 * cos(.turns + pi),
    north    = 165 * sin(.turns + pi),
    altitude = 900 + .turns * 79,
    glider   = "Bravo"
  )
)
# When the samples were taken, so the route can also be *flown* rather than only
# traced. `path` reads the table's order and draws the whole flight at once;
# `play(second)` advances through the same order and draws where the glider is.
# One column, two readings, which is the pair the play chapter is built on.
thermals$second <- rep(seq(0, by = 2, length.out = length(.turns)), 2)
rm(.turns)

# The marker that climbs the route above, sampled every 20 seconds so a sequence
# is 17 frames rather than 170.
#
# **Its time column is called `instant`, not `second`, and that is not tidiness.**
# The engine builds the frame list from *every source table carrying the played
# column*, not only from the layer that bound `play` — one plot, one sequence, on
# the same rule that gives it one scale. So naming this column `second` too would
# hand the backdrop's 170 distinct seconds to the sequence and draw 170 frames
# instead of 17. `milestones` above makes the same argument for a different
# reason; this is where it has teeth.
thermal_marks <- thermals[thermals$second %% 20 == 0, c("east", "north", "altitude", "glider")]
thermal_marks$instant <- thermals$second[thermals$second %% 20 == 0]

# -- One ripple tank, two depths (the cube, the panels and the frames at once) --
# The only four-dimensional table in this file: a height over a floor, in two
# tanks, at twelve instants. It exists because the three things the book teaches
# last — the third position, the panels and the frames — have no shared example
# otherwise, and a sentence spending all three at once is worth being able to
# write.
#
# Illustrative rather than measured, like `thermals` and `winds`, but *computed*
# rather than sampled, so there is no `set.seed` here: every number is a value of
# one formula and the same formula draws the same tank every time.
#
# A dipper taps one corner of each tank at one fixed rate, and the arc it makes
# travels outward at the water's own speed. The tanks differ only in depth, and
# depth is what sets that speed: from identical tapping, the shallow tank's
# crests sit 140 mm apart and the deep tank's 220 mm. So the facet compares two
# wavelengths against one shared height scale, and what moves is the arcs leaving
# the corner.
#
# **One shared period is what closes the loop.** Both tanks are driven at the same
# rate, so twelve instants cover exactly one tap in either, and the twelfth frame
# is one step before the first repeats. Tanks with different *periods* would need
# a common multiple of frames before the sequence came back to where it started,
# and the seam would show on every loop.
#
# **Three things keep the sheet from spiking where the dipper is**, and each is a
# way of not dividing by a radius:
#   1. The envelope is 1 / (1 + (r/260)^2), which is finite at r = 0. Real
#      cylindrical spreading falls off as 1/sqrt(r), is infinite at the source,
#      and draws as one node shot through the lid of the cube.
#   2. The dipper sits at a *half-cell offset*, so no node lands on it. The nodes
#      nearest it are then all the same distance away and so all the same height,
#      where a node exactly on it would stand alone.
#   3. The wavelength is at least *six node spacings*. At six, neighboring nodes
#      are 60 degrees apart in phase and swing the full amplitude between them;
#      below six the mesh aliases and the sheet reads as noise rather than as a
#      wave. 140 mm at 20 mm spacing is seven, and 220 mm is eleven.
#
# The corner rather than the middle, for reach: centered, the radius spans seven
# node spacings and the tank holds one and a half wavelengths. From the corner it
# spans twenty, so the shallow tank shows 2.8 arcs against the deep tank's 1.8, at
# the same size and the same cost.
#
# 15 x 15 nodes at 12 instants is 5,400 rows and 4,704 faces, about 700 KB of SVG.
# Sixteen instants is 960 KB for a picture that reads the same, and one `<path>`
# per face is a cost `maunga_whau` already asks to be considerate with — here paid
# once per frame per panel.
.node  <- seq_len(15) * 20                       # mm across the tank floor
.floor <- expand.grid(across = .node, along = .node)
.r     <- sqrt((.floor$across - 10)^2 + (.floor$along - 10)^2)
.tank  <- function(name, wavelength) do.call(rbind, lapply(1:12, function(i)
  data.frame(
    across  = .floor$across,
    along   = .floor$along,
    height  = round(6 / (1 + (.r / 260)^2) *
                      sin(2 * pi * (.r / wavelength - (i - 1) / 12)), 3),
    tank    = name,
    instant = i
  )))
ripples <- rbind(.tank("Shallow", 140), .tank("Deep", 220))
# Shallow first: the reading runs from the busier sheet to the calmer one, where
# alphabetically it would be "Deep". A factor is how a table says so (`census`).
ripples$tank <- factor(ripples$tank, levels = c("Shallow", "Deep"))
stopifnot(
  nrow(ripples) == 15 * 15 * 12 * 2,
  # A complete grid per (tank, instant), which is what a `surface` needs: a face
  # spans four nodes, and a missing node opens a hole in the sheet.
  all(table(ripples$tank, ripples$instant) == 225),
  # Nothing sits on the dipper, so nothing divides by zero and nothing stands alone.
  min(.r) > 0
)
rm(.node, .floor, .r, .tank)

# A `surface` needs one row per (x, y) crossing, and the canonical grid in R is
# `volcano` — Maunga Whau (Mt Eden), digitized from a topographic map by Ross
# Ihaka and shipped with base R, so this table needs no dependency.
#
# The help page settles which index is which, and it is worth getting right rather
# than guessing: "rows corresponding to grid lines running east to west and
# columns to grid lines running south to north." A row is therefore a line of
# constant *northing*, so the row index is north and the column index is east.
#
# Kept as every second grid line, which is a 20 m spacing rather than the source's
# 10 m: 44 x 31 nodes is 1290 faces, where the whole matrix would be 5160. The
# terrain still reads at this spacing, and a mark that emits one `<path>` per face
# is worth being considerate with. `expand.grid` is deliberately *not* used — it
# varies its first argument fastest and `as.vector` varies the matrix's row index
# fastest, so pairing the two silently transposes the map.
.v <- datasets::volcano[seq(1, nrow(datasets::volcano), by = 2),
                        seq(1, ncol(datasets::volcano), by = 2)]
maunga_whau <- data.frame(
  east      = rep(seq_len(ncol(.v)) * 20, each = nrow(.v)),
  north     = rep(seq_len(nrow(.v)) * 20, times = ncol(.v)),
  elevation = as.vector(.v)
)
rm(.v)

# -- Where the earthquakes are, under Fiji (a cube of real measurement) -------
# `datasets::quakes`: 1,000 events recorded near Fiji, each with a position on
# the globe and a depth below it. Base R again, so no dependency, and it is the
# counterweight the 3-D chapters need — `ripples` is a formula and `thermals` is
# an illustration, where every number here was measured.
#
# The shape is the point. These are not scattered through the volume: they lie on
# a sheet that dives away from the trench, which is the Tonga-Kermadec slab going
# down under the Australian plate. It is a structure a reader can *see* in the
# cube and cannot see in any pair of the three columns, which is the argument for
# the third position made by data rather than by prose.
#
# `elevation` rather than `depth`, negated, for the same reason `maunga_whau`
# uses that word: `z` runs up, so a number that means "below sea level" has to be
# negative or the slab is drawn upside down. The vocabulary is shared with the
# volcano deliberately — one word for height above the datum, whichever side of
# it the measurement falls.
quakes_fiji <- data.frame(
  east      = datasets::quakes$long,
  north     = datasets::quakes$lat,
  elevation = -datasets::quakes$depth,
  magnitude = datasets::quakes$mag
)
# Depth in 90 km bands, ordered shallow to deep. This is what `play` advances
# through in the cookbook, and it is there to make one point: the frames run
# along *any* ordered column, and this one is not time. A factor fixes the order,
# because the labels are text and text otherwise runs alphabetically.
.edges <- seq(0, 720, by = 90)
quakes_fiji$slab <- cut(
  datasets::quakes$depth, breaks = .edges, include.lowest = TRUE,
  labels = paste0(utils::head(.edges, -1), "-", .edges[-1], " km")
)
stopifnot(
  nrow(quakes_fiji) == 1000,
  # Every event lands in a band: the deepest is 680 km, so 720 covers them all.
  !anyNA(quakes_fiji$slab)
)
rm(.edges)

# -- What eight pantry staples are made of (the cluster tree) ----------------
# One row is one (food, nutrient) pair, the long form a tile plot reads
# directly and `cluster` profiles over. Literal values, roughly per 100 g as
# commonly served, chosen so the tree finds groups a reader already believes:
# the two grains, the two legumes, the two animal proteins, and almonds off on
# their own where the fat column puts them. Nothing is standardized — that the
# large columns weigh most is part of what the chapter teaches.
#
# The arrival order interleaves those groups on purpose. The chapter's first
# tile plot draws the slots in arrival order to show an order nothing chose,
# and an arrival order that already grouped the foods would show no such
# thing — the un-clustered picture must have a visible problem for the
# clustered one to fix.
nutrients <- local({
  foods <- c("salmon", "rice", "almonds", "lentils",
             "chicken", "spinach", "beans", "oats")
  kinds <- c("protein", "fat", "carbs", "fiber", "iron")
  amounts <- c(
    25.4, 12.4,  0.0,  0.0, 0.5,   # salmon
     2.7,  0.3, 23.0,  1.8, 0.5,   # rice
    21.2, 49.9, 21.6, 12.5, 3.7,   # almonds
     9.0,  0.4, 20.0,  7.9, 3.3,   # lentils
    31.0,  3.6,  0.0,  0.0, 1.0,   # chicken
     2.9,  0.4,  3.6,  2.2, 2.7,   # spinach
     8.9,  0.5, 23.7,  8.7, 2.1,   # beans
     2.5,  1.5, 18.0,  1.7, 0.9    # oats
  )
  data.frame(
    food     = rep(foods, each = length(kinds)),
    nutrient = rep(kinds, times = length(foods)),
    amount   = amounts
  )
})

# ---------------------------------------------------------------------------
# Write every frame to book/data/*.csv
#
# A double is written at the shortest precision that still parses back to the
# same value, rather than at a flat 17 significant digits. Flat 17 renders 5.1
# as 5.0999999999999996 and costs the format the readability and the clean
# diffs that chose CSV in the first place.
#
# 31 of 35 frames round-trip bit-exactly. The other four carry values from
# `runif`/`rnorm` where R's own decimal parsing does not recover the last bit at
# any precision — measured: %.17g through %.21g all fail, and more digits made
# it worse. That was chased far enough to establish it does not matter: the two
# worst frames, `winds` (25 values) and `thermals` (134), render **byte-
# identical** SVG from the CSV, because a coordinate is printed to two decimals
# and a one-ULP difference cannot reach it. The standard here is the plot, not
# the decimal.
# ---------------------------------------------------------------------------

.gog_shortest <- function(v) {
  vapply(v, function(x) {
    if (is.na(x)) return(NA_character_)
    s <- NULL
    for (d in 15:17) {
      cand <- format(x, digits = d, trim = TRUE, scientific = FALSE)
      if (identical(as.numeric(cand), x)) { s <- cand; break }
    }
    if (is.null(s)) s <- format(x, digits = 17, trim = TRUE, scientific = FALSE)
    # A whole double keeps its point. Without this `120.0` writes as "120" and
    # every CSV reader in every language infers an integer column, so a double
    # silently becomes an int on the way back — 21 of the 35 frames changed type
    # that way before this line existed. The point is the column's type,
    # written down where the file can carry it, and it costs one character.
    if (!grepl("[.eE]", s)) s <- paste0(s, ".0")
    s
  }, character(1), USE.NAMES = FALSE)
}

.gog_as_csv <- function(x) {
  if (is.factor(x))                   return(as.character(x))
  if (inherits(x, "Date"))            return(format(x, "%Y-%m-%d"))
  if (is.integer(x) || is.logical(x)) return(as.character(x))
  if (is.double(x))                   return(.gog_shortest(x))
  as.character(x)
}

# The same `gog-cli/` marker `data.R` and `setup.R` walk up for, so all three
# agree about where the repository is regardless of the working directory.
.gog_out <- local({
  for (up in c(".", "..", "../..", "../../..")) {
    if (dir.exists(file.path(up, "gog-cli")))
      return(normalizePath(file.path(up, "book", "data"), mustWork = FALSE))
  }
  stop("make-data.R: cannot find the repository root from ", getwd())
})
dir.create(.gog_out, showWarnings = FALSE, recursive = TRUE)

# `environment()` rather than `globalenv()`: run by Rscript the two are the same,
# but `sys.source(..., envir = e)` puts the frames in `e` and this wrote zero
# files in that case — silently, which is the worst way for a generator to fail.
.gog_env <- environment()
.gog_frames <- sort(Filter(function(n) is.data.frame(get(n, envir = .gog_env)),
                           ls(envir = .gog_env)))
for (.n in .gog_frames) {
  .d <- get(.n, envir = .gog_env)
  .flat <- as.data.frame(lapply(.d, .gog_as_csv), stringsAsFactors = FALSE,
                         check.names = FALSE)
  write.csv(.flat, file.path(.gog_out, paste0(.n, ".csv")),
            row.names = FALSE, quote = TRUE, na = "")
}
cat("wrote", length(.gog_frames), "CSVs to", .gog_out, "\n")
