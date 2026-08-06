# Changelog

All four packages share one version number and are released together: `gog` on
CRAN-style repositories and PyPI, `GrammarOfGraphics` on Julia's General, and
`grammar-of-graphics` on npm. A version means the same grammar in every one.

## Unreleased

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
  cannot do because it forgets the last one. Click the card to take it off, or
  `unstamp` to take them all off together; `clear` leaves them alone, because a
  stamp is not a selection. A click on empty space still clears the selection,
  as before. Nothing about the sentence changes and the printed page carries no
  stamps: this is a way of reading a plot, like turning a cube. What it is for
  is finding the points worth naming, and naming them is `text`.

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

### Fixed

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
