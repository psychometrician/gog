# gog — a grammar of graphics

<img src="book/images/gog_hex.png" alt="The gog hex sticker: a peach hexagon holding a face built from the package's four operators, a plus and an asterisk as its two eyes, a slash and a vertical bar as its nose, and the package name as its mouth." align="right" width="170">

One graphics engine, written in Rust, spoken from four languages. Plots are
**specifications, not drawing code**: you say what you want to see, and the engine
decides every stroke.

```r
data(gapminder_2007) + point + x(gdp) + y(life) + color(continent)
```

Read it aloud: *"Given gapminder 2007 — points, x is gdp, y is life, color by
continent."* That sentence is the entire program. You named a table, a mark, two
positions and a channel; the engine chose the axes, the ticks, the margins, the
palette, and drew the legend — because a mapped column earns one. That is the
rule, not a convenience.

**📖 The manual is online, and every plot in it was drawn by this engine:
<https://psychometrician.github.io/gog-book/>**

## Why another one

Most plotting tools are logographies: a zoo of chart types, each memorized whole,
each with its own arguments in its own order. The best of them are Englishes —
real grammars, generative and expressive, whose composition rules accumulated
exceptions until fluency became a specialist's skill.

gog takes its discipline from **Hangeul**, the Korean alphabet designed in 1443
and learnable in a morning. Not because its letter set is small — English's is
small too, and takes years — but because **its composition has no exceptions**.
The power was never the atoms; it was the regularity with which they combine.

So a histogram is nothing to memorize. It is `bar * bin` — derived, the way ㅋ is
derived from ㄱ rather than invented fresh. Keep the statistic and change the
shape, and the same counts draw a line, an area or a step. One law stands over
the library: **a rule you learn on one mark holds on every mark, or the library
has failed you.**

Every plot has the same shape:

```
data(table) + mark + positions + refinements
```

The design answers to [nine laws](CONTRIBUTING.md#the-nine-laws) — orthogonality,
no exceptions, plain names, bind-once, and five more — with one enemy behind all
of them: *the expert's convenience*.

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

One grammar, four spellings. The first three are one sentence three times,
differing only in how a column is named; JavaScript cannot overload `+ * | /`, so
it spells those four operators as four words. (Each logo above names the language
it stands for and belongs to its owner; the attribution is in
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

| Language | Package | Today |
|---|---|---|
| **Python** | `gog` | **`pip install gog`** — [live on PyPI](https://pypi.org/project/gog/) |
| **R** | `gog` | not on CRAN yet — build from source, below |
| **Julia** | `GrammarOfGraphics` | not in the General registry yet |
| **JavaScript** | `grammar-of-graphics` | **`npm install grammar-of-graphics`** — [live on npm](https://www.npmjs.com/package/grammar-of-graphics) |

Each binding ships the engine inside it, so there is no second thing to install
and nothing to put on your `PATH`.

## The vocabulary

The whole kernel, and it fits on one screen. The combinations *are* the chart
types — there is no `histogram()` to look up.

| | |
|---|---|
| **Marks** | `point` `line` `area` `bar` `step` `interval` `box` `ribbon` `text` `path` `rule` `zone` `surface` |
| **Channels** | `x` `y` `z` `color` `size` `shape` `pattern` `opacity` `group` `label` `play` |
| **Transforms** | `bin` `smooth` `count` `density` `proportion` `sum` `mean` `median` `max` `min` `range` `confidence` `bounds` `partition`, plus `dodge` `stack` `jitter` |
| **Spaces** | `flat` `space` `polar` `nest` |
| **Settings** | `style` `theme` `palette` |
| **Composition** | layering `+`, derivation `*`, faceting `\|` and `/` |

Two further spaces, `globe` and `map`, are designed but **not built**, and the
engine refuses them by name rather than accepting and ignoring them. That is the
general rule: nothing is accepted and silently dropped, so a plot that draws is a
plot that means what you wrote.

## Architecture

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="images/rust-dark.png">
    <img src="images/rust.png" alt="Rust" height="64">
  </picture>
</p>

One engine, at the narrow end. The four spellings above are what a person
writes; Rust is what runs, downstream of all of them, and it holds no opinion the
grammar did not give it.

```
r-pkg/gog                 ─┐
py-pkg/gog                 │
jl-pkg/GrammarOfGraphics   ├─build spec─▶ JSON ─stdin─▶ gog-cli ─▶ gog-core ─▶ SVG
js-pkg/gog                ─┘                            (bridge)   (the engine)
```

Each front end is a thin DSL that builds a specification. The engine does
everything else. A rule implemented in a binding is a rule the other three will
get wrong, so anything more than one binding needs lives in `gog-core`.

The IR encodes the *visual*, never one backend's draw commands, so a second
renderer changes nothing above it. The workspace depends on nothing but `serde`
and `serde_json`.

## Build from source

Requires a Rust toolchain, and the language you want to drive it from.

```bash
cargo build --release
cargo test --release
cargo run --release --example scatter    # also: bar, transforms
```

The engine is language-agnostic — it reads a JSON specification on stdin and
writes SVG to stdout, so anything that can spawn a process can drive it:

```bash
echo '{ … }' | ./target/release/gog-cli > plot.svg
```

For R, install the package from this checkout; `configure` bundles the engine
into it, building the crate from source if no binary is present:

```bash
R CMD INSTALL --no-docs r-pkg/gog
```

## The book

`book/` is the manual, written in Quarto, and **every plot in it is live** —
drawn by the engine as the page builds. Nothing is a screenshot; if a page shows
it, the engine drew it. Refusals render live too, because the engine's *no* —
always with a direction — is half of what it teaches.

```bash
cd book && quarto preview
```

It opens with the whole grammar in one sitting, then takes each atom family in
depth, then composition, then a cookbook organized by your data's shape and your
question — never by chart name.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the nine laws the grammar obeys, the
module map, the file-organization rules, and what to run before opening a pull
request.

## License

Code is **Apache License 2.0** — see [LICENSE](LICENSE), and [NOTICE](NOTICE) for
the color schemes this project carries from elsewhere. Each binding keeps its own
copy of both, because a wheel, an npm tarball and a Julia package are each built
from a directory this one sits above.

The book's prose is **CC BY-NC-SA 4.0** — see [book/LICENSE.md](book/LICENSE.md).
