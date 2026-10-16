# gog for Python

Draw a plot by describing it. You name a table, a mark to draw (points, bars,
lines and so on), and which column goes to which part of the picture. That is
a specification, and `gog` draws it as an SVG.

One engine, written in Rust, does the drawing for R, Python, Julia and
JavaScript. You write the specification in your language's own syntax, and the
engine draws the same picture from it in all four, byte for byte.

## Why you might want this

**One vocabulary instead of many chart types.** There is no `histogram()`
function, so there are no arguments that belong to a histogram alone. The
whole vocabulary fits on one page, and each chart type is a combination of
words from it. A histogram is `bar * bin`: the `bar` mark joined to the `bin` transform,
which counts the rows in each interval. Keep `bin` and change `bar` to
`line`, `area` or `step`, and the same counts are drawn as a line, a filled
area or a staircase. You learn each word once, and it works the same way in
every plot.

**It says no, and says what to write instead.** If you write something the
grammar has no meaning for, `gog` refuses it, and the message says what to
write instead. It never accepts part of what you wrote and then ignores it, so
a plot that is drawn shows everything you wrote.

**The same picture from four languages.** A colleague who works in R,
Julia or JavaScript writes the same specification in that language's syntax, and
gets the same file, byte for byte. So a team that uses different programming
languages can still work on one plot.

**A plot is a specification, not drawing code.** What you build is a
specification, and the engine draws it. So you can save one to a file, compare
two of them, create one from another program, or send one over a network. That is harder in a library where a plot exists only as an object
in your running session.

**Things that usually need a second package are already here.** Three
dimensions, animation, brushing (selecting rows and dimming the rest),
circular layouts, maps, networks, treemaps, flow diagrams and contour maps are
all part of the same vocabulary. None of them is an add-on with a syntax of
its own.

## Install

```bash
pip install gog
```

The wheel includes the drawing engine, so there is nothing else to install and
no Rust toolchain to set up.

## Your first plot

```python
from gog import *

gm = {"gdp": [1000, 8000, 30000],
      "life": [52, 68, 79],
      "continent": ["Africa", "Asia", "Europe"]}

p = data(gm) + point + x(col.gdp, scale="log") + y(col.life) + color(col.continent)
save_svg(p, "life.svg")
```

Read that aloud: *"Given gm: points, x is gdp on a log scale, y is life, color
by continent."* gog calls a line of code like this a sentence, and each part of
it becomes one phrase of what you read aloud. So you can say a plot in words
first, and then type it.

A table is a dict that maps each column name to a list of values, as above.
Any object that has `.columns` and returns a column for `table[name]` works
too, so a pandas or polars `DataFrame` works. `gog` needs neither package.

`save_svg()` writes an SVG, which stays sharp at any size. For a document or a
slide, `save_png(p, "life.png")` saves the same plot as a PNG picture of 1600
by 1200 pixels.

When a plot does not draw as you expect, `save_json(p, "plot.json")` saves what
the engine reads to draw it: the sentence, and every value of each column the
sentence names. The engine draws the same plot from that file, so attach the
file to a bug report. Read it before you share it, because it holds your data.

## Five plots, five sentences

Each picture below was drawn from the code under it. Each example downloads
its table from the website of [the book][book], gog's online manual, with
`gog_table()`. So the code runs without changes on a computer that is
online. Click a picture to open its page in the book, where you can turn the
3-D plots with the pointer.

The examples assume you have already run `from gog import *`.

### Storms of hurricane strength, by calendar month

<a href="https://psychometrician.github.io/gog-book/play.html#frames-on-the-globe"><img src="https://raw.githubusercontent.com/psychometrician/gog/main/images/showcase/hurricanes.gif" alt="An animated globe, dark blue, turned toward the western Pacific. Each frame is one calendar month, from January to December. It draws the track of every storm of hurricane strength in that month, in any year from 1980 to 2025, colored by strength category: 1 or 2, 3 or 4, and 5." width="720"></a>

```python
cyclones = gog_table("cyclones")
cyclones["month"] = ordered(cyclones["month"],
                            ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                             "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"])
world_borders = gog_table("world_borders")

p = (data(world_borders) + zone + x(col.lon) + y(col.lat) + group(col.country) +
     style(color="#1c2636") +
     data(cyclones) + path + x(col.lon) + y(col.lat) + group(col.storm) +
     color(col.category) +
     palette(["mediumvioletred", "darkorange", "gold"]) + style(opacity=0.6) +
     play(col.month) + globe(turn=135, tilt=10) +
     theme(background="#050a14", grid="none") +
     title("Every storm of hurricane strength, 1980 to 2025, by month"))
save_gif(p, "hurricanes.gif")
```

### A volcano in three dimensions

<a href="https://psychometrician.github.io/gog-book/marks/surface.html#color-by-height-per-face"><img src="https://raw.githubusercontent.com/psychometrician/gog/main/images/showcase/volcano.png" alt="Maunga Whau, a volcano in Auckland, drawn as a 3-D surface in a cube. East and north run along the floor, and elevation goes up. The color goes from dark purple at the base to yellow at the summit." width="720"></a>

```python
maunga_whau = gog_table("maunga_whau")

p = (data(maunga_whau) + surface + x(col.east) + y(col.north) + z(col.elevation) +
     color(col.elevation) + palette("viridis") +
     z_label("Elevation (m)") + title("Maunga Whau, a volcano in Auckland"))
save_svg(p, "volcano.svg")
```

### Trade between countries, as a chord diagram

<a href="https://psychometrician.github.io/gog-book/flow.html#the-chord-diagram"><img src="https://raw.githubusercontent.com/psychometrician/gog/main/images/showcase/chord.png" alt="A chord diagram: countries around a ring, each joined to its trading partners by bands as wide as the trade between them, colored by the exporting country." width="720"></a>

```python
trade_partners = gog_table("trade_partners")

p = (data(trade_partners) + x(col.billions) +
     ribbon * flow(col.exporter, col.importer, shared=True) + color(col.exporter) +
     zone * flow(col.exporter, col.importer, shared=True) +
     text * flow(col.exporter, col.importer, shared=True) + label(col.name) +
     polar() + title("Trade between countries, as a chord diagram"))
save_svg(p, "chord.svg")
```

### Life expectancy against GDP per person

<a href="https://psychometrician.github.io/gog-book/play.html"><img src="https://raw.githubusercontent.com/psychometrician/gog/main/images/showcase/gapminder.gif" alt="An animated scatter plot of countries, with GDP per person on the x axis, on a log scale, and life expectancy on the y axis. There is one frame for every five years from 1952 to 2007. Points are colored by continent and sized by population." width="720"></a>

```python
gm_all = gog_table("gm_all")

p = (data(gm_all) + point + x(col.gdp, scale="log") + y(col.life) +
     color(col.continent) + size(col.population) + play(col.year) +
     x_label("GDP per person") + y_label("Life expectancy") +
     title("Life expectancy and GDP per person, 1952 to 2007"))
save_gif(p, "gapminder.gif")
```

### A household's money, as a Sankey diagram

<a href="https://psychometrician.github.io/gog-book/flow.html#links-in-layers"><img src="https://raw.githubusercontent.com/psychometrician/gog/main/images/showcase/sankey.png" alt="A Sankey diagram of one month's money. Salary and side work flow into income, and income divides into taxes, housing, food, transport, leisure and savings. Housing, food, transport and leisure each divide again into smaller costs." width="720"></a>

```python
budget = gog_table("budget")

p = (data(budget) + y(col.amount) +
     ribbon * flow(col.source, col.target, layered=True) + color(col.source) +
     zone * flow(col.source, col.target, layered=True) +
     text * flow(col.source, col.target, layered=True) + label(col.name) +
     title("A month of a household's money"))
save_svg(p, "sankey.svg")
```

## What you can draw

**Marks** — points, lines, bars, areas, steps, error bars, boxes, ribbons,
text, paths, reference lines, shaded regions, surfaces in three dimensions, and
the links of a network.

**Ways to show a column** — position, color, size, shape, texture,
transparency, a text label, or a split into one series per group.

**Transforms, written into the sentence** — binning, counting, shares, sums,
averages, medians, quantiles, running totals, ranges, confidence intervals,
smoothing curves and density estimates, and the layouts of treemaps, flow
diagrams, networks and cluster trees. The engine computes them, so you do not
compute them before you plot.

**Panels, spaces and animation** — small multiples in rows or columns, several
plots side by side or one above another, a circular (polar) space, a
three-dimensional cube, maps and globes, and animation with one frame for each
value of a column.

**Selection** — a brush selects the rows inside a range you give. Those rows
keep their full color, and the other rows are dimmed. In a web browser you can
also drag the brush with the pointer.

Chart types come from these words too: a pie chart is a stacked bar chart in
polar space, and a violin plot is a density drawn as a ribbon.

## Two things to know in Python

**A column is written `col.gdp`.** In R you can write a column's name alone, as
in `x(gdp)`. In Python a bare `gdp` would be a variable, so in `gog` you
write the column called `gdp` as `col.gdp`. A plain string is always a value, as in
`style(color="tomato")`, so `x("gdp")` is refused, and the message names the
correct spelling. Use `col["life exp"]` for a name with a space in it.

**`from gog import *` hides six of Python's built-in functions.** In `gog`,
`bin`, `sum`, `min`, `max` and `range` are transforms, and `map` is a space. To
keep Python's own functions in the same file, write `import gog` and then
`gog.point`, `gog.x` and so on. Or import one of them again with
`from builtins import sum`.

## The book

The book is gog's full manual, free online. The engine draws every plot in it
each time the book is built, so no picture in it is a screenshot.

**<https://psychometrician.github.io/gog-book/>**

The book shows its examples in R, Python, Julia and JavaScript. To run an
example that uses one of the book's tables, read the table first, for example
`gapminder_2007 = gog_table("gapminder_2007")`. The book's
[Python chapter][py-chapter] lists what is particular to Python.

## License

Apache License 2.0. See [LICENSE](https://github.com/psychometrician/gog/blob/main/py-pkg/gog/LICENSE) and
[NOTICE](https://github.com/psychometrician/gog/blob/main/py-pkg/gog/NOTICE). Both files are included in
the package.

[py-chapter]: https://psychometrician.github.io/gog-book/bindings/python.html
[book]: https://psychometrician.github.io/gog-book/
