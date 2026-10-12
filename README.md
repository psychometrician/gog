# gog — a grammar of graphics

<img src="book/images/gog-hex.png" alt="The gog hex sticker: a peach hexagon holding a face built from the package's four operators, a plus and an asterisk as its two eyes, a slash and a vertical bar as its nose, and the package name as its mouth." align="right" width="170">

**Be agog. Use gog.**

One graphics engine, written in Rust, spoken from four languages. Plots are
**specifications, not drawing code**: you say what you want to see, and the engine
decides every stroke.

```r
data(gapminder_2007) + point + x(gdp) + y(life) + color(continent)
```

Read it aloud: *"Given gapminder 2007: points, x is gdp, y is life, color by
continent."* That sentence is the entire program. You named a table, a mark, two
positions and a channel. The engine chose the axes, the ticks, the margins and
the palette, and it drew the legend because a mapped column earns one. That is a
rule, not a convenience.

**📖 The manual is online, and every plot in it was drawn by this engine:
<https://psychometrician.github.io/gog-book/>**

It is also archived and citable:
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.21807652.svg)](https://doi.org/10.5281/zenodo.21807652)

## Five plots, five sentences

Each picture is one sentence, drawn by the engine. Each reads its table from the
book with `gog_table()`, so it runs as written after `library(gog)`. The same
sentence in Python, Julia and JavaScript is folded under each one. Click a
picture to open its page in the manual, where the 3-D plots turn under your
mouse.

<!-- showcase -->

### Every hurricane since 1980, month by month

<a href="https://psychometrician.github.io/gog-book/play.html#frames-on-the-globe"><img src="images/showcase/hurricanes.gif" alt="An animated globe, dark blue, turned toward the western Pacific. Each frame is one month, from January to December, and draws the track of every storm of hurricane strength from 1980 to 2025 in that month, colored by category." width="720"></a>

```r
cyclones <- gog_table("cyclones")
cyclones$month <- factor(cyclones$month, levels = month.abb)
world_borders <- gog_table("world_borders")

p <- data(world_borders) + zone + x(lon) + y(lat) + group(country) +
  style(color = "#1c2636") +
  data(cyclones) + path + x(lon) + y(lat) + group(storm) + color(category) +
  palette(c("mediumvioletred", "darkorange", "gold")) + style(opacity = 0.6) +
  play(month) + globe(turn = 135, tilt = 10) +
  theme(background = "#050a14", grid = "none") +
  title("Every storm of hurricane strength, 1980 to 2025, by month")
save_gif(p, "hurricanes.gif")
```

<details>
<summary>Python, Julia and JavaScript</summary>

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

```julia
cyclones = gog_table("cyclones")
cyclones["month"] = ordered(cyclones["month"],
                            ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                             "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"])
world_borders = gog_table("world_borders")

p = data(world_borders) + zone + x(:lon) + y(:lat) + group(:country) +
    style(color = "#1c2636") +
    data(cyclones) + path + x(:lon) + y(:lat) + group(:storm) + color(:category) +
    palette(["mediumvioletred", "darkorange", "gold"]) + style(opacity = 0.6) +
    play(:month) + globe(turn = 135, tilt = 10) +
    theme(background = "#050a14", grid = "none") +
    title("Every storm of hurricane strength, 1980 to 2025, by month")
save_gif(p, "hurricanes.gif")
```

```js
import { plot, layer, data, gog_table, ordered, col, x, y, z, color, size,
         group, label, play, point, path, zone, ribbon, text, surface, flow,
         palette, style, theme, title, x_label, y_label, z_label, polar, globe,
         save_svg, save_gif } from "grammar-of-graphics";

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

</details>

### A volcano in three dimensions

<a href="https://psychometrician.github.io/gog-book/marks/surface.html#color-by-height-per-face"><img src="images/showcase/volcano.png" alt="Maunga Whau, a volcano in Auckland, drawn as a surface in a cube over east and north, shaded from dark purple at the base to yellow at the summit." width="720"></a>

```r
maunga_whau <- gog_table("maunga_whau")

p <- data(maunga_whau) + surface + x(east) + y(north) + z(elevation) +
  color(elevation) + palette("viridis") +
  z_label("Elevation (m)") + title("Maunga Whau, a volcano in Auckland")
save_svg(p, "volcano.svg")
```

<details>
<summary>Python, Julia and JavaScript</summary>

```python
maunga_whau = gog_table("maunga_whau")

p = (data(maunga_whau) + surface + x(col.east) + y(col.north) + z(col.elevation) +
     color(col.elevation) + palette("viridis") +
     z_label("Elevation (m)") + title("Maunga Whau, a volcano in Auckland"))
save_svg(p, "volcano.svg")
```

```julia
maunga_whau = gog_table("maunga_whau")

p = data(maunga_whau) + surface + x(:east) + y(:north) + z(:elevation) +
    color(:elevation) + palette("viridis") +
    z_label("Elevation (m)") + title("Maunga Whau, a volcano in Auckland")
save_svg(p, "volcano.svg")
```

```js
const maunga_whau = await gog_table("maunga_whau");

const volcano = plot(
  data(maunga_whau), surface, x(col.east), y(col.north), z(col.elevation),
  color(col.elevation), palette("viridis"),
  z_label("Elevation (m)"), title("Maunga Whau, a volcano in Auckland"));
save_svg(volcano, "volcano.svg");
```

</details>

### Trade between countries, as chords

<a href="https://psychometrician.github.io/gog-book/flow.html#the-chord-diagram"><img src="images/showcase/chord.png" alt="A chord diagram: countries around a ring, each joined to its trading partners by bands as wide as the trade between them, colored by the exporting country." width="720"></a>

```r
trade_partners <- gog_table("trade_partners")

p <- data(trade_partners) + x(billions) +
  ribbon * flow(exporter, importer, shared = TRUE) + color(exporter) +
  zone * flow(exporter, importer, shared = TRUE) +
  text * flow(exporter, importer, shared = TRUE) + label(name) +
  polar() + title("Trade between countries, as chords")
save_svg(p, "chord.svg")
```

<details>
<summary>Python, Julia and JavaScript</summary>

```python
trade_partners = gog_table("trade_partners")

p = (data(trade_partners) + x(col.billions) +
     ribbon * flow(col.exporter, col.importer, shared=True) + color(col.exporter) +
     zone * flow(col.exporter, col.importer, shared=True) +
     text * flow(col.exporter, col.importer, shared=True) + label(col.name) +
     polar() + title("Trade between countries, as chords"))
save_svg(p, "chord.svg")
```

```julia
trade_partners = gog_table("trade_partners")

p = data(trade_partners) + x(:billions) +
    ribbon * flow(:exporter, :importer, shared = true) + color(:exporter) +
    zone * flow(:exporter, :importer, shared = true) +
    text * flow(:exporter, :importer, shared = true) + label(:name) +
    polar() + title("Trade between countries, as chords")
save_svg(p, "chord.svg")
```

```js
const trade_partners = await gog_table("trade_partners");

const chords = plot(
  data(trade_partners), x(col.billions),
  layer(ribbon, flow(col.exporter, col.importer, { shared: true })),
  color(col.exporter),
  layer(zone, flow(col.exporter, col.importer, { shared: true })),
  layer(text, flow(col.exporter, col.importer, { shared: true })),
  label(col.name), polar(), title("Trade between countries, as chords"));
save_svg(chords, "chord.svg");
```

</details>

### Fifty-five years of life and income

<a href="https://psychometrician.github.io/gog-book/play.html"><img src="images/showcase/gapminder.gif" alt="An animated scatter plot of countries, GDP per person on a log axis against life expectancy, one frame for every five years from 1952 to 2007, colored by continent and sized by population." width="720"></a>

```r
gm_all <- gog_table("gm_all")

p <- data(gm_all) + point + x(gdp, scale = "log") + y(life) +
  color(continent) + size(population) + play(year) +
  x_label("GDP per person") + y_label("Life expectancy") +
  title("Life and income, 1952 to 2007")
save_gif(p, "gapminder.gif")
```

<details>
<summary>Python, Julia and JavaScript</summary>

```python
gm_all = gog_table("gm_all")

p = (data(gm_all) + point + x(col.gdp, scale="log") + y(col.life) +
     color(col.continent) + size(col.population) + play(col.year) +
     x_label("GDP per person") + y_label("Life expectancy") +
     title("Life and income, 1952 to 2007"))
save_gif(p, "gapminder.gif")
```

```julia
using GrammarOfGraphics: size   # `size` is also a function in Base

gm_all = gog_table("gm_all")

p = data(gm_all) + point + x(:gdp, scale = "log") + y(:life) +
    color(:continent) + size(:population) + play(:year) +
    x_label("GDP per person") + y_label("Life expectancy") +
    title("Life and income, 1952 to 2007")
save_gif(p, "gapminder.gif")
```

```js
const gm_all = await gog_table("gm_all");

const gapminder = plot(
  data(gm_all), point, x(col.gdp, { scale: "log" }), y(col.life),
  color(col.continent), size(col.population), play(col.year),
  x_label("GDP per person"), y_label("Life expectancy"),
  title("Life and income, 1952 to 2007"));
save_gif(gapminder, "gapminder.gif");
```

</details>

### A household's money, as a Sankey diagram

<a href="https://psychometrician.github.io/gog-book/flow.html#links-in-layers"><img src="images/showcase/sankey.png" alt="A Sankey diagram of one month's money: salary and side work flow into income, and income divides into savings, leisure, transport, food, housing and taxes, each of which divides again." width="720"></a>

```r
budget <- gog_table("budget")

p <- data(budget) + y(amount) +
  ribbon * flow(source, target, layered = TRUE) + color(source) +
  zone * flow(source, target, layered = TRUE) +
  text * flow(source, target, layered = TRUE) + label(name) +
  title("A month of a household's money")
save_svg(p, "sankey.svg")
```

<details>
<summary>Python, Julia and JavaScript</summary>

```python
budget = gog_table("budget")

p = (data(budget) + y(col.amount) +
     ribbon * flow(col.source, col.target, layered=True) + color(col.source) +
     zone * flow(col.source, col.target, layered=True) +
     text * flow(col.source, col.target, layered=True) + label(col.name) +
     title("A month of a household's money"))
save_svg(p, "sankey.svg")
```

```julia
budget = gog_table("budget")

p = data(budget) + y(:amount) +
    ribbon * flow(:source, :target, layered = true) + color(:source) +
    zone * flow(:source, :target, layered = true) +
    text * flow(:source, :target, layered = true) + label(:name) +
    title("A month of a household's money")
save_svg(p, "sankey.svg")
```

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

</details>

<!-- /showcase -->

## Why another one

Most plotting tools ask you to memorize chart types. Each type is its own
picture with its own arguments in its own order, so learning one teaches you
little about the next. The best tools are real grammars instead, where small
parts combine. But their rules for combining collect exceptions over the years,
until using them well becomes a specialist's skill.

gog takes its discipline from **Hangeul**, the Korean alphabet designed in 1443.
A learner can pick it up in a morning. The reason is not that it has few
letters, because English has few letters too and still takes years to write
well. The reason is that **its letters combine without exceptions**. The power
was never in the parts. It was in the regularity with which they join.

So a histogram is nothing to memorize. It is `bar * bin`, derived rather than
invented, the way ㅋ is derived from ㄱ. Keep the statistic and change the mark,
and the same counts draw a line, an area or a step. One law stands over the
whole library: **a rule you learn on one mark holds on every mark, or the
library has failed you.**

Every plot has the same shape:

```
data(table) + mark + positions + refinements
```

The design answers to [nine laws](CONTRIBUTING.md#the-nine-laws) — orthogonality,
no exceptions, plain names, bind-once, and five more — with one enemy behind all
of them: *the expert's shortcut*.

## The same plot, in four languages

<p align="center">
  <img src="images/r.png" alt="R" height="76">
  &nbsp;&nbsp;&nbsp;
  <img src="images/python.png" alt="Python" height="76">
  &nbsp;&nbsp;&nbsp;
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="images/julia-dark.png">
    <img src="images/julia.png" alt="Julia" height="76">
  </picture>
  &nbsp;&nbsp;&nbsp;
  <img src="images/javascript.png" alt="JavaScript" height="76">
</p>

One grammar, four spellings. The first three are one sentence written three
times, and they differ only in how a column is named. JavaScript cannot overload
`+ * | /`, so it spells those four operators as six words. (Each logo above
names the language it stands for and belongs to its owner; the attribution is in
[`images/NOTICE`](images/NOTICE).)

```r
# R
data(gm) + point + x(gdp) + y(life) + color(continent)
```
```python
# Python
data(gm) + point + x(col.gdp) + y(col.life) + color(col.continent)
```
```julia
# Julia
data(gm) + point + x(:gdp) + y(:life) + color(:continent)
```
```js
// JavaScript
plot(data(gm), point, x(col.gdp), y(col.life), color(col.continent))
```

All four produce **byte-identical SVG**, and that is enforced rather than hoped:
three parity harnesses draw every sentence in the manual and compare against R.

## Install

```r
# R
install.packages("gog", repos = c("https://psychometrician.r-universe.dev",
                                  "https://cloud.r-project.org"))
```
```bash
# Python
pip install gog
```
```julia
# Julia
using Pkg; Pkg.add("GrammarOfGraphics")
```
```bash
# JavaScript
npm install grammar-of-graphics
```

All four are live, on
[r-universe](https://psychometrician.r-universe.dev/gog),
[PyPI](https://pypi.org/project/gog/),
[General](https://github.com/JuliaRegistries/General/tree/master/G/GrammarOfGraphics)
and [npm](https://www.npmjs.com/package/grammar-of-graphics). Python and
JavaScript **ship the engine inside the package**, built for your platform.
Julia downloads it with the package, as an artifact, and R ships it on macOS and
Windows. For those there is nothing else to install, nothing to put on your
`PATH`, and no Rust toolchain to set up.

On Linux the R package installs from source and compiles the engine. That install
needs Rust 1.75 or newer and a network connection.

Each of those packages also carries a second copy of the engine, built for the
browser. That is what lets a 3-D plot turn under the mouse on a web page, and a
brush move as you drag it. It is optional: a package without it still draws
every plot, but a 3-D plot does not turn and a brush does not move.

`gog` is not on CRAN yet. That is why the R line names r-universe first and a
CRAN mirror second, and the second entry is what keeps your other packages
resolving.

**What changed between versions** is in two places, and they answer different
questions.
[`NEWS.md`](https://github.com/psychometrician/gog/blob/main/NEWS.md) is the
short version, a few paragraphs per release, and it is what
`news(package = "gog")` prints in R.
[`CHANGELOG.md`](https://github.com/psychometrician/gog/blob/main/CHANGELOG.md)
is the complete one: every change a user could notice, including the small fixes
that never make the headlines.

## The vocabulary

The whole kernel, and it fits on one screen. The combinations *are* the chart
types — there is no `histogram()` to look up.

| Kind | The words |
|---|---|
| **Sources** | `data` `query` |
| **Marks** | `point` `line` `area` `bar` `step` `interval` `box` `ribbon` `text` `path` `rule` `zone` `surface` `edge` |
| **Channels** | `x` `y` `z` `color` `size` `shape` `pattern` `opacity` `group` `label` `play` |
| **Selections** | `brush` |
| **Transforms** | `bin` `smooth` `count` `density` `proportion` `sum` `mean` `median` `max` `min` `quantile` `cumulative` `range` `confidence` `deviation` `smooth_band` `bounds` `partition` `flow` `layout` `cluster`, plus `dodge` `stack` `jitter` `repel` |
| **Scales** | `linear` `log` `time` `category` `order` |
| **Spaces** | `flat` `space` `polar` `nest` `map` `globe` `network` |
| **Labels** | `title` `x_label` `y_label` `z_label` |
| **Settings** | `style` `theme` `palette` |
| **Composition** | `facet`, and the operators: layering `+`, derivation `*`, arranging `\|` and `/` |

Every word above draws today: every mark, every channel, every transform and
every space.

What gog never does is accept one of these names and quietly ignore it. Nothing
is accepted and silently dropped, so a plot that draws is a plot that means what
you wrote.

## Architecture

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="images/rust-dark.png">
    <img src="images/rust.png" alt="Rust" height="64">
  </picture>
</p>

One engine, and all four languages narrow down to it. The four spellings above
are what a person writes. Rust is what runs, below all of them, and it holds no
opinion the grammar did not give it.

<p align="center">
  <img src="images/pipeline.svg" width="100%"
       alt="Four language bindings, one each for R, Python, Julia and JavaScript, all build the same specification. It travels as JSON on standard input to gog-cli, the bridge, and then to gog-core, the engine, which draws the SVG.">
</p>

Each front end is a thin layer that builds a specification, and the engine does
everything else. A rule written into one binding is a rule the other three will
get wrong, so anything more than one binding needs lives in `gog-core`.

The specification describes the *visual*, never one renderer's draw commands, so
a second renderer would change nothing above it. The engine, `gog-core`, depends
on nothing but `serde` and `serde_json`. The command-line bridge adds `resvg` and
`gif`, and uses them only to write a moving plot as an animated GIF.

## Build from source

Requires Rust 1.75 or newer, and the language you want to drive it from.

```bash
cargo build --release
cargo test --release
cargo run --release --example scatter    # also: bar, transforms
```

The engine is language-agnostic. It reads a JSON specification on stdin and
writes SVG to stdout, so anything that can spawn a process can drive it:

```bash
echo '{ … }' | ./target/release/gog-cli > plot.svg
```

For R, install the package from this checkout. `configure` bundles the engine
into it, building the crate from source if no binary is present:

```bash
R CMD INSTALL --no-docs r-pkg/gog
```

## The book

`book/` is the manual, written in Quarto, and **every plot in it is live** —
drawn by the engine as the page builds. Nothing is a screenshot; if a page shows
it, the engine drew it. A 3-D plot turns under your mouse and a brush follows
your drag, because the same engine is compiled for the browser and shipped with
the page. Refusals are live
too. The engine's *no* always comes with a direction, and that is half of what
the manual teaches.

```bash
cd book && quarto preview
```

It opens with the whole grammar in one sitting. Then it takes each family of
atoms in depth, then composition, then a cookbook. The cookbook is organized by
your data's shape and your question, never by chart name. After it come the
reasons behind the nine laws and the engine's refusals, and one chapter for each
language. Its examples are shown in all four languages. The appendices hold a
one-page kernel card, the functions around the grammar, a grid of which words
combine, chart names translated into sentences, a phrasebook for ggplot2 users,
and a comparison of what gog and ggplot2 each draw.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the nine laws the grammar obeys, the
module map, the file-organization rules, and what to run before opening a pull
request.

## License

Code is **Apache License 2.0** — see [LICENSE](LICENSE), and [NOTICE](NOTICE) for
the color schemes this project carries from elsewhere. The Python, JavaScript and
Julia packages keep their own copies of both: a wheel, an npm tarball and a Julia
package are each built from a directory below this one, so a copy has to sit
there. The R package carries only the `NOTICE`, because R ships the Apache text
itself and its checks ask packages to point at that copy rather than bundle one.

The book's prose is **CC BY-NC-SA 4.0** — see [book/LICENSE.md](book/LICENSE.md).
