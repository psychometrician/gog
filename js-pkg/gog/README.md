# gog for JavaScript

Draw a plot by describing it. You name a table, a mark to draw (points, bars,
lines and so on), and which column goes to which part of the picture. That is
a specification, and `gog` draws it as an SVG. On npm its package is named
`grammar-of-graphics`.

One engine, written in Rust, does the drawing for R, Python, Julia and
JavaScript. You write the specification in your language's own syntax, and the
engine draws the same picture from it in all four, byte for byte.

## Why you might want this

**One vocabulary instead of many chart types.** There is no `histogram()`
function, so there are no arguments that belong to a histogram alone. The
whole vocabulary fits on one page, and each chart type is a combination of
words from it. A histogram is `layer(bar, bin)`: the `bar` mark joined to the `bin` transform,
which counts the rows in each interval. Keep `bin` and change `bar` to
`line`, `area` or `step`, and the same counts are drawn as a line, a filled
area or a staircase. You learn each word once, and it works the same way in
every plot.

**It says no, and says what to write instead.** If you write something the
grammar has no meaning for, `gog` refuses it, and the message says what to
write instead. It never accepts part of what you wrote and then ignores it, so
a plot that is drawn shows everything you wrote.

**The same picture from four languages.** A colleague who works in R,
Python or Julia writes the same specification in that language's syntax, and
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
npm install grammar-of-graphics
```

This one command installs everything. The engine for each platform is a
separate optional dependency, and npm downloads only the one for your
computer. There is nothing to put on your `PATH` and nothing to build.

Every export has help with an example, and an editor shows it when the pointer
rests on the name. The package includes TypeScript types, so a TypeScript
project gets the same help as a JavaScript one.

If you want to change the engine rather than use it, the book's
[JavaScript chapter][js-chapter] shows how to work from a copy of the source
repository.

## Your first plot

```js
import { plot, data, point, x, y, color, col, save_svg }
  from "grammar-of-graphics";

const gm = { gdp: [1000, 8000, 30000],
             life: [52, 68, 79],
             continent: ["Africa", "Asia", "Europe"] };

const p = plot(
  data(gm), point,
  x(col.gdp, { scale: "log" }), y(col.life), color(col.continent));

save_svg(p, "life.svg");
```

Read that aloud: *"Given gm: points, x is gdp on a log scale, y is life, color
by continent."* gog calls a line of code like this a sentence, and each part of
it becomes one phrase of what you read aloud. So you can say a plot in words
first, and then type it.

A table is an object of columns, as above, or an array of rows, one object per
row.

`save_svg()` writes the file, and `render_svg()` returns the same SVG as a
string. In a browser you can put that string into the page, and on a server
you can send it as a response.

For a document or a slide, `save_png(p, "life.png")` saves the same plot as a
PNG picture of 1600 by 1200 pixels. Import `save_png` beside `save_svg` to use
it.

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

The examples use the words imported below. They also use `await` outside a
function, which JavaScript allows only in a module: a `.mjs` file, or a
project whose `package.json` has `"type": "module"`.

```js
import { plot, layer, data, gog_table, ordered, col, x, y, z, color, size,
         group, label, play, point, path, zone, ribbon, text, surface, flow,
         palette, style, theme, title, x_label, y_label, z_label, polar, globe,
         save_svg, save_gif } from "grammar-of-graphics";
```

### Storms of hurricane strength, by calendar month

<a href="https://psychometrician.github.io/gog-book/play.html#frames-on-the-globe"><img src="https://raw.githubusercontent.com/psychometrician/gog/main/images/showcase/hurricanes.gif" alt="An animated globe, dark blue, turned toward the western Pacific. Each frame is one calendar month, from January to December. It draws the track of every storm of hurricane strength in that month, in any year from 1980 to 2025, colored by strength category: 1 or 2, 3 or 4, and 5." width="720"></a>

```js
const cyclones = await gog_table("cyclones");
cyclones.month = ordered(cyclones.month,
  ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
   "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]);
const world_borders = await gog_table("world_borders");

const hurricanes = plot(
  data(world_borders), zone, x(col.lon), y(col.lat), group(col.country),
  style({ color: "#1c2636" }),
  data(cyclones), path, x(col.lon), y(col.lat), group(col.storm),
  color(col.category),
  palette(["mediumvioletred", "darkorange", "gold"]), style({ opacity: 0.6 }),
  play(col.month), globe({ turn: 135, tilt: 10 }),
  theme({ background: "#050a14", grid: "none" }),
  title("Every storm of hurricane strength, 1980 to 2025, by month"));
save_gif(hurricanes, "hurricanes.gif");
```

### A volcano in three dimensions

<a href="https://psychometrician.github.io/gog-book/marks/surface.html#color-by-height-per-face"><img src="https://raw.githubusercontent.com/psychometrician/gog/main/images/showcase/volcano.png" alt="Maunga Whau, a volcano in Auckland, drawn as a 3-D surface in a cube. East and north run along the floor, and elevation goes up. The color goes from dark purple at the base to yellow at the summit." width="720"></a>

```js
const maunga_whau = await gog_table("maunga_whau");

const volcano = plot(
  data(maunga_whau), surface, x(col.east), y(col.north), z(col.elevation),
  color(col.elevation), palette("viridis"),
  z_label("Elevation (m)"), title("Maunga Whau, a volcano in Auckland"));
save_svg(volcano, "volcano.svg");
```

### Trade between countries, as a chord diagram

<a href="https://psychometrician.github.io/gog-book/flow.html#the-chord-diagram"><img src="https://raw.githubusercontent.com/psychometrician/gog/main/images/showcase/chord.png" alt="A chord diagram: countries around a ring, each joined to its trading partners by bands as wide as the trade between them, colored by the exporting country." width="720"></a>

```js
const trade_partners = await gog_table("trade_partners");

const chords = plot(
  data(trade_partners), x(col.billions),
  layer(ribbon, flow(col.exporter, col.importer, { shared: true })),
  color(col.exporter),
  layer(zone, flow(col.exporter, col.importer, { shared: true })),
  layer(text, flow(col.exporter, col.importer, { shared: true })),
  label(col.name), polar(),
  title("Trade between countries, as a chord diagram"));
save_svg(chords, "chord.svg");
```

### Life expectancy against GDP per person

<a href="https://psychometrician.github.io/gog-book/play.html"><img src="https://raw.githubusercontent.com/psychometrician/gog/main/images/showcase/gapminder.gif" alt="An animated scatter plot of countries, with GDP per person on the x axis, on a log scale, and life expectancy on the y axis. There is one frame for every five years from 1952 to 2007. Points are colored by continent and sized by population." width="720"></a>

```js
const gm_all = await gog_table("gm_all");

const gapminder = plot(
  data(gm_all), point, x(col.gdp, { scale: "log" }), y(col.life),
  color(col.continent), size(col.population), play(col.year),
  x_label("GDP per person"), y_label("Life expectancy"),
  title("Life expectancy and GDP per person, 1952 to 2007"));
save_gif(gapminder, "gapminder.gif");
```

### A household's money, as a Sankey diagram

<a href="https://psychometrician.github.io/gog-book/flow.html#links-in-layers"><img src="https://raw.githubusercontent.com/psychometrician/gog/main/images/showcase/sankey.png" alt="A Sankey diagram of one month's money. Salary and side work flow into income, and income divides into taxes, housing, food, transport, leisure and savings. Housing, food, transport and leisure each divide again into smaller costs." width="720"></a>

```js
const budget = await gog_table("budget");

const sankey = plot(
  data(budget), y(col.amount),
  layer(ribbon, flow(col.source, col.target, { layered: true })),
  color(col.source),
  layer(zone, flow(col.source, col.target, { layered: true })),
  layer(text, flow(col.source, col.target, { layered: true })),
  label(col.name), title("A month of a household's money"));
save_svg(sankey, "sankey.svg");
```

## What you can draw

**Marks** — points, lines, bars, areas, steps, error bars, boxes, ribbons,
text, paths, reference lines, shaded regions, surfaces in three dimensions, and
the links of a network.

**Ways to show a column** — position, color, size, shape, texture,
transparency, a text label, or a split into one series per group.

**Transforms, written into the sentence** — binning, counting, shares, sums,
averages, medians, quantiles, running totals, ranges, confidence intervals,
smoothing curves, least-squares lines and density estimates, and the layouts of
treemaps, flow diagrams, networks and cluster trees. The engine computes them,
so you do not compute them before you plot.

**Panels, spaces and animation** — small multiples in rows or columns, several
plots side by side or one above another, a circular (polar) space, a
three-dimensional cube, maps and globes, and animation with one frame for each
value of a column.

**Selection** — a brush selects the rows inside a range you give. Those rows
keep their full color, and the other rows are dimmed. In a web browser you can
also drag the brush with the pointer. On a flat plot, resting the pointer on a
point, a bar or a text label shows the values of its row, with or without a
brush. A mark that a transform moves or summarizes shows none.

**Words and numbers** — a title, a subtitle and a caption, a name for each
axis, and three ways to write a number: as a percent, short with K, M, B or T,
or with the number of decimal places you choose.

Chart types come from these words too: a pie chart is a stacked bar chart in
polar space, and a violin plot is a density drawn as a ribbon.

## Two things to know in JavaScript

**A column is written `col.gdp`.** A plain string is a value, as in
`style({ color: "tomato" })`, so `col.` marks the name of a column. Use
`col["life exp"]` for a name with a space in it.

**Six words do the work of four operators.** R, Python and Julia join the parts
of a plot with `+`, `*`, `|` and `/`. JavaScript cannot give those symbols a new
meaning, so it uses six words instead: `plot`, `layer`, `across`, `down`,
`beside` and `below`.

| In R, Python and Julia | In JavaScript | What it does |
|---|---|---|
| `+` | `plot(…)`, with a comma between the parts | adds a part to the plot |
| `*` | `layer(…)` | joins a mark to a transform, as in `layer(bar, bin)` |
| `\| facet(…)` | `across(…)` | splits the plot into panels side by side, one for each value of a column |
| `/ facet(…)` | `down(…)` | the same, with the panels one above another |
| `\|` between plots | `beside(…)` | puts plots side by side |
| `/` between plots | `below(…)` | puts plots one above another |

Here is a histogram of `life` from the first plot's table, with one panel for
each continent. In R it reads

```r
data(gm) + bar * bin + x(life) | facet(continent)
```

and in JavaScript, after importing the four words the first plot did not use:

```js
import { layer, bar, bin, across } from "grammar-of-graphics";

plot(data(gm), layer(bar, bin), x(col.life), across(col.continent))
```

All the other words are spelled the same in all four languages, with the same
underscores: `render_svg`, `x_label`, `border_color`.

## The book

The book is gog's full manual, free online. The engine draws every plot in it
each time the book is built, so no picture in it is a screenshot.

**<https://psychometrician.github.io/gog-book/>**

The book shows its examples in R, Python, Julia and JavaScript. To run an
example that uses one of the book's tables, read the table first, for example
`const gapminder_2007 = await gog_table("gapminder_2007")`. The book's
[JavaScript chapter][js-chapter] lists what is particular to JavaScript.

## License

Apache License 2.0. See [LICENSE](https://github.com/psychometrician/gog/blob/main/js-pkg/gog/LICENSE) and
[NOTICE](https://github.com/psychometrician/gog/blob/main/js-pkg/gog/NOTICE).

[js-chapter]: https://psychometrician.github.io/gog-book/bindings/javascript.html
[book]: https://psychometrician.github.io/gog-book/
