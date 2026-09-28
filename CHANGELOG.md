# Changelog

All four packages share one version number and are released together: `gog` on
CRAN-style repositories and PyPI, `GrammarOfGraphics` on Julia's General, and
`grammar-of-graphics` on npm. A version means the same grammar in every one.

## Unreleased

### Added

- **`save_svg(plot, path)`, in all four languages.** It draws the plot and writes
  the SVG to a file whose name ends in `.svg`, byte for byte as `render_svg()`
  returns it, so the same plot gives the same file in R, Python, Julia and
  JavaScript, and it returns the path. R had no way to write the file but
  `cat()`, and the other three spelled the job three ways. A path with another
  ending is refused, and a plot gog refuses leaves a file already at the path
  unchanged.

- **The beeswarm, `point * dodge`.** Over a category, `dodge` now moves each
  point across its slot only as far as it must to clear the points beside it, so
  no two touch and the swarm's outline shows each group's distribution. Values
  never move along the measure axis. A `color` split gives each group its part of
  the slot first, as it does for bars. When a slot is too crowded, the points
  that do not fit are drawn against its edge, never in the next category, and gog
  says how many. `point * dodge` used to be refused.

- **Colors by name.** `palette(c(Asia = "tomato", Europe = "steelblue"))` gives
  each category the color written beside its name, so a category keeps its
  color whatever order the rows arrive in. Python passes a dict, JavaScript an
  object, and Julia pairs. A name that is not a category, a drawn category left
  without a name, a name given twice, and names on a numeric column are
  refused; a misspelled name is answered with the category it was probably
  meant to be.

- **Leaving a legend out.** `legend = FALSE` on `color`, `size`, `shape`,
  `pattern` or `opacity`, as in `color(continent, legend = FALSE)`, keeps the
  channel mapped and leaves its legend out, and the plot takes the room. It is
  for a plot that already names the categories, on an axis or beside the marks.
  On a position, `group`, `label` or `play`, which draw no legend, it is
  refused. When `color` and `shape` share one legend, turning `color`'s off keeps
  the shapes and draws them in gray.

- **A halo for `text`.** `style(border_color =, border_size =)` now draws on
  `text` as well: a band of color under each label's letters, so a name drawn
  over lines, contours or dense points stays readable. Either setting works
  alone. The color defaults to the panel's background and the width to 3; over
  a transparent panel, name the color. A border on `text` used to be refused.

- **`smooth_band`, the band around a `smooth` line.** It is `smooth`'s pair
  transform, as `confidence` is `mean`'s: at each point of the LOESS curve it
  gives the confidence band of the fit, a low and a high, with the fit as its
  center. `ribbon * smooth_band + line * smooth` draws the trend with its band.
  The level is 0.95 unless you name another, as in `smooth_band(0.99)`. It
  needs five rows in each group, where `smooth` needs three: with fewer, the
  curve passes through every row and nothing is left to measure the band from.

### Changed

- **An `interval` or a `ribbon` in a space that never draws it hears only the
  space.** On `map()`, `globe()` and the other spaces that refuse them, they
  were first told to add a range transform, and `interval * range` was then
  refused by the space. Three globe refusals also use the globe chapter's
  words: a sphere has "no straight side" to write an axis label along, a `z`
  with no spike keeps "the marks drawn on the sphere" rather than "the surface
  marks", and `turn` is a longitude, not a bearing.

- **What a globe hides is said once per table, and names it.** The note that
  rows face away from the view, and the one for spikes below zero, now name the
  layer's table, as in "3192 of 4150 row(s) of `world_borders`". Two tables with
  the same counts printed one line, and a globe split into panels printed one
  line per panel; each table now gets one line, counted over every panel.

- **A log `z` is refused by the cube only in the cube.** "A log `z`-axis is
  not drawn yet" printed in `globe()`, `map()`, `polar()`, `nest()` and
  `network()` as well, first and above each space's own refusal of `z`. It is
  now given only to a plot drawn in the cube.

- **A flat `zone` with `group()` is told every way it gets its sides.** The
  refusal named a category's slot, `bounds`, `bin` and `density`, and left
  out `count`, `proportion`, `partition` and `flow`. It now lists all seven
  transforms that give a zone its sides.

- **Asking for a declared order names every language's way.** The `order()`
  refusal for a plot with no categorical position, and the `play()` note for a
  column with no stated order, said to "set the column's factor levels", R's
  word, in all four languages. Both now say to declare the column's levels in
  the table: a factor in R, `ordered()` in Python, Julia and JavaScript.

- **A bar over two categories is refused toward counts that draw.** `bar +
  x(country) + y(continent)` was told to use `bar * count`, which keeps
  `y(continent)` and is refused as well. The refusal now names counting either
  column, `bar * count + x(country)` or `bar * count + x(continent)`, and
  `bar * count + x(country) + y(continent) + space()`, which stands a bar on
  each pair. `bar * count` over two categories is no longer told there is
  nothing to measure, and its own refusal offers `+ space()`. Inside `space()`
  with no `z`, a `bar`, `box` or `interval` over two categories is asked for
  `z(<column>)`, not told to count. The note that a `space()` plot "is drawn
  flat" is no longer printed beside a refusal, since nothing is drawn.

- **An `edge` or a `layout` outside `network()` is refused once.** Its own
  refusal already names `network()`. A second one was printed beside it and
  offered a fix that was refused as well: `edge + polar()` was also told to
  drop `polar()` and draw the `edge` flat, and `point * layout(from, to) + map()`
  was also told to add `x` and `y`. The same holds in `nest()`, `space()`,
  `map()` and `globe()`.

- **A transform refused on a mark is not followed by "needs `y()`".** `line *
  cluster(...) + x(food)` was refused with the two marks that draw a cluster,
  and then told to add a `y`, which only led to another refusal. The same held
  for `step`, `area`, `point` and `bar`, and for any transform a mark does not
  take.

- **A clustered tile plot's refusals offer only what works.** With one position
  bound, `zone * cluster(over = )` said the leaf axis named ``, a name the table
  does not have; it now says the plot binds `x` but not `y` and that a clustered
  tile plot needs a category on each position. When `over` names neither axis,
  a numeric position is no longer offered as the profile, since naming it is
  refused too.

- **The circular cluster tree's refusal names the tree's horizontal bars**, where
  it said "treads", a word the book does not use.

- **A transform refused on a `zone` or a `surface` is explained by name.** Each
  refusal was one fixed sentence whatever was written: the zone's called
  `bounds` refused, though `zone * bounds` draws, and listed four fewer
  transforms than a zone takes; the surface's explained five of the thirteen
  it refuses. The message now gives the reason for each transform written, and
  its list of what the mark takes is the one the Combinations grid shows.

- **In JavaScript, `save_gif()` reads a leading `~` as the home folder,** as R,
  Python and Julia do; it took the path literally and failed on `~/wave.gif`.

- **`z` on `line`, `step`, `area` and `ribbon` is listed as refused, not as
  unbuilt.** gog refuses these four pairs by rule, but `gog-cli --rules` listed
  them as allowed and not drawn yet, so the Combinations grid marked them ◌. They
  are now `cannot`, drawn `—`. The refusal for `step`, `area` and `ribbon` gives
  each its own direction, where it repeated `line`'s ("A `area`", and `path` as
  "`area` with that sort removed").

- **A title sits over its map, not over the room the map leaves.** A map keeps
  its shape by drawing a smaller panel inside its rectangle, and the title stayed
  at the rectangle's top, as much as 120 px above the map. It now sits just above
  the panel, as the axis names already did, and the same holds for a `ratio` that
  shortens a panel. Over a facet's column strips it stays at the top.

- **On a page, an axis a plot gives up costs it no margin.** In `a / (b | c)`,
  where `c` shares its y axis with `b` and its x axis with `a`, `c` kept a blank
  strip where the y axis it gives up would have been; it now sits where it does
  beside `b` alone. A plot whose x axis a plot below draws no longer keeps room
  for tick labels it does not draw, so such panels are a little wider.

- **A misspelled column on a `zone` that reads its positions is refused.**
  `zone * density`, `zone * bin`, a tally or a partition whose `x` or `y` named
  a column the table does not hold drew an empty panel with no message. It now
  gets the refusal every other mark gives, naming the column. A zone whose sides
  come from `bounds` still takes the other axis from the panel.

- **Filled density bands leave a hole where the density dips.** With
  `zone * density(levels = )`, points spread around a ring filled the middle
  with the ring's own color, so the dip read as the highest band. Each level is
  now filled as one region with its holes, and the lower bands show through, as
  `path * density` draws them. A field with no holes looks the same as before.

- **A computed value outside a stated domain is left out, and said.** On an
  axis a transform computes, `y(count, limits = c(0, 40))` now leaves out a
  count above 40 and says how many values fell outside, as it already did for
  the table's own rows. Such a bar was drawn cut off at the top, as tall as 40,
  and in `polar()` a sunburst's ring past `y(depth, limits = …)` was drawn
  outside the circle. A `line`, `area`, `step`, `ribbon` or `path` that runs
  past a stated end is drawn as before.

- **`bin` cuts on a stated domain.** With `x(v, limits = c(0, 10))`, the bins
  now start at 0 and stop at 10. They started and stopped at the smallest and
  largest values inside the limits. An end left unstated still comes from the
  data. In `polar()`, `x(bearing, limits = c(0, 360))` is now the whole turn: the
  wedges fill it with no gap at the top, and 0 is at the top instead of 16
  degrees round.

- **A step in `polar()` draws its closing jump.** On a categorical angle the
  last value was carried round to the first spoke and stopped there, with no
  jump back to the first category's value. `line`, `area` and `ribbon` already
  closed there.

- **A layer that maps no color is drawn in dark gray beside a color legend.**
  It was drawn in the palette's first color, which the legend gives to the first
  category, so the `smooth` line through points colored by continent read as
  Asia's trend. A plot with no color legend keeps the first color, and a color
  set with `style(color = )` is kept. A texture legend's swatches now take the
  color their marks are drawn in: bars set to firebrick were keyed in blue.

- **Zero is written `0` on an axis counted in thousands or millions.** It read
  `0K` or `0M`, as if zero came in a unit.

- **A cube names every category on its floor.** A category name that met its
  neighbor was dropped, the way crowded numbers are, so the floor of
  `bar * bin(12) + y(continent) + space()` lost "Americas" with no message.
  Names are now moved aside to fit, and a name that still cannot fit is
  reported. Looking straight down (`tilt = 90`), the axis seen end-on no longer
  leaves one stray number behind.

- **A globe's labels are no longer cut at the edge of the sphere.** Labels were
  clipped with everything else a few pixels past the sphere, so a name beside a
  place near the edge lost letters: "Anchorage" read "Anchora". A label may now
  run past the sphere to the edge of its panel.

- **A map's longitude lines follow the projection.** Under the default
  equal-area projection every longitude gridline was drawn straight up from the
  data's southernmost latitude, so it was off by up to 20° elsewhere, and a
  longitude could be labeled outside the panel. Each line is now the curve its
  meridian makes, labeled where it meets the panel's bottom edge.
  `map(preserve = "angle")` draws them straight, as before.

- **Only a flat plot shares its axes on a page.** A map, a globe, a cube or a
  disc places its positions in its own space, but a page lined it up with any
  plot naming the same column: an equal-area map stacked over a Mercator one was
  drawn to the Mercator's longitudes, and two cubes side by side labeled a floor
  range neither had alone. Such a plot now keeps its own axes.

- **A flow's count axis is ticked the same way whatever stages it names.**
  `flow(class, survived)` labeled its count axis 500 / 1000 / 1500 and left out
  0 and 2000, while `flow(class, sex, survived)` over the same total labeled
  0K / 1K / 2K. Every flow's count axis is now ticked over its whole range, as a
  partition's is.

- **A histogram's end bars are drawn whole.** Its axis was fitted to the bin
  centers with a small margin, which below eleven bins is less than half a bin,
  so the panel cut into the first and last bars: a quarter of each at six bins,
  40% at three. The axis now reaches the bins' outer edges, on linear, log and
  calendar axes alike.

- **A legend takes the opacity its marks were set to.** With
  `style(opacity = 1)` or `style(opacity = 0.3)`, the marks changed and their
  key stayed at its default, so a horizon chart's darkest key read paler than its
  band. The color, shape and pattern keys now use the layer's set opacity.

- **Legend numbers drop their trailing zeros.** A count key read
  38.00 / 22.00 / 6.00 and a year key 2007.0 / 1979.5 / 1952.0; they read
  38 / 22 / 6 and 2007 / 1979.5 / 1952. Numbers below 1 keep three significant
  digits, where two decimals printed a share of 0.004 as 0.00.

- **Nodes sized by their `degree` draw a key.** A layout makes the `degree`
  column, so the table does not hold it, and `point * layout(from, to) +
  size(degree)` sized its nodes with no legend to read them by. The key now
  reads the frame the nodes were drawn from, and so does an `opacity` key.

- **A line is told it zigzags only when it does.** Every `line` of more than
  five rows with no `group` or `color` was told its points would be joined in x
  order, so each single series heard it, and so did each panel of
  `facet(country)`. The note is now said only when several rows share one `x`
  in a panel at one moment, as it is for `area` and `step`, and never above a
  refusal.

- **A `nest()` plot refuses a summed layer beside a plain one.** In
  `bar * sum + color(continent) + text + label(continent) + nest()`, the text
  packed its own rows over the summed regions, and most names landed in another
  continent's region. Each layer packs its own rows, so gog now refuses the
  sentence and says to sum the table in the host language first, so both layers
  pack one row per region.

- **`order()` with no column is refused, by name.** `order(desc = TRUE)` alone
  sent an empty column in R and raised a language error in the other three. It
  now says it names no column, and gives the two that work: the category column
  itself, as in `order(continent, desc = TRUE)`, sorts the axis from Z to A, and
  a column of values sorts the categories by value.

- **A `nest()` plot refuses the settings of an axis.** A packing has no axes,
  so `theme(grid =)`, `theme(tick_angle =)`, `theme(axis_label =)`,
  `theme(frame =)`, a position's `tick_count`, and `limits` on the measure under
  a summary such as `bar * sum` changed nothing and were accepted anyway. Each
  is now refused, as `x_label()` already was.

- **A channel written for the whole plot that every mark replaces is refused.**
  In `size(population) + point + size(life)`, the points take their own
  `size(life)`, so `size(population)` reached no mark and the plot drew as
  though it had never been written. The message names each mark's own column.

- **A `data()` at the end of a sentence is refused.** It has no mark after it
  to read its table, and the plot drew as though it had never been written.
  The message names the table and says to write a mark after it, or remove it.

- **Every panel, moment and selection reads one scale.** `size`, `opacity`, a
  numeric `color`, and the order of the `shape` and `pattern` categories were
  fitted again for each facet panel, each moment of a `play` sequence, and each
  pass of a selection. In `size(population) | facet(continent)`, Australia was
  drawn as large as China; a played bubble chart drew each year's most populous
  country at the largest size; and a panel without the first `pattern` category
  gave its texture to the second. Each is now fitted once, over every row the
  layer draws, and that is the scale its legend decodes.

- **Folded facets on a page keep their width.** Two plots folded with
  `facet(g, wrap = 2)` and stacked on a shared column were each squeezed into
  the width of one column, with the rest of the cell left empty. Each now keeps
  its own width, and the shared column still gives them one scale.

- **A y axis's name stays beside its panel on a page.** In `(a | b) / c`, where
  `c` shares a column with `b`, the page moves `c`'s panel to run under `b`, and
  the name of `c`'s y axis stayed at the left edge, 450px from its panel. It now
  moves with the panel. Plots stacked in one column keep their names in one
  column, as before.

- **A legend too wide for its plot is left out, and said.** A plot narrower
  than its legend drew its panel at a negative width, with no message: a scatter
  colored by continent at `theme(width = 150)`, or a thin marginal plot given a
  `color`. A legend that would leave the panel narrower than itself is now left
  out with a note, as a legend too tall already was, and the panel takes its
  room.

- **A `size` or `opacity` legend under a summary reads the summaries.** In
  `point * mean + x(continent) + y(life) + size(life)`, each dot is sized by its
  continent's mean, but the legend ran over every country's value, so Africa's
  mean of 54.8 read as 39.6. The legend now decodes the means.

- **A pattern key beside another color column is drawn in neutral ink.** With
  `color(sex)` and `pattern(survived)`, the "Survived" key was drawn in the
  first sex's color, as if the two were related. It is now drawn in the neutral
  ink the shape key uses. When `color` maps the same column, the key keeps each
  category's hue.

- **An empty label under `repel` draws nothing.** A label of `""` under
  `text * repel` still drew a leader line from its point to nothing. An empty
  label now draws no text and no leader, so `label = ""` keeps a point as
  something the other labels move around without naming it.

- **A legend keys only the categories the plot colored.** A `flow` that left
  out Crew's rows for a missing stage drew no Crew band, and its legend still
  listed Crew, in 1st class's color. A category left out of the drawing is now
  left out of the legend too.

- **A flow's bands draw the pattern their legend shows.** `ribbon * flow(…) +
  pattern(class)` drew a hatched legend over solid bands. The bands now take
  the hatch, as every other filled mark does.

- **A summarized point lies on its side as a bar does.** `point * median +
  x(life) + y(year)`, the category on `y`, drew every row of the table rather
  than one dot per year, while `bar * median` on the same sentence drew one bar
  per year. `point` with `mean`, `median`, `sum`, `min`, `max` or `quantile` now
  reads its orientation from which axis holds the category, as `bar` does.

- **Crowded numbers no longer print over each other.** In narrow panels, such
  as five facet columns, the numbers on a continuous axis used to run into one
  another at the panel edges. The axis now draws fewer numbers, leaving out the
  ticks and gridlines of the rest, and keeps the ones a larger round step would
  choose, so 0K to 40K by 10K becomes 0K, 20K and 40K. No message is printed,
  unless you stated a `tick_count`; then a message says how many ticks were
  drawn. A map's degree axes keep every tick.

- **Crowded category names no longer print over each other.** When the names
  on a categorical axis do not fit side by side, gog turns them 90 degrees to
  read upward. When even turned names are too close, it draws one name in every
  two or three, each with its tick, and says how to fit them all. Names down a
  `y` axis thin the same way. A `tick_angle` you state is kept, and names are
  thinned at that angle if they need it. The refusal of an angle outside -90 to
  90 now says this, where it recommended 45 for names that overlap. The axis
  name sits below turned names, where it used to be drawn across them. A
  dendrogram's leaf axis no longer draws a gridline for each leaf.

- **An animation keeps time with its column.** When `play` runs through numbers
  or dates that are not evenly spaced, each frame now shows in proportion to the
  gap to the next value, between a quarter of the usual time and four times it,
  so 2000, 2001, 2002, 2020 holds 2002 longest. Evenly spaced and categorical
  columns play exactly as before. The play buttons on the page and the pointer's
  readout follow the uneven frames, and a GIF saved from the plot keeps the same
  times. A GIF also keeps a `speed` written on a `play` before the marks, which
  it used to ignore.

- **A network's layout no longer depends on the order of its rows.** The same
  rows in another order used to move nodes by a fraction of a pixel. Every node
  now lands in the same place whatever order the table's rows are in, which
  moves the network figures by about a pixel once.

- **A network keeps its layout across panels, and packs its separate parts.**
  Under `| facet()` every panel now places each node where the others do, and
  draws only the edges its own rows state, so panels can be compared node by
  node. A graph in several separate parts draws them side by side, each at the
  same scale, where they used to drift to the panel's corners. A connected graph
  draws as before. `play` on a network is still refused; the message now points
  to the facet.

- **A `density` layer smooths all its groups alike.** Where one layer draws
  several estimates (a violin or a ridge per category, a curve per `color`, a
  contour per `group`, one per panel or per animation frame), they now share one
  bandwidth: the average of the bandwidths Silverman's rule gives each group on
  its own. A small group is no longer drawn as a needle beside a smooth large
  one, and shapes can be compared. A freed axis takes one per panel. `adjust`
  multiplies the shared bandwidth, a stated `bandwidth` still replaces it, and
  a plot with one group draws exactly as before.

- **Composed plots line up.** In a row, the panels now share their top and
  bottom edges, and in a column their left and right edges, even when the plots
  share no column. The plot with the narrower margin gives up the difference, so
  above or below a plot with a legend, the other panel is just as narrow.

- **`repel` steps around every dot drawn in the panel.** A label now clears
  the dots of every `point` layer, each at the size it is drawn. So labels from a
  small table of chosen rows no longer land on the dots they do not name, and a
  big bubble's name no longer sits across its own bubble. A dot that no label
  names pushes a label only while the label is next to its own point.

- **An alluvial diagram crosses less.** Where bands share a slot, each stage
  now orders them by the nearest stages, before and after, instead of by the
  first stage. On the Titanic counts the crossing between stages falls by 13%
  on three stages and 16% on four, to the same totals ggalluvial reaches.

- **A treemap packs the rows inside each region largest first.** The regions
  keep the order of their categories, and `order()` still sorts them. Inside a
  region the rows used to keep the table's order, which turned a long tail into
  thin slivers; packed largest first, the tiles come out close to square and the
  large countries have room for their names.

- **A statistic splits by every channel that splits, as a line always did.**
  `color`, `group`, `pattern` and `shape` each split the rows a summary is
  computed in, together. Before, only `color` (or else `group`) did, and the
  rest were merged away: `pattern` under `interval * range` drew solid
  whiskers under a legend of dashes, `shape` under `point * mean` drew circles
  only, and `color` beside `group` under `line * mean` drew an empty panel.
  `dodge` and `stack` split the same way, so `pattern` alone is enough to
  dodge, and a dodge leaves out the column its slots already come from.

- **`pattern` divides a bar that has no `x()`, as `color` does.**
  `bar * count * stack + pattern(era)` draws one bar divided by era. It was
  refused as a bar with no `x()`, while the same sentence with `color(era)`
  drew.

- **`proportion` after a summary works on a `zone`.** `zone * sum * proportion`
  gives each cell's sum as a share of the total over all cells, as
  `bar * sum * proportion` does. It drew exactly `zone * sum` before.

- **A declared order over numbers draws as categories in Python, JavaScript
  and Julia.** `ordered([2019, 2020, 2021], ...)` and a pandas `Categorical` of
  numbers crossed to the engine as numbers, so a year column drew as bars as
  long as the year. They now draw one slot per year, as `factor()` does in R.

- **`tick_count` works on a log axis, a date axis and a map.** On a log axis the
  count picks among a tick at every power, every second or third power and so
  on, or at 1, 2 and 5 times each power; on a date axis among the calendar's
  steps (a day, two days, a week, two weeks, a month and so on). The engine
  takes the spacing whose number of ticks is closest to the count. A map's
  degree ticks take the count too. All three drew their default whatever the
  count said, and said nothing.

- **`pattern` maps on an `edge`**, one dash per category, one per row, as
  `color` does. It was refused as not drawn.

- **`style(arrow = )` works on an `edge`.** `"end"` puts a head at the node in
  the second column of `layout(from, to)`, `"start"` at the first, `"both"` at
  each, and a head stops short of the node's dot. It was refused with a reason
  that said an edge has no direction.

- **`style(caps = )` works on a `box`.** `caps = FALSE` draws the whiskers
  without their crossbars, as on an `interval`. It was refused with a message
  that said a box has none, while every box drew them.

- **`color` and `shape` on one column draw one legend**, each glyph in its
  category's color, as `color` and `pattern` already did. They drew two legends
  with the same title.

- **The axis names and the legend sit beside the plot.** With
  `theme(ratio = )`, on a map, and around a polar circle, the plot is smaller
  than the room it was given, and the y axis name stayed at the image's edge,
  up to 150 pixels from its axis. The names now move in with the plot, and the
  legend's top stays level with the panel's.

- **A composed page ticks a shared axis the way each plot ticks it alone.** The
  page chose the step over a range that included each plot's margin, which can
  double it: `x(gdp, tick_count = 3)` beside `x(gdp, tick_count = 12)` drew 2
  and 6 ticks on the page and draws 3 and 11 now, as each does by itself.

- **JavaScript reads a `Date` on the session's clock**, as the other three
  bindings read a date-time's own clock. `new Date(2024, 0, 1, 12)` is 12:00 on
  the axis in every time zone; it drew the UTC clock, 03:00 in Seoul. A column
  of dates written without a time, `new Date("2024-01-01")`, which JavaScript
  reads as midnight in UTC, is drawn as the days it names.

- **R's `opacity()` no longer takes `tick_count`.** It accepted the argument and
  dropped it. A legend takes no tick count, and `color()`, `size()` and the
  other three bindings' `opacity()` never took one.

- **A page carries only the columns its plot names.** A plot that the reader's
  browser redraws, one with `brush()`, in the cube, on the globe, or a network
  given an angle, puts its table into the page. It carried every column of the
  table, including columns the sentence never maps. It now carries every row
  but only the columns the sentence names, so a column you never map stays off
  the page as it stays out of the SVG.

- **Every plot is named for a screen reader.** The `<svg>` carries
  `role="img"` and an `aria-label`: the plot's title, or with none, what it
  draws, such as "Points, x is gdp, y is life". Nothing on the page changes, and
  no tooltip appears. A page of several plots is a group, each plot named.

- **A partition's axes start at 0.** The mosaic, the icicle, the sunburst and
  the donut were ticked across their cells' centers, so a share axis read 0.2
  to 0.8 and a measure axis started at 500 or 1000, wherever the centers fell.
  They are now ticked across the cells: from 0 to the total, or from 0 to 1 for
  shares, as every other measure axis is. A sunburst shows 0 once, at the top,
  where its total would fall on the same line.

- **`proportion` works on a `partition`.** `zone * partition(...) * proportion`
  divides the measurement by its total, so the measure axis runs from 0 to 1.
  Each node's width, or each sector's angle in `polar()`, is its share of the
  total. `text * partition(...) * proportion` places the names on the same axis.
  Before, both were refused: the `zone` layer as a tile plot with no `y()`, the
  `text` layer as a summary with no label to draw.

### Fixed

Each of these sentences was accepted and drew something other than it said.
Each is now refused, with what to write instead.

- **`jitter(amount)` above 1.25.** At 1.25 a point reaches the edge of its
  category's slot, and past it some points land in the next category.

- **`order()` by a column of text, or by a column the table does not have.**
  The axis stayed in row order. `order()` by another column sorts by its
  numbers.

- **A `rule` layer naming both `x` and `y`.** Only the first was drawn. A rug
  on both axes is two layers, `rule + x(gdp) + rule + y(life)`.

- **`opacity` on a mark whose `color` is `"none"`.** `opacity` fades what
  `color` paints, so a hollow point stayed fully opaque. The refusal gives the
  border color that carries the transparency, such as `"#4682b44d"`.

- **A column named on the axis that `bin`, `count`, `density` or `proportion`
  draws on.** Each draws a number of its own there, so the column was never
  read and only titled the axis: `bar * count + x(continent) + y(life)` drew
  counts under "Life", and `point * bin * stack + x(life) + y(continent)` a dot
  plot under "Continent". The refusal names the statistic that reads the column,
  such as `bar * mean`. A name the table does not hold, such as `y(count)`,
  still titles the axis.

- **A plot with no mark.** `data(gapminder_2007) + x(gdp)` drew an empty panel
  with both axes marked 0 to 1, and so did a table, a `color()` or a `title()`
  with no mark. The refusal names a mark to add, chosen from the positions you
  wrote: `point` for two, `bar * bin` for one number, `bar * count` for one
  category.

- **A category on the axis `bounds` draws.** `ribbon * bounds(zero, height) +
  y(group)` drew one band through every group, with the category as the axis
  title. `bounds` draws that axis from its own two columns, so the category was
  never read. The refusal names the splits that draw a band per group:
  `group()`, `color()` or a facet.

- **A shared axis split into panels differently on one page.** A histogram
  stacked over a scatter faceted by continent stretched across every panel, lost
  its tick labels and lined up with none of them. Two plots that share an axis
  on a page must now be split the same way along it; faceting both by the same
  column lines up one panel over each.

- **A `y` under `partition`, other than `depth`.** It was not read: the mosaic
  was drawn unweighted, and the column's name was printed as the axis title. A
  partition reads its weight from `x`, and the refusal says so. `y(depth)`, the
  column `partition` computes, still states a sunburst's radial domain.

- **A negative weight in a `partition` or a `flow`.** It was counted as 0: its
  leaf was drawn with no width, or its path was left out, and the whole no
  longer summed to the table. `nest()` already refused the same data, and now
  both do, naming the column.

- **Two tables that read one axis as different kinds of value.** When one
  layer's table held a position column as text and another layer's held a
  column of the same name as numbers, the numbers were read as the places of
  the categories, counting from 0: a label at `g = 10` over three bars was drawn
  off the plot, and points under one text label left it too. A column of dates
  against one of numbers went wrong the same way. The refusal names each table
  and what it holds.

- **Two layers that map one channel from two columns.** With `color(country)`
  on one layer and `color(continent)` on another, the second column's colors
  went through the first column's scale, or through a ramp of their own, and
  the one legend named only the first column, so nothing decoded the second.
  `size`, `opacity`, `shape` and `pattern` did the same. The refusal names
  another channel for the second column that draws a legend of its own, such as
  `shape(continent)` on a point or `pattern(continent)` on a line, or a
  `facet()`. The same column on every layer still draws, and so does a plot
  that turns the legend off with `legend = FALSE`.

- **`brush` on a `map()`.** A map moves longitude and latitude to projected
  positions before it draws, and a brush on it could not agree with itself: a
  written range counted the right rows and dimmed the wrong ones, a dragged
  range dimmed the right ones and counted none, and a drag over a choropleth
  cut countries into shapes nobody drew. The refusal points to the same
  columns without `map()`, `point + x(lon) + y(lat) + brush(lon)`.

- **An `edge` with no `layout`.** `data(trade_partners) + edge` drew an empty
  panel with axes marked 0 to 1, flat and in the cube, and with a `color()` a
  legend for nothing. Beside a `layout` on another layer in `network()`, the
  edge was left out. The refusal gives the sentence that draws it,
  `edge * layout(<from>, <to>) + network()`.

- **`point * smooth` with a `z`.** Every point lost its third position, and
  the plot was an empty cube with an axis marked 0 to 1. It is refused as
  `smooth` is on every other mark in the cube, because a curve needs a domain
  and the cube's floor has none. The refusal points to `line * smooth` on the
  plane, or to `bar * mean` for a summary over the floor.

- **A stage named twice in `flow()`.** Stages are placed by their column's
  name, so `zone * flow(class, class)` drew every slot at the first stage, and
  `flow(class, survived, class)` drew its third stage on top of its first.

- **A second facet in one direction.** `plot | facet(continent) | facet(era)`
  kept only `era`, and two `/ facet()` did the same. A plot splits once across
  and once down, so the second is refused, and the refusal gives the crossing
  that keeps both, `plot | facet(a) / facet(b)`. In JavaScript the same holds
  for `across()` or `down()` written twice.

- **Two plots on a page that read one column through different scales.** A
  page draws one axis for a column, so a log scatter of `gdp` under a histogram
  of `gdp` put every point at the panel's edge, under an axis marked from
  10⁵⁸³¹, and `limits` on one of the two was read against the other's axis.
  It is refused, and the refusal asks for the same scale and limits in both,
  or another name for the column in one.

- **A second coordinate space.** Each space replaced the one before it, so
  `space() + polar()` drew polar, `polar() + space()` drew flat axes, and
  `map() + polar()` drew polar, with no message. A plot is drawn in one space,
  so the second is refused, and the refusal shows how to see both: the plot
  twice, side by side. Writing the same space again, as in `space() +
  space(turn = 60)`, still changes its angle.

- **`facet()` with no plot.** `facet(g) | facet(g)` built a pair of facets
  with nothing to split, and failed as a JSON parse error in R, an
  `AttributeError` in Python and a `MethodError` in Julia. It is refused with
  the sentence that facets a plot.

- **Some refusals are reworded.** A `zone` with no sides now lists all five
  ways to give it some, a boundary on a map included. A refusal about both
  axes is printed once and names both, not once per axis. `path * mean`,
  `box * mean` and `zone * density * proportion` print one refusal each, where
  each also printed a second that did not apply.

These still draw, and now say what they drew.

- **An `area` or `step` through rows that share one `x`.** The outline was
  joined in x order and zigzagged inside each value, with nothing said, while
  `line` on the same rows said the points would be connected in x order. Now a
  note says so, and names `color()`, `group()` or a summary such as
  `area * mean`.

- **The message under a bare `space()`.** A plot with no third dimension under
  `space()` is drawn flat, and the message told every reader to add `z()`,
  which a `line`, `step`, `area`, `ribbon` or cluster tree refuses. It now says
  to drop `space()` to draw flat on purpose, or to add `z()` to a mark that
  takes one, such as `point` or `bar`. For a floor of two categories it names
  `bar * count`, where it named only `bar * bin`, which needs numbers.

- **A `tick_count` that asks for more ticks than an axis draws.** An axis
  draws at most 26 ticks, and a count whose step gave more kept the first 26
  and left the rest of the axis bare, with no message:
  `x(gdp, tick_count = 40)` labeled 0K to 25K on an axis that runs to 49K. The
  step is now made larger until the ticks fit, so the axis is labeled from end
  to end, and a message names the step the count asked for and the step drawn.

- **`jitter(0)`.** It moves no point, so it draws the plot that `point` draws
  without `jitter`. A message now says so and names the ways to spread the
  points.

- **A summary whose every group is a single row.** `bar * mean + x(gdp) +
  y(life)` over a column whose values never repeat drew each row's own value,
  one bar per row, under an axis named for the column, with nothing to say the
  means were the rows. A message now says so and names the ways to summarize:
  cut the column into ranges with `bin`, or group by a column whose values
  repeat.

These drew something other than the sentence said, and now draw what it says.

- **A bare `space()` over `bounds`, `partition` or a cluster tree.** Each drew
  an empty cube with a made-up 0 to 1 vertical axis and none of its marks. None
  of them has a height to stand up in the cube, so the plot is drawn flat with a
  message, as `space()` is over every plot with no `z`. `interval * bounds` and
  `flow` under a bare `space()` draw the same way; each was refused before,
  `interval` as missing a `y`.

- **A network row with a missing end.** The row was left out, and a message
  said so, but its named end was still added as a node. It stood alone, and
  every other node moved to make room. A row that draws no edge now adds no
  node.

- **A stacked tally with a column on `y` and no `x`.** `bar * count * stack +
  y(season) + color(dir)` drew one pile of counts and used `season` only to
  title its axis. The column is the key of a bar on its side, as it is in
  `bar * count + y(season)`, so the bars now lie on their side, one per season,
  stacked by `dir`. The same holds for a number on `y` under `count`, `bin` or
  `proportion`. A `y` that a statistic reads, as in `bar * sum * stack +
  y(population)`, still draws one pile.

- **Another table's points past the last `bar`.** The axis was built from the
  bars' positions alone, so a forecast drawn after actual values, such as
  points at 2024 to 2026 beside bars at 2019 to 2023, was placed beyond the
  panel and cut off. The axis now holds every layer, and where the bars stand
  at an even step, a year each, the labels go on at that step.

- **A `rule` on a map from a table with one column.** A parallel at any
  latitude but 0 kept its raw degrees in the projected panel, which stretched
  the map into a thin strip, and a longitude did the same across. The rule is
  projected on the axis it names now: a parallel is placed exactly where the
  same rule with both columns is.

- **A date on `z`.** The cube's third axis labeled it in epoch seconds,
  `1710M`, where the same column on `x` reads `Mar 4`. It is ticked on the
  calendar now.

- **A log axis past a trillion.** A base-10 label grew a digit every power,
  `1000B` at 10^12 and `1000000B` at 10^15. From a trillion up it names the
  power, `10¹²`. A tick at 2 or 5 times a power with no short decimal, such as
  2×10¹² or 2×10⁻⁵, printed thirteen digits or `0.000`, and reads `2×10¹²` and
  `2×10⁻⁵` now.

- **`theme(background = "transparent")`.** Only the panel was transparent: the
  image behind it was painted white, so the whole figure stayed white. Now
  nothing is painted behind the panel, the legend's box is unfilled, and the
  page the figure is placed on shows through. A page of transparent plots
  paints no background either.

- **The spine plot's vertical axis.** `zone * partition(city, cross = TRUE) +
  x(people)` labeled it `-0`, `0` and `2`, and its columns filled only the
  middle half of the panel. The axis now runs from 0 to 1, and the columns fill
  the panel.

- **Five color names in a palette ramp.** `gray`, `grey`, `green`, `maroon`
  and `purple` were mixed from the values an older color list (X11) gives them,
  so `palette(c("white", "green"))` ended at a bright `#00FF00` while
  `style(color = "green")` drew CSS's `#008000`. A ramp now uses the CSS values,
  the same color the name gives everywhere else.

- **A crossed `partition` with no `x()`.** Its cells were drawn outside the
  panel, and its labels along one diagonal line. It now draws inside the panel,
  and each leaf counts as 1.

- **A name written with `y_label()` on an axis that draws no numbers.** A pile
  moved off zero by `stack(baseline = "wiggle")` or `"center"`, and the ring
  axis of a `partition`, draw no numbers and no name of their own. They dropped
  a name written with `y_label()` too, with no message. A written name is now
  drawn.

And these are now refused, with what to write instead.

- **`label()` naming a column of the edge table under `layout`.**
  `text * layout(exporter, importer) + label(exporter)` drew a text layer with
  nothing in it. A node has two columns, `name` and `degree`, and the refusal
  names `label(name)`.

- **`box * jitter`, `box * stack`, `box * bin` and `box * range` in R.** R
  answered with its own error, "non-numeric argument to binary operator". The
  refusal now reaches R as it reaches the other three bindings, naming `dodge`
  or the transform to drop.

- **A border on a `cross` point.** A cross is two strokes with no fill, so
  `style(shape = "cross", border_color = …)` drew the plot without the border,
  byte for byte. The refusal names the glyphs that have a fill. When a mapped
  `shape` draws some categories as crosses, the plot still draws, and a message
  names the categories that go without a border.

These were refused or noted before, and the message now says something
different.

- **A treemap's note about the names it leaves out is reworded.** It called
  every name left out "wider" than its region, even a name that was too tall,
  and it said "1 are". It now says how many "do not fit", in the singular for
  one. When a share is too small to have a region at all, the note no longer
  also says that the packing drew every share.

- **More refusals are reworded.** The gray palette refused on categories names
  the channel the plot's mark takes, `shape` for a `point`, where it named
  `pattern`, which a point refuses. Two `surface` refusals describe what the
  sheet draws: `density` lays flat cells with steps between them, as `bin`
  does, and over two categories the direction is a column in each cell. The
  refusals that list the reductions name all six, `quantile` included.

- **A `line` warning is reworded.** With a `color` of numbers and nothing that
  splits the rows, the warning said the line had "no group or color channel".
  It now names the `color`, says that a color of numbers colors along the one
  stroke instead of splitting it, and points to `group()`.

- **The refusal of `proportion` with `stack(share = TRUE)` is reworded.** It
  named plain `stack`, which divides nothing, so following its advice drew
  counts. For `bar * stack(share = TRUE) * proportion`, it also gave each
  transform the other's total. It now names `stack(share = TRUE)` and gives
  two sentences that draw: `bar * proportion * stack` for shares of the whole
  plot, piled, and `bar * count * stack(share = TRUE)` for shares within each
  pile.

- **`flow`, `layout` and `cluster` with a second transform have a refusal of
  their own.** Each sets the position and the size of every mark in its
  picture, so it takes no other transform. The old refusals spoke of cells and
  piles, which these three do not have. One advised "`flow` for shares of the
  whole plot".

- **The refusal of `bounds` on a mark that draws no low and high pair names
  `zone`**, the fifth mark that takes one, with `ribbon`, `interval`, `line`
  and `step`.

- **The refusal of `bar * dodge` with nothing to split names `color` or
  `pattern`**, the channels that split a bar. It named `group`, which a bar
  refuses.

- **The refusal of parentheses inside a plot names the marks you wrote.** In R,
  Python and Julia, `+ (data(life_bands) + rule)` is refused because
  parentheses do not group marks, and the example it gave named `point` and
  `area` whatever the parentheses held. It now writes out the marks inside
  them, `+ data(life_bands) + rule`. A group with no mark in it, such as
  `(data(life_bands) + x(level))`, is told to write its parts one after
  another.

## 0.3.0 (unreleased)

### Changed

- **Axis names follow one convention now, and it is the conventional one.** gog
  drew two at once: the y name sat horizontally at the top of its axis while the
  x name sat centered beside its own. Every plot now centers each name along its
  own axis, so the y name is turned through 90 degrees and the panel gets back
  the band the horizontal one used. `theme(axis_label = "end")` asks for the
  other convention, both names at their axis's far end and horizontal, so
  nothing on the plot is read sideways. **Every plot with a y label looks
  different**, which is the point: one rule a chapter can state, instead of two
  that cancelled out.

### Fixed

- **Installing from GitHub needs `build = FALSE`, and the refusal now says so.**
  `remotes::install_github("psychometrician/gog", subdir = "r-pkg/gog")` never
  installed: by default `remotes` packs the package directory alone into a
  tarball before installing it, and the engine's Rust sources sit beside that
  directory, so `configure` found nothing to build and refused. With
  `build = FALSE` remotes installs from the whole tree and the engine builds
  during the install. A copy that arrives the default way is recognized from
  the fields remotes writes into DESCRIPTION, and the refusal now names the
  argument and the exact call.

- **`style(pattern = )` tells every reader which five values are a stroke's and
  which five are a fill's.** Three of the four packages listed all ten flat, so
  a reader learned the split only by being refused a second time — and the split
  is real, since a dash on a fill and a hatch on a stroke are both refused.

- **The four angle atoms show an example in every language.** `space`, `polar`,
  `globe` and `network` each refuse a non-number with a call you can copy;
  JavaScript had carried those examples and the other three had not.

- **A Julia refusal printed its own template.** `theme(frame = )` told a reader
  the three choices were `"$v"`, `"$v"`, `"$v"`, because an escaped dollar left
  the list uninterpolated. It reached a notebook cell, not only a console.

- **The four packages word the `frame` refusal the same way.** Each had written
  its own sentence for the same rule. How a call is spelled is still each
  language's own; what the sentence says is the grammar's.

- **Two plots on one page no longer borrow each other's ink.** An SVG id is
  resolved against the whole document, and every plot in a notebook is in one
  document. gog minted ids by hashing the element, so two plots using the same
  texture in the same color were handed the same id and the second bound to the
  first's definition. It drew correctly until the host put the owning cell out
  of view, and then a correct plot drew nothing at all. Every id a plot mints is
  now its own.

- **`stripes` and `grid` draw at their full weight.** Both put their rules on
  the edge of the repeating tile, where a pattern clips away the half of each
  rule that falls outside, so they drew at about a third of the weight of the
  diagonal textures they are meant to match and read as a paler shade of the
  fill. The rules now run down the middle of the tile.

- **A refusal names the values that exist.** `style(pattern = )` advised three
  dashes and five textures after the vocabularies grew to five and six, so a
  reader who wrote one wrongly was told the previous release's list. Both
  refusals now read the vocabulary rather than restating it.

- **Python's refusals quote gog's vocabulary the way the other three do.** A
  value like `"end"` is the grammar's, identical in every language, and Python
  alone wrote it as `'end'`. How a call is spelled is still each language's own.

- **A dashed line's key is readable again.** The legend drew each dash in a 16
  pixel run, which is shorter than one `longdash` repeat, so `solid` and
  `longdash` came out as the same line with one slightly longer than the other
  while the plot drew them clearly apart. A dash swatch now gets a wider column
  than the other swatch kinds, enough to show the pattern repeating, because a
  key that cannot show the repeat is not decoding anything.

### Added

- **Two more glyphs, two more dashes and one more fill texture.** `shape` now
  draws seven kinds rather than five, gaining `star` and `wye`, which completes
  d3's symbol family. A stroke's `pattern` gains `dotdash` and `longdash` for
  five, and a fill's gains `stripes` for six. `stripes` fills the hole in the
  family the other five already made: `hatch` is one diagonal, `crosshatch`
  both, `grid` both orthogonals, and nothing drew a single orthogonal until now.
  The counts differ on purpose. A glyph shows its whole silhouette at ten
  pixels, a fill is the largest thing on the panel, and a dash reads only where
  enough uninterrupted line is visible, so each geometry stops where its own
  room runs out.

- **A `shape` or `pattern` column with more categories than kinds now says so.**
  The vocabulary starts over once it runs out, so two categories drew the same
  glyph and looked like one group, silently. gog now names the count, the limit
  and two ways out. The plot still draws: it is legal, and the grammar does not
  forbid it.

### Fixed

- **A refusal that ends in `style()` now says so.** Mapping a column to `size`
  or `opacity` on a mark drawn as one stroke or one region was refused with "a
  line has no size feature", which is not true: a line has a width, it cannot
  vary it row by row, and `style(size = 2.5)` is the value the reader was
  after. Those sixteen refusals now say the mark takes one for the whole layer
  rather than one per row, and hand over the setting. A mark that genuinely has
  no such feature, a `bar`'s `size` or a `text`'s `shape`, still says so.

## 0.2.0 (2026-08-27)

### Added

- **`opacity` maps on a `surface`, face by face.** Bind a column and each face of
  the sheet takes its own transparency, so the mesh thins where its measure is
  small and a reader sees through it to what it was hiding. A face reads its
  opacity where it reads its color, at its own center, which is the mean of its
  four corners — one reading of a face however many channels ask for it. The
  mapping was legal grammar the engine refused, and the refusal argued from an
  `area`'s reason: a region has one interior, so a row there is a vertex of the
  boundary and there is nothing for a per-row value to vary across. That is still
  true of `area` and `ribbon`, and it was never true of a mesh, which has faces.
  `style(opacity = )` still sets one value for a whole sheet.

### Fixed

- **A refusal from the engine now names the column instead of spelling a call.**
  Binding a channel a mark has no feature for used to say `Remove `opacity(life)``,
  which is R's spelling shown to every reader; a Python reader who wrote
  `opacity(col.life)` was told to remove something they had not typed. The engine
  is never told which language called it, so it now writes `Remove the `life`
  mapping from `opacity``, which reads correctly in all four. A refusal a binding
  raises itself still speaks that binding's idiom, because a binding knows its
  reader.

- **`flow()`'s refusal now reads the same in all four bindings, and names a
  column that exists.** Given one stage column, every binding refuses and writes
  its own message. R named the endpoints of its own example, the other three said
  "from its first stage to its last", and the four therefore disagreed on wording
  that carries no idiom. All four now name the endpoints. Python's and
  JavaScript's also pointed at `col.klass`, which names a column called `klass`;
  the table in question has one called `class`, so they now read `col["class"]`
  and `col.class`, each of which resolves to the real column.

- **R: `layout()` handed a matrix now names `graphics::layout()`.** gog's
  `layout` masks the base function that arranges plot panels, and a masked
  name's refusal must say which function answered. A matrix is that proof: it
  is `graphics::layout()`'s first argument and can never be an edge column, so
  the refusal now points there, the way `density()`, `jitter()` and the other
  masked names already do.
- **A treemap's label report now names what it drew, so its numbers close.** A
  packing says how many names it could not fit, and a reader subtracts to learn
  how many are on the plot. The subtraction was wrong wherever a share was too
  small to have a region at all: such a row is not an unfitted label, so it was
  counted in neither number, and the book's own treemap of 142 countries
  reported 116 left out over a plot carrying 25 names. The message leads with
  the drawn count and gives a reason for every row that is missing, so nothing
  has to be worked out and no row goes unaccounted for.

## 0.1.0 (2026-08-18)

### Added

- **JavaScript: `html_block()` returns a plot the reader can turn.** The same
  SVG `render_svg()` writes, wrapped for a page, with the controls beside it
  and the engine that turns a cube or a globe. Write it into a page, a notebook
  cell, or a dashboard panel. It was in the package and reachable from nothing:
  a JavaScript reader could get a turnable plot in a browser window through
  `show()`, and a still picture everywhere else, while R, Python and Julia each
  handed a notebook the turnable one automatically. Those three answer a
  question their host asks an object about how to display itself; JavaScript
  has no such question, so it gets a function.

- **`network()` is a coordinate space, `layout` places a graph in it, and
  `edge` is the fourteenth mark: the network diagram.** `layout(from, to)`
  reads the two endpoint columns of an edge table — one row is one relation —
  and the engine computes a position for every distinct name, the same way in
  every language, so one sentence is one picture to the byte. Three marks read
  the placement: `edge * layout(from, to)` draws the connections, `point *
  layout(...)` the nodes, `text * layout(...) + label(name)` their names, and
  the layout publishes each node's `name` and `degree` so `size(degree)` reads
  its busyness. The space draws no axes, no ticks and no grid, because a
  layout's positions mean nothing as quantities; for the same reason a bound
  position, a brush, or an axis label is refused with its reason rather than
  drawn. Stating a viewing angle states the cube — `network(turn = 35, tilt =
  20)` computes the same layout in three dimensions and draws the glass box
  without numbers, and in the web edition it turns with a drag. A row from a
  node to itself is refused with its count; a row missing an endpoint is left
  out and counted. Edges map `color` by any of their columns and `opacity`
  continuously, the stroke that fades by weight.

- **`flow` lays a magnitude through its stages: the flow diagram.** The stages
  are categorical columns named in the atom, in reading order, and one row of
  the table is one path through all of them — `ribbon * flow(class, sex,
  survived) + y(n)` draws the Titanic's people as bands, each as thick as its
  path's count at both ends. Three marks read the one layout: `ribbon` the
  bands, `zone` each stage's stacked slots, and `text * flow(...) +
  label(name)` their names, so the layers always agree about where everything
  sits. `color(<stage>)` on the band layer colors every band by the category
  its path holds at that stage; a column outside the stages is refused, since
  only a stage holds a value a whole path carries. Bind nothing to `y` and
  every row weighs 1, the same tally `partition` falls back on. The stacks are
  contiguous, so the measure axis keeps its ticks and reads true cumulative
  magnitude, and rows missing a stage value are counted out loud rather than
  dropped in silence. Bending a flow into `polar()` — the chord diagram — is
  valid grammar the engine does not draw yet, and says so.

- **`globe()` is a coordinate space: the earth itself, viewed.** `x` is
  longitude and `y` is latitude, exactly as on `map()`, and the same five marks
  stand on the sphere's facing half — a `point` at a place, a `path` bending
  along great circles, a `text` naming a place, a `rule` holding a whole
  meridian or parallel, and a `zone` with `group()` filling each region of a
  boundary, where a country the horizon cuts is closed again along the edge of
  the disk. `globe(turn =, tilt = )` names the place the view faces, a bearing
  that wraps and a latitude that stops at the poles. Rows on the far half are
  hidden behind the sphere and the plot says how many, never dropping them in
  silence. The graticule is the panel grid, so `theme(grid = )` reaches it, and
  a globe draws no axes at all. In the web edition, dragging turns the globe
  the way it turns the cube. A binned field on the sphere is designed and not
  drawn yet: its correct tiling is hexagonal, and that equal-area grid is not
  built, so `zone` without a boundary refuses with the reason named.

- **A `bar` on the globe is a spike: the measure standing on the radius.** The
  flattened map has no axis to spare, and the sphere has exactly one — the
  radius, pointing away from every place — so `bar + x(<lon>) + y(<lat>) +
  z(<column>) + globe()` stands a spike at each place, measuring outward from
  the surface against the fitted top. The sphere itself is the clip: a spike
  just behind the horizon still peeks over the limb when it is tall enough. A
  value below zero has nowhere to point and is counted out loud, a `bar`
  without `z` is asked for its measure, and `z` with no `bar` to read it is
  refused rather than ignored. Shaping the measure is the host's line of code:
  `z` takes no `scale` here, and the refusal says why and what to write.

- **`cluster` joins the closest leaves one pair at a time: the cluster tree, and
  the clustered heatmap.** The leaves are the levels of the bound categorical
  position, each described by its value at every level of a profile column —
  `path * cluster(amount, over = nutrient) + x(food)` draws the tree, with
  the merge distance on the unbound axis, which names itself; `y(food)` lays
  the same tree on its side. The statistic is fixed and stated: Euclidean
  distance on the values as given, branches joined at their average distance,
  leaves ordered so the closest neighbors sit adjacent. `zone * cluster(over
  = nutrient)` is the second reading: the tile plot unchanged, its slots
  reordered to the tree's leaf order. Composed with `|` and `/`, a clustered
  panel decides the order of any categorical axis it shares, so trees above
  and beside a plain tile plot are the whole clustered heatmap — three plots
  and two operators, no new figure type. A profile with a missing or doubled
  cell is refused with the pair named, `order()` against the tree's own axis
  is refused as two orders for one axis, and two composed panels deriving
  different orders refuse rather than letting one win in silence. Coloring
  subtrees, the circular tree, and clustering both axes in one sentence are
  valid grammar the engine does not draw yet, and each says so.

### Changed

- **A repelled label now rests on whichever side of its point has room.**
  `text * repel` pulled every label back toward its starting place above its
  dot, and moved colliding labels apart only straight up, down, left or right
  — so nearly every name ended up stacked on top of its point, 24 of 26 on the
  fixture that measured it, where ggrepel spreads them wherever there is room.
  The placement now pulls each label toward its own point, pushes it off every
  word and every dot along the line between their centers, and lets the two
  motions settle just clear of the dot on the free side. A lone label still
  rests above its point, `style(nudge = )` still names the side a label
  prefers, the count of labels still touching is still reported, and the
  placement is still deterministic. Reported, measured against ggrepel, and
  prototyped by @lh (#2).

- **`repel` now composes with every transform, `layout` and `flow` included.**
  It was classified as a position modifier and so collided with any transform
  that places its own marks, but repel moves *ink*, at draw time, after every
  computation has run — it decides where a word rests, never where a mark
  sits. `text * layout(from, to) * repel + label(name)` is the network whose
  names step off their dots, and the same composition now works wherever
  `text` reads a computed placement.

- **The boundary refusal on `zone` names both geographic spaces.** Writing
  `zone + group(<column>)` outside them is refused as before, and the
  direction now says to add `map()` or `globe()` if the column names regions
  on a boundary — it named only `map()` while `map` was the one space that
  could read one.

- **A written `space(tilt = )` outside -90 to 90 is now refused.** `tilt` is how
  high your eye is, and height runs out: at 90 you look straight down and at -90
  straight up, so past either end the scene hangs upside down with all three axis
  names piled into one corner. Dragging already stopped at both ends, and writing
  now does too. `space(turn = )` is unaffected and stays silent at any value,
  because a bearing genuinely wraps: 390 is 30, and refusing both angles alike
  would teach a cap the grammar does not have.

- **`smooth` now refuses a group with fewer than three rows.** Two points are a
  straight line and one is a point, so there is no curve to fit. Below three rows
  the curve could not be computed and the rows themselves were drawn instead,
  which put a two-point segment beside a hundred-point curve with nothing to say
  that one was a fit and the other the data. The count is per group and per panel,
  not per table, since a statistic runs inside each group and faceting splits
  before it — so a table of 284 rows split into 142 pairs is refused, and the
  message names the split to give up.

### Fixed

- **Turning a cube past a full circle no longer loses axis labels.**
  `space(turn = -360)` is the same view as `space(turn = 0)` and drew the
  same marks and the same box, but two of eighteen tick numbers went missing: an
  axis silently lost part of its own scale, with every mark in place and nothing
  reported. Equal bearings now draw the same picture to the byte, however many
  laps the number carries.

### Removed

- **Three warnings about a missing `x()` or `y()` are gone.** Each said
  "Rendering empty chart", and each named a plot that was already refused with
  direction, so the message was never the only voice and never the deciding one.
  It also described neither outcome: nothing is rendered when the plot is refused,
  and a chart *is* rendered under `GOG_STRICT=0`. The refusals themselves are
  unchanged.

- **`gog_table()` refuses a table name the book does not have.** It says which
  name it could not find, and when one table is within two letters it names that
  table. JavaScript was the worst of the four: a name that does not exist gets a
  404 page from the site, and that page was parsed as a table, so the caller
  received 88 rows in a column named `<!DOCTYPE html>`. The other three stopped,
  but each with its host language's words for a failed request, which
  `except GogError` and `catch (e instanceof GogError)` do not catch. All four
  now refuse the same way the rest of the grammar refuses, and a site that cannot
  be reached is told apart from a name that does not exist.

- **R's `save_gif()` suggests a path in the directory you asked for.** Given
  `out/sub/wave.png` it answered `save_gif(p, "wave.gif")`, dropping everything
  but the file's name, so following the advice wrote the file into the working
  directory. Python, Julia and JavaScript already kept the directory.

## 0.0.5 (2026-08-12)

### Added

- **A played plot carries a transport: step back, stop or start, step forward.**
  The three buttons sit on the same line as the zoom and the camera, and they move
  the clock rather than the plot. Stepping stops the clock, because a running one
  would carry you off the frame you asked for. The ends join up, so going back
  from the first frame reaches the last, which is what the sequence already does
  when it loops. Stop the clock and the camera saves the frame you stopped on.
  Nothing in the grammar grew a word for any of it: the sentence is unchanged, and
  a printed sequence is the first frame as before.

- **`play` on a column with no stated order says so.** A sequence claims that one
  frame comes after another, and a text column with no declared levels runs in
  whatever order its values happen to appear, which is a fact about how the file
  was sorted. gog now names the order it had to invent and offers the two ways to
  state one. The plot still draws: a column whose levels *are* declared is silent,
  ordered or not, so a category with a real order is unaffected.

### Changed

- **`book_table()` is now `gog_table()`, and the old name is gone.** Change the
  call and nothing else: the arguments, the table names and everything it returns
  are the same. The name now says which package it comes from, which the old one
  did not. It was named after an artifact while every other helper is named after
  what it does, so a reader with `god` loaded saw `book_table` and `god_table`
  side by side with no way to tell they were the same helper doing the same job
  for sibling books. The one-letter distinction that separates the two projects
  everywhere else now separates these two names as well, and neither masks the
  other. There is no alias, deliberately: two spellings of one function is a debt
  that only ever gets paid by removing one of them, and removing it costs less
  today than it ever will again.

- **The hand is gone from the row of controls under every plot**, leaving four:
  zoom out, zoom in, fit, and the camera. It was a label rather than a button, and
  the pointer already says the same thing better, becoming a hand over a plot that
  can be moved. Dragging a magnified plot still moves it, exactly as before.

- **Turning a cube with the mouse now reaches straight down and straight up.**
  The drag stopped one degree short of both, at a tilt of 89, while
  `space(tilt = 90)` was accepted when written, so the gesture could not reach an
  angle the sentence could name. The drag now stops at 90 in both directions.

### Fixed

- **A selection no longer restarts a stopped sequence.** Drawing a selection
  redraws the picture, and the redraw carried the clock's reading across but not
  the fact that it had been stopped, so a played plot started running again with
  its button still saying it was paused.

- **Writing a GIF now says what the engine said.** `save_gif()` and `--gif`
  built every diagnostic and then dropped the list, so an Assumption — or, under
  `GOG_STRICT=0`, a refusal being drawn anyway — wrote the file in silence. The
  same words now reach you there as on every other path.

- **The browser hears every warning the command line prints.** Rows a log axis
  cannot place, a custom palette with the wrong number of colors, and a
  many-row `line` with no `group` all warned on stderr, which a browser does
  not have, so a notebook user was never told. The warnings now travel with the
  drawing. A clean render also clears the previous render's note, a refusal
  still reports the rows a missing value cost, and a request that is not UTF-8
  is a report rather than undefined behavior.

- **One missing value no longer poisons a group's statistic.** A single NaN in
  `bar * mean + x(category)` turned that category's whole bar into nothing,
  while the same transform without an `x` quietly dropped the value. Every
  keying now drops non-finite values the same way, and a group with nothing
  finite draws nothing.

- **A NaN coordinate no longer aborts a surface under `GOG_STRICT=0`.** A zero
  on a log axis, asked to draw anyway, reached the mesh as NaN and crashed the
  render instead of refusing or drawing; the row is now skipped like any other
  row with no place.

- **A value outside its scale is held at the scale's ends.** A transform output
  past a stated `limits` could ask for an opacity above one or a negative point
  radius; both are now clamped, and a NaN gets the least ink rather than an
  attribute nothing renders.

- **R refuses a value where a channel wants a column, as the other three
  bindings always have.** `color("red")` used to reach the engine as a column
  named `"red"`, quotes included, so the error blamed a column the reader never
  meant. The refusal now names both fixes: the bare name to map, `style()` to
  set. The viewing angles (`space()`, `polar()`) must be numbers and a label
  (`title()`, `x_label()` and siblings) one string, each refused in R at the
  line that wrote it.

- **Every refusal in Python and JavaScript is one class.** `map(preserve=)`
  raised a bare `ValueError` and `book_table()` a `TypeError`, so
  `except GogError` missed them. Julia's `book_table()` answered a non-name
  with a bare `MethodError` and now speaks gog's sentence — and it deletes its
  downloaded file when it is done reading it.

- **Julia reads the two browser-asset addresses at load time.**
  `ENV["GOG_WASM_URL"]` and `ENV["GOG_JS_URL"]` were read while the package
  precompiled, so setting them in your own script did nothing until the package
  happened to rebuild. Its missing-engine message also now says the plain
  truth: no installed copy of this binding carries an engine yet.

- **The README's kernel table names all of 0.0.4's atoms** — `quantile`,
  `deviation` and `repel` were missing directly above the sentence claiming
  every word draws — and Python's warning about `from gog import *` now counts
  six shadowed builtins: `map` was absent from the sentence written to warn
  about exactly that.

## 0.0.4 (2026-08-05)

### Added

- **`repel` moves labels off one another when they overlap.** `text * repel` is
  the fourth collision modifier, beside `dodge`, `stack` and `jitter`, and the
  first whose collision is made of ink rather than of position: a label is as wide
  as the word it draws, so two labels overlap where their points never did. Every
  label ends up outside its own dot, and one that moved far keeps a thin line back
  to its point. The placement depends only on the labels and the rows, never on a
  random-number generator, so one specification always draws the same picture.
  Above some number of labels no arrangement fits at all; every label is still
  drawn, and the plot reports how many still overlap. It takes no parameter, and
  it composes with `style(nudge = )`, which names the side a label prefers.

- **`deviation` draws the spread of the data, as `confidence` draws the
  uncertainty of the mean.** `interval * deviation` is the mean plus and minus
  one standard deviation, and `deviation(2)` two. It carries a center, so it
  draws a pointrange the way `confidence` does. The two are drawn as the same
  whisker everywhere else, and on the same fifty rows they differ by a factor of
  three and a half, which is why both are written out rather than left to a
  caption.

- **`quantile(p)` reduces a group to the value at one probability.**
  `line * quantile(0.9)` is the 90th percentile per group, which is the shape of
  a service level, a growth chart and a pay band. There is no default: the only
  sensible one is the middle, and the middle already has a plain name. At 0, 0.5
  and 1 the plot draws and says that `min`, `median` and `max` are the plain
  names for the same numbers, so a program sweeping over deciles does not break
  at the middle.

- **`range` takes the band's two ends.** `interval * range(0.25, 0.75)` draws the
  interquartile band instead of the full spread, and any other pair works the
  same way: `ribbon * range(0.1, 0.9)` is the middle 80 percent, and two of those
  layered at different widths is a fan chart. The two numbers are quantile
  probabilities, so an unset end is that side's extreme and bare `range` is
  unchanged, being the minimum to the maximum it always drew. The quartiles are
  the ones `box` already computes, from the same rule, so a band and a box body
  agree. An end outside 0 to 1 is refused, and so is a band that runs downward.

- **Clicking a mark stamps its values onto the plot.** The row that appears
  while the pointer is over a mark stays there once you click it, so several
  rows can be read at once and compared against each other, which hovering
  cannot do because it forgets the last one. Drag a card and it goes where you
  put it: the line stretches after it, and a long line grows an arrow head at
  the end that means the row, so cards can be moved clear of the crowd they
  name. A card keeps its place beside its point through zoom, pan and every
  redraw. On a plot that plays, a stamp belongs to the frame it was made in: a
  row there is one country in one year, so the stamp waits while the other
  frames run and comes back on its point each time the loop returns, and the
  card names the year it holds. Three things take a stamp off, the `×` on a
  card, a click on a card that did not move, and `unstamp` for all of them at
  once. `clear` leaves them alone, because a stamp is not a selection. A click
  on empty space still clears the selection, as before. The camera saves the
  cards where you put them, which is the camera's own rule rather than a new
  one: it writes what you are looking at, the way it already writes how far you
  have zoomed and the angle a cube is turned to. Nothing about the sentence
  changes and the printed page carries no stamps: this is a way of reading a
  plot, like turning a cube. What it is for is finding the points worth naming,
  and naming them is `text`.

### Changed

- **In R, `range` now masks `base::range`.** A transform that takes a parameter
  has to be a function, and R resolves a call by looking for a function, so
  `range(x)` reaches gog's where it used to fall through to base R's. Write
  `base::range(x)` for the smallest and largest of a vector. gog's `range` says
  so when it is handed something that is not a probability. This affects R only:
  the Python, Julia and JavaScript atoms were already callable.

- **The controls under a plot are two lines, and the first is the same one
  everywhere.** Zoom, fit, the hand and the camera are the only controls every
  plot carries, so they now have a line of their own directly under the picture,
  with whatever the plot adds for itself beneath them. In one line they slid
  along as the controls beside them changed width, so the button you wanted was
  never twice in the same place. A plain plot looks as it did. On a plot in the
  cube this also separates the two words that undo something: the frame returns
  the view, beside the buttons that changed it, and `reset` returns the angle,
  beside the readout stating it.

- **A control with no word on it now says what it does when you rest the pointer
  on it.** Eleven of them are drawings rather than words: the two magnifiers, the
  frame, the hand, the camera, the three drag modes, the two page arrows under a
  table of selected rows, and the cross on a stamp card. A drawing is only
  recognizable to someone who has met it before, and the browser's own tooltip
  answered about a second later in a box the page cannot style. Each control now
  raises its own small label instead, filled with the color the bar already
  inherits from the page and written in whichever of black or white can be read
  on it, so it is legible in a dark editor as well as on a white page. Keyboard
  focus raises it too. A button carrying a word, such as `clear` or `show rows`,
  is left alone.

### Fixed

- In R, the `box(whiskers = )` refusal printed two hyphens where every other
  message in the package, and the same message in the other three bindings,
  printed a dash. R is the one binding that cannot carry the character directly,
  so it has to be written as an escape, and this message was not.

- A table value containing `<` broke the box that reported it. The values under
  the pointer, and the card a click leaves behind, were built by pasting each
  cell into markup, so a column holding something as ordinary as `a < b` or a
  name with an ampersand in it stopped being read as a value. The characters are
  shown as themselves now.

- A frequency polygon was told it might be a tangle. `line * bin` draws one point
  per bin, so connecting them in order is the whole plot, and it was answered
  with the warning meant for a line drawn through raw ungrouped rows. The same
  went for `line * quantile(0.9)` as soon as that transform existed, while
  `line * mean` on the same rows said nothing. A `line` now asks whether anything
  in the sentence leaves one value per x, rather than checking a list of the
  transforms that do, so every summary is treated alike and a new one needs no
  amendment. The warning is unchanged where it was right: a line through many
  rows with nothing grouping them still says so.

- A long `play` sequence asked you to slow it down. The note that reports how
  long the loop will run always offered the same pace, whatever pace the
  sentence already set, so `play(second, speed = 6)` was answered with "run it
  faster with `speed = 4`". It now works out the pace that would bring the loop
  under the length it would not have remarked on at all, and offers that. Where
  the sentence is already at or past that pace it offers no number, and says to
  bind a column with fewer values instead, which is the only thing left that
  would help.

- Pointing at a mark named the wrong row on several kinds of plot. The readout
  works out where each row was drawn instead of asking the picture, which is
  exact only where a mark stands at its own value. On a faceted plot it searched
  the whole table against whichever panel the pointer was over, and because the
  panels share their scales the row it found from a different panel landed
  exactly where an answer belongs. On a plot that plays it searched every moment,
  including the ones not on the screen. It also had no way to know that `jitter`,
  `dodge` and `stack` set a mark beside its value, that a polar plot bends both
  axes, or that a map turns its two columns into places on the page before
  drawing. Panels now say which rows they drew and which moment is showing, so a
  reader gets a row from the panel they are pointing at and the moment in front
  of them. Where the position cannot be worked back out the readout says nothing
  and the line under the plot says why, which is the honest answer where naming
  a row would be a guess.

- A drag across a violin or a ridgeline selected the wrong categories. Where
  `density(reach = )` reaches past half a slot the shapes lean out of their
  slots, so the axis widens to leave room for them, and the browser was reading
  that axis as though it were still exactly as wide as the categories standing
  on it. The pointer landed a slot or more from where it looked, and the reader
  dragged over one category and selected another. Plots whose axis was never
  widened were never affected, because there the two readings give the same
  answer.

## 0.0.3 (2026-08-04)

### Added

- **`save_gif()` writes a played plot as a file that moves.** A plot that binds
  `play()` moves in a browser and shows one still frame everywhere else, so a
  slide, a message or a post got a picture that had stopped.
  `save_gif(plot, "wave.gif")` writes the sequence those places will play, and
  `scale` multiplies the canvas when the file is wanted larger. Nothing has to be
  installed first. The frames come from the one drawing the plot already made, so
  the file cannot disagree with the picture beside it. A plot with no `play()` is
  refused, and so is a path that does not end in `.gif`.
- **A surface can be colored by the estimate it draws.** `surface * density +
  color(density)` ramps the sheet by the same number that gives it its height,
  which a `zone` and a `path` could already do.
- **Composed plots in the cube turn together.** One drag moves every panel on
  the page, and each keeps the angle its own sentence asked for, so a set of
  views stays a set. Reset returns each panel to where it began.
- **A page states how big it is.** `theme(width =, height =)` added to composed
  plots sizes the figure, the way it sizes one plot drawn alone. Plots set side
  by side divide the width and each keep the whole height, so until now nothing
  could say how much height that was, and two cubes on a page came out small
  with empty bands above and below them. Size is the only thing a page can say:
  every other `theme()` property describes a panel, and those refuse and name
  the plot they belong to.

### Changed

- **The name is written `gog` everywhere, in one case.** The documentation used
  to write `GOG` for the grammar and `gog` for the package you install, which
  asked a reader to carry two spellings of one name. It is now lowercase in every
  place a reader meets it, the way ggplot2 and pandas are written, and the way
  the hex sticker has always read. Nothing about the grammar changed, and
  `GOG_STRICT` and the other environment variables keep their capitals.
- **Dragging a cube turns the cube, not the camera.** Drag right and the side
  facing you moves right; drag down and it tips down to show you the top. Both
  directions are reversed from before, and both now match what other 3-D viewers
  do, so the gesture a reader already knows works here.

### Fixed

- A refused plot in a notebook showed a crash instead of the reason. The engine
  writes a sentence naming what it would not do and what to write instead, and
  the cell buried it under twenty or thirty lines of the notebook's own
  internals, none of which is anywhere you can act. The cell now shows the
  message. Nothing else changes: a script still stops on a refusal, and so does
  every check that reads one.
- Two plots whose tables were never given a name could not be set on one page.
  A table is named so that a layer can find it, and where a binding cannot read
  the name it invents one. Two plots each invented the same one, and composing
  them was then refused as though the author had chosen it twice. The invented
  name gives way instead, so the page draws exactly what naming both tables
  would have drawn. A name you wrote is still yours: two different tables under
  one of those is refused, as before. Python was also giving every unnamed table
  the same name inside a single plot, so a second one there is now `data2`.
- The five controls under every plot were drawn in a fixed grey, legible on a
  white page and close to invisible on a dark one, so a reader working in a dark
  editor had five buttons they could not see. They take their color from the
  text beside them now, and are legible wherever the surrounding words are.
- A colored surface read its height at one corner of each face instead of across
  it. A symmetric field came out asymmetric, and the last row and column of a
  grid colored nothing.
- Two maps composed onto one page drew nothing at all.
- A legend was wider than the labels inside it, which took room from the plot
  beside it, most of all when plots were composed.
- A long title could run off the edge of its plot and lose its first letters.
- `palette()` on a plot that maps no `color` was accepted and then ignored. It is
  refused now, and the message names both ways to get what you meant.
- A plot that draws its guides inside the panel held room beside it for half of a
  tick label it never writes. The cube, the circle and the packing each get that
  room back, and so does a plot whose axis another plot on the page is drawing.
  It is 20 pixels, invisible on a full-size plot and a sixth of what was left of
  a composed cell's panel.

## 0.0.2 (2026-08-03)

### Added

- **`query()`** binds a table that lives in a database instead of in memory.
  Pass a connection your language already knows how to open. The sentence you
  write does not change; only where the rows come from does.
- **`brush()`** selects rows by dragging across a plot. One drag reaches every
  plot on the page that names the same column.
- **`map()`** is a coordinate space for longitude and latitude, with a
  projection under it. Give a region a value and you have a choropleth.
- **`book_table()`** fetches any of the manual's example tables by name, so you
  can run an example without writing a data reader first.
- **Controls under every plot** in a browser: zoom in, zoom out, fit, grab to
  pan, and save as a PNG. A plot in the cube turns under the mouse.
- **Hover** reads the row beneath the pointer.

### Fixed

- A `group` written beside a `color` was discarded, so five marks drew one
  series too few.
- A bound on a logarithmic axis was compared against logged values, so it
  matched nothing.
- Dragging a selection along a categorical axis never found its slots.
- Two coordinate spaces were accepted and then drawn flat without a word.
- Parentheses around a run of marks dropped them silently. They are refused now.
- R returned one byte less than the other three bindings drew.

### Installing

- All four packages are on their registries and install with one command.
- An installed package carries the browser engine, so a 3-D plot turns without
  building anything separately.
- `gog-cli --version` reports which engine a package is using.

### Known limits

- Julia installs the package but not the engine. Build `gog-cli` yourself until
  that ships.
- A plot moves in a browser. In print it is still.
