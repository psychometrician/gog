# Changelog

All four packages share one version number and are released together: `gog` on
CRAN-style repositories and PyPI, `GrammarOfGraphics` on Julia's General, and
`grammar-of-graphics` on npm. A version means the same grammar in every one.

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
