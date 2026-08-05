"""gog — A Grammar of Graphics, spoken from Python.

A plot is a sentence: a table, a mark, and the channels that bind the mark to
the table's columns.

    from gog import *

    gapminder = {"gdp": [...], "life": [...], "continent": [...]}
    p = data(gapminder) + point + x(col.gdp, scale="log") + y(col.life) + color(col.continent)
    p.save("life.svg")

The same sentence in R is `data(gapminder) + point + x(gdp) + y(life) +
color(continent)`. One engine reads both: the binding builds a specification,
`gog-cli` draws it. Nothing about *what a plot means* lives in this package.

Two Python-specific things are worth knowing before the first plot.

**A column is `col.name`.** Python has no bare names, and in this grammar a
plain string is a *value* (`style(color="tomato")`, `title("...")`), so the
accessor is what keeps "which column?" and "which value?" apart — see
`gog/columns.py` for why the four characters earn their place.

**`from gog import *` shadows five builtins** — `bin`, `sum`, `min`, `max` and
`range` are transforms here. That is the same ruling the R package makes when
it masks `base::range` and `base::sum`: a DSL keeps its own vocabulary, and the
alternative is a grammar that reads `gog.bar * gog.bin + gog.x(...)`. Where a
module needs both, import the package instead (`import gog`) and spell the
sentence with the prefix, or re-import the builtin you need
(`from builtins import sum`).
"""

from .atoms import (
    area,
    bar,
    bin,
    bounds,
    box,
    brush,
    color,
    colour,
    confidence,
    count,
    density,
    dodge,
    facet,
    group,
    interval,
    jitter,
    label,
    line,
    map,
    max,
    mean,
    median,
    min,
    nest,
    opacity,
    order,
    palette,
    partition,
    path,
    pattern,
    play,
    point,
    polar,
    proportion,
    range,
    ribbon,
    rule,
    shape,
    size,
    smooth,
    space,
    stack,
    step,
    style,
    sum,
    surface,
    text,
    theme,
    title,
    x,
    x_label,
    y,
    y_label,
    z,
    z_label,
    zone,
)
from .columns import col
from .errors import GogError
from .render import ordered, render_svg, save_gif
from .spec import Page, Plot, data, query
from .tables import book_table

# The seventh place a version is declared, and the one that hides: the other six
# are manifests a release process reads, this one is *code a user reads*. It sat
# at `0.0.0.dev0` while `pyproject.toml` said `0.0.1`, so every built wheel
# carried metadata and a `__version__` that disagreed — caught only because an
# upgrade in a venv reported the old number back. `test_basic.R`'s drift guard
# now covers this file too; it did not at first, because the guard enumerated
# manifests and this is source.
__version__ = "0.0.3"

__all__ = [
    # the table
    "data",
    "query",
    "col",
    "ordered",
    # marks
    "point",
    "line",
    "path",
    "rule",
    "zone",
    "area",
    "bar",
    "step",
    "interval",
    "box",
    "brush",
    "ribbon",
    "surface",
    "text",
    # transforms
    "bin",
    "smooth",
    "count",
    "density",
    "sum",
    "mean",
    "median",
    "max",
    "min",
    "proportion",
    "range",
    "confidence",
    "bounds",
    "partition",
    "dodge",
    "stack",
    "jitter",
    # positions and spaces
    "x",
    "y",
    "z",
    "space",
    "polar",
    "nest",
    "map",
    # channels
    "color",
    # exported only to be refused: the British spelling names its fix
    "colour",
    "group",
    "size",
    "shape",
    "opacity",
    "label",
    "pattern",
    "play",
    # settings and plot-level atoms
    "style",
    "theme",
    "order",
    "facet",
    "palette",
    "title",
    "x_label",
    "y_label",
    "z_label",
    # rendering
    "render_svg",
    "save_gif",
    "Page",
    "Plot",
    # the book's example tables
    "book_table",
    "GogError",
]
