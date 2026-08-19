# Changelog

All four packages share one version number and are released together: `gog` on
CRAN-style repositories and PyPI, `GrammarOfGraphics` on Julia's General, and
`grammar-of-graphics` on npm. A version means the same grammar in every one.

## Unreleased

### Fixed

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
