# atoms.py — the vocabulary: marks, transforms, channels, settings
#
# The mirror of `r-pkg/gog/R/atoms.R`, and the words are the same words: the
# grammar is the engine's, not the binding's, so anything that differs here is
# a bug in one of the two front ends. What each atom *means* is documented once,
# in the book and the spec; this file only says how Python spells it.
#
# Two spellings do differ from R, and both are Python's doing:
#
#   * a column is `col.gdp`, never a bare name (see `columns.py`);
#   * an atom that takes a parameter — `bin`, `density`, `confidence`,
#     `jitter`, `box`, `space`, `polar` — is an *object that is also callable*,
#     so `bar * bin` and `bar * bin(30)` reach the same code path exactly as
#     they do in R, where `*` calls an uncalled transform with its defaults.
#
# The value checks live here rather than in Rust for the reason R's do: the
# caller gets the error at the line that wrote it, and a misspelling never
# reaches the wire as an enum serde cannot decode. What is *legal* — which mark
# takes which channel, whether this transform means anything on that mark —
# stays in the engine's `legality`, where every binding inherits it.

from datetime import date, datetime
from inspect import Parameter, Signature, cleandoc, signature
from textwrap import dedent
from typing import Any, Dict, Mapping, Optional, Sequence, Union

from ._help import with_help
from .columns import Column, column_name
from .errors import GogError
from .render import _epoch_seconds
from .spec import Atom, CallableAtom, _shadowed

# ---------------------------------------------------------------------------
# Help for the words that are objects rather than functions
#
# A mark or a bare transform is an `Atom` instance, so its help is attached to
# the instance (`_help.py`) rather than written as a docstring. Each one's help
# is the paragraph its whole family shares, then what this one word draws, then
# an example that uses it. The examples assume `from gog import *`, print
# nothing, and are run by the test suite.
# ---------------------------------------------------------------------------


def _helped(atom: Atom, summary: str, family: str, about: str, examples: str) -> Atom:
    """Attach help to an exported atom: a one-line summary, the family's shared
    paragraph, what this word does, and its examples."""
    parts = [summary, cleandoc(family), cleandoc(about),
             "Examples\n--------\n" + cleandoc(examples)]
    return with_help(atom, "\n\n".join(parts))


_MARK_FAMILY = """
    A mark is the shape a layer draws. Each layer has exactly one, and a plot
    needs at least one. A mark carries no data of its own: channels say which
    column goes to which part of the picture, and the engine decides every
    stroke from there. A mark is an object, not a function, so it is written
    bare: `data(table) + point + x(col.a) + y(col.b)`. A mark and its required
    positions together are the smallest sentence that draws; neither draws
    alone. Which channels a mark takes is a property of the mark, and a
    channel it cannot carry is refused with a direction rather than ignored.
    Joined to a transform with `*`, a mark draws the transform's summary
    instead of the rows: `bar * count`.
"""

# ---------------------------------------------------------------------------
# Marks — the geometric forms
# ---------------------------------------------------------------------------

point = _helped(Atom("mark", mark="point"), "The point mark: one dot for each row.",
                _MARK_FAMILY, """
    `point` draws one dot, or another symbol chosen with `shape()`, for each
    row. It shows how one measurement changes with another.
""", """
    A mark plus its positions is the smallest sentence that draws.

    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = data(cars) + point + x(col.wt) + y(col.mpg)
""")
line = _helped(Atom("mark", mark="line"), "The line mark: the rows joined in x order.",
               _MARK_FAMILY, """
    `line` connects the rows in x order. It shows how one value changed over
    time.
""", """
    Sales in each year, joined from left to right.

    >>> sales = {"year": [2019, 2020, 2021, 2022, 2023],
    ...          "sales": [120, 135, 128, 150, 162]}
    >>> p = data(sales) + line + x(col.year) + y(col.sales)
""")
path = _helped(Atom("mark", mark="path"), "The path mark: the rows joined in the table's order.",
               _MARK_FAMILY, """
    `path` connects the rows in the order of the table rather than in x
    order, so the path can turn back and cross itself. `style(arrow="end")`
    puts an arrowhead on its last point.
""", """
    Each eruption of a geyser joined to the next one, in the order they were
    recorded.

    >>> geyser = {"eruptions": [3.6, 1.8, 3.333, 2.283, 4.533, 2.883,
    ...                         4.7, 3.6, 1.95, 4.35, 1.833, 3.917],
    ...           "waiting": [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84]}
    >>> p = data(geyser) + path + point + x(col.eruptions) + y(col.waiting)
""")
rule = _helped(Atom("mark", mark="rule"), "The rule mark: a reference line across the panel.",
               _MARK_FAMILY, """
    `rule` draws a reference line across the panel: the threshold everything
    else is compared against. Its position is a column, not a number, so a
    table of three values draws three lines. It marks a value on the axis its
    table has a column for, and spans the other axis.
    `style(reach="edge")` draws a short tick instead, which makes a rug plot:
    one short tick at each value.
""", """
    A line at 20 miles per gallon. The line's own table holds one value, in a
    column named like the plot's `y`.

    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> limit = {"mpg": [20]}
    >>> p = data(cars) + point + x(col.wt) + y(col.mpg) + data(limit) + rule
""")
zone = _helped(Atom("mark", mark="zone"), "The zone mark: a shaded region of the panel.",
               _MARK_FAMILY, """
    `zone` shades a rectangular region of the panel. Where its sides are known
    it is bounded by them, and where they are not it spans the whole panel.
    `zone * bounds(col.low, col.high)` names two sides in columns,
    `zone * bin` cuts both axes into cells (the heatmap), and a category on
    each axis gives each pair of categories its own tile.
""", """
    A target band from 130 to 150 behind the sales. The band's own table holds
    its two ends, and the zone is written first so the line draws over it.

    >>> sales = {"year": [2019, 2020, 2021, 2022, 2023],
    ...          "sales": [120, 135, 128, 150, 162]}
    >>> target = {"lower": [130], "upper": [150]}
    >>> p = (data(sales) + x(col.year) + y(col.sales)
    ...      + data(target) + zone * bounds(col.lower, col.upper)
    ...      + line + point)
""")
area = _helped(Atom("mark", mark="area"), "The area mark: the region under a line, filled.",
               _MARK_FAMILY, """
    `area` fills the region between the values and the baseline.
""", """
    >>> sales = {"year": [2019, 2020, 2021, 2022, 2023],
    ...          "sales": [120, 135, 128, 150, 162]}
    >>> p = data(sales) + area + x(col.year) + y(col.sales)
""")
bar = _helped(Atom("mark", mark="bar"), "The bar mark: a rectangle from the baseline to the value.",
              _MARK_FAMILY, """
    `bar` draws a rectangle from the baseline to the value. It shows how large
    each category is. With a transform it draws a summary: `bar * count` counts
    the rows in each category, and `bar * bin` is the histogram.
""", """
    >>> cars = {"car": ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
    ...                 "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = data(cars) + bar + x(col.car) + y(col.mpg)
""")
step = _helped(Atom("mark", mark="step"), "The step mark: a line that stays level until the value changes.",
               _MARK_FAMILY, """
    `step` draws a line of horizontal and vertical pieces: each value stays
    level until the next row, then the line goes straight up or down.
""", """
    >>> sales = {"year": [2019, 2020, 2021, 2022, 2023],
    ...          "sales": [120, 135, 128, 150, 162]}
    >>> p = data(sales) + step + x(col.year) + y(col.sales)
""")
interval = _helped(Atom("mark", mark="interval"), "The interval mark: a segment from a low value to a high one.",
                   _MARK_FAMILY, """
    `interval` draws a segment from a low value to a high one at each
    position: an error bar, or a range. It needs a low and a high value at
    each position, so it is usually written with a transform that gives
    them, such as `range` or `confidence`: `interval * range`.
""", """
    The lowest and the highest petal length of each species.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = data(flowers) + interval * range + x(col.species) + y(col.petal_length)
""")
ribbon = _helped(Atom("mark", mark="ribbon"), "The ribbon mark: a band between two values across a span.",
                 _MARK_FAMILY, """
    `ribbon` draws a band between two values across a span. Like `interval`
    it needs a low and a high value at each position, so it is usually
    written with a transform that gives them: `ribbon * range`, or
    `ribbon * smooth_band` around a trend.
""", """
    The band from the thinnest tree to the thickest at each age, with the
    mean drawn through it.

    >>> trees = {"age": [118, 484, 664, 1004] * 3,
    ...          "circumference": [30, 58, 87, 115, 33, 69, 111, 156,
    ...                            30, 51, 75, 108]}
    >>> p = (data(trees) + ribbon * range + line * mean
    ...      + x(col.age) + y(col.circumference))
""")
text = _helped(Atom("mark", mark="text"), "The text mark: a string at each position.",
               _MARK_FAMILY, """
    `text` draws the string given by `label` at each position. `label` is its
    required channel, the way `x` and `y` are its position.
""", """
    Each car's name, written at its weight and mileage.

    >>> cars = {"car": ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
    ...                 "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
    ...         "wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = data(cars) + text + x(col.wt) + y(col.mpg) + label(col.car)
""")
# A sheet through the samples, and the one mark that draws in the cube alone. Its
# rows are nodes: the grid the two position columns describe is recovered rather
# than declared, so it wants one row per (x, y) crossing. Three positions, all
# required and all numeric — a face asserts every value *between* two nodes, and
# between two categories there is nothing to assert (for a mesh over categories,
# `bar * bin + space()`). One transform, `density`, which makes it the third
# geometry of one field: `zone * density` paints it as cells, `path * density`
# traces its contours, `surface * density` raises it with the estimate as height.
surface = _helped(Atom("mark", mark="surface"), "The surface mark: a sheet over a grid of samples.",
                  _MARK_FAMILY, """
    `surface` draws a sheet over a grid of samples, for three-dimensional data.
    It takes three positions, `x`, `y` and `z`, all numbers, with one row for
    each point of the grid. It is the only mark that must be drawn in the
    cube, so it needs no `space()` to get there.
""", """
    A small hill, sampled on a 4 by 4 grid.

    >>> hill = {"x": [1, 2, 3, 4] * 4,
    ...         "y": [1] * 4 + [2] * 4 + [3] * 4 + [4] * 4,
    ...         "height": [0.223, 0.435, 0.435, 0.223, 0.435, 0.846, 0.846, 0.435,
    ...                    0.435, 0.846, 0.846, 0.435, 0.223, 0.435, 0.435, 0.223]}
    >>> p = data(hill) + surface + x(col.x) + y(col.y) + z(col.height)
""")
# The stroke whose two endpoints the layout supplies — one row is one edge of a
# graph. Nothing binds to its positions; its minimum syllable is
# `edge * layout(col.a, col.b)` inside `network()`, and like `surface` it does
# not draw flat.
edge = _helped(Atom("mark", mark="edge"), "The edge mark: a stroke between two connected things.",
               _MARK_FAMILY, """
    `edge` draws one stroke for each row of a table of links, between the two
    things the row names. No column says where a thing is placed, so an edge
    takes no `x` and no `y`: `layout` places each thing once, and the edge is
    drawn inside `network()`. `style(arrow="end")` points each edge from the
    first column to the second.
""", """
    Who sends work to whom.

    >>> links = {"sender": ["Ann", "Ann", "Bob", "Cy", "Dee"],
    ...          "receiver": ["Bob", "Cy", "Cy", "Dee", "Ann"]}
    >>> p = data(links) + edge * layout(col.sender, col.receiver) + network()
""")


# `box` — the box-and-whisker mark, with its one knob.
class _Box(CallableAtom):
    __slots__ = ()

    def __call__(self, whiskers: Optional[str] = None) -> Atom:
        if whiskers is not None and whiskers not in ("tukey", "range"):
            raise GogError(
                'gog: `box(whiskers=)` is either "tukey" (the default — whiskers to '
                '1.5*IQR, points beyond drawn as outliers) or "range" (whiskers to the '
                "true min and max, no outliers)."
            )
        atom = Atom("mark", mark="box")
        if whiskers is not None:
            atom.fields["box"] = {"whiskers": whiskers}
        return atom


box = with_help(_Box("mark", mark="box"), """
    box(whiskers=None)

    The box mark: a box-and-whisker summary of each group.

    `box` draws how a column's values spread in each group: a box from the
    first quartile to the third, a line at the median, and whiskers beyond
    it. The mark carries its own summary, so it needs no transform. Used
    bare, `+ box`, it takes the default whiskers; called,
    `+ box(whiskers="range")`, it takes the ones named.

    Parameters
    ----------
    whiskers : str, optional
        "tukey" (the default) runs the whiskers to the most extreme value no
        more than 1.5 times the interquartile range beyond the box, and draws
        the values past that as outliers: the standard box plot. "range" runs
        them to the true minimum and maximum with no outliers: the plain
        five-number summary.

    Returns
    -------
    Atom
        A mark, added to a plot with `+`.

    Examples
    --------
    One box for each species: the median, the quartiles and the whiskers.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "sepal_length": [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5,
    ...                             6.3, 5.8, 7.1, 6.3]}
    >>> b = data(flowers) + box + x(col.species) + y(col.sepal_length)

    Whiskers that reach the smallest and the largest value.

    >>> r = data(flowers) + box(whiskers="range") + x(col.species) + y(col.sepal_length)
""")

# ---------------------------------------------------------------------------
# Transforms — used with `*`:  bar * bin,  line * smooth
# ---------------------------------------------------------------------------

_TRANSFORM_FAMILY = """
    A transform replaces the rows with a summary of them, and the mark then
    draws that summary. It is joined to a mark with `*`, which reads as
    derivation: `bar * count` is a bar of counts, so a histogram is built
    from a mark and a transform, not chosen from a list of chart types. A
    transform is an object, not a function, so it is written bare. The same
    transform behaves the same way on every mark it is legal on: keep the
    statistic and change the mark, and
    the same numbers draw as a line, an area or a step. Transforms chain left
    to right, so `step * bin * cumulative` cuts the column into bins and then
    adds up their counts.
"""

# What a star import does to a name Python already has, said in the help of
# each transform it happens to.
_SHADOWS = """
    After `from gog import *`, the name `{name}` is this transform and not
    Python's builtin. A call such as `{call}` is refused, and the message
    says how to reach Python's: `from builtins import {name}`.
"""

smooth = _helped(Atom("transform", transform="smooth"),
                 "The smooth transform: a fitted trend through the points.",
                 _TRANSFORM_FAMILY, """
    `smooth` draws a fitted trend through the points rather than the points
    themselves: a LOESS curve, which may bend. `smooth_band` is the band
    around it, and `straight` the straight line.
""", """
    >>> geyser = {"eruptions": [3.6, 1.8, 3.333, 2.283, 4.533, 2.883,
    ...                         4.7, 3.6, 1.95, 4.35, 1.833, 3.917],
    ...           "waiting": [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84]}
    >>> p = (data(geyser) + point + line * smooth
    ...      + x(col.eruptions) + y(col.waiting))
""")
straight = _helped(Atom("transform", transform="straight"),
                   "The straight transform: the least-squares line through the points.",
                   _TRANSFORM_FAMILY, """
    `straight` is the least-squares line through the points: a straight fit,
    where `smooth` may bend. It is refused on a log `y` axis, where the line
    would be drawn as a curve. `straight_band` is the band around it.
""", """
    >>> geyser = {"eruptions": [3.6, 1.8, 3.333, 2.283, 4.533, 2.883,
    ...                         4.7, 3.6, 1.95, 4.35, 1.833, 3.917],
    ...           "waiting": [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84]}
    >>> p = (data(geyser) + point + line * straight
    ...      + x(col.eruptions) + y(col.waiting))
""")
count = _helped(Atom("transform", transform="count"),
                "The count transform: the number of rows in each group.",
                _TRANSFORM_FAMILY, """
    `count` is the number of rows for each value on `x`, or on `y` when the
    bars run sideways. It needs no other position: the count is the
    measure, and its column is called `count`.
""", """
    Bars of counts: how many cars have each number of cylinders.

    >>> cars = {"cylinders": ["6", "6", "4", "6", "8", "6", "8", "4"]}
    >>> h = data(cars) + bar * count + x(col.cylinders)

    The same statistic, a different shape.

    >>> l = data(cars) + line * count + x(col.cylinders)

    A mark joined to a transform is one layer, and can be named.

    >>> counts = bar * count
    >>> c = data(cars) + counts + x(col.cylinders)
""")
sum = _helped(Atom("transform", transform="sum"),
              "The sum transform: the total of the measure in each group.",
              _TRANSFORM_FAMILY, """
    `sum` adds up the measure, usually the column on `y`, within each group.
""" + _SHADOWS.format(name="sum", call="sum([1, 2])"), """
    The people aboard the Titanic in each class, survivors and others added
    together.

    >>> titanic = {"travel_class": ["1st", "2nd", "3rd", "Crew"] * 2,
    ...            "people": [122, 167, 528, 673, 203, 118, 178, 212]}
    >>> p = data(titanic) + bar * sum + x(col.travel_class) + y(col.people)
""")
mean = _helped(Atom("transform", transform="mean"),
               "The mean transform: the mean of the measure in each group.",
               _TRANSFORM_FAMILY, """
    `mean` reduces the measure, usually the column on `y`, to its mean
    within each group.
    `confidence` and `deviation` draw a band around it.
""", """
    The mean mileage of the cars with each number of cylinders.

    >>> cars = {"cylinders": ["6", "6", "4", "6", "8", "6", "8", "4"],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> m = data(cars) + bar * mean + x(col.cylinders) + y(col.mpg)
""")
median = _helped(Atom("transform", transform="median"),
                 "The median transform: the middle value of the measure in each group.",
                 _TRANSFORM_FAMILY, """
    `median` reduces the measure, usually the column on `y`, to its middle
    value within each group. It is `quantile(0.5)` under its plain name.
""", """
    The median circumference of three trees at each age.

    >>> trees = {"age": [118, 484, 664, 1004] * 3,
    ...          "circumference": [30, 58, 87, 115, 33, 69, 111, 156,
    ...                            30, 51, 75, 108]}
    >>> p = data(trees) + line * median + x(col.age) + y(col.circumference)
""")
max = _helped(Atom("transform", transform="max"),
              "The max transform: the largest value of the measure in each group.",
              _TRANSFORM_FAMILY, """
    `max` reduces the measure, usually the column on `y`, to its largest
    value within each group.
""" + _SHADOWS.format(name="max", call="max(1, 2)"), """
    The highest horsepower among the cars with each number of cylinders.

    >>> cars = {"cylinders": ["6", "6", "4", "6", "8", "6", "8", "4"],
    ...         "hp": [110, 110, 93, 110, 175, 105, 245, 62]}
    >>> p = data(cars) + bar * max + x(col.cylinders) + y(col.hp)
""")
min = _helped(Atom("transform", transform="min"),
              "The min transform: the smallest value of the measure in each group.",
              _TRANSFORM_FAMILY, """
    `min` reduces the measure, usually the column on `y`, to its smallest
    value within each group.
""" + _SHADOWS.format(name="min", call="min(1, 2)"), """
    The lowest mileage among the cars with each number of cylinders.

    >>> cars = {"cylinders": ["6", "6", "4", "6", "8", "6", "8", "4"],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = data(cars) + bar * min + x(col.cylinders) + y(col.mpg)
""")
proportion = _helped(Atom("transform", transform="proportion"),
                     "The proportion transform: each group's share of the total.",
                     _TRANSFORM_FAMILY, """
    `proportion` is each group's share of the total, rather than its count.
    The shares are in a column called `proportion`, so
    `y(col.proportion, percent=True)` writes them as percents.
""", """
    >>> cars = {"cylinders": ["6", "6", "4", "6", "8", "6", "8", "4"]}
    >>> p = data(cars) + bar * proportion + x(col.cylinders)
""")

_COLLISION_FAMILY = """
    When several marks land in the same place, something has to decide where
    they go. `stack`, `dodge`, `jitter` and `repel` are collision modifiers:
    they compute no statistic, they move marks that would otherwise cover
    each other. Like transforms they are joined with `*`, so they compose
    with a statistic rather than replacing it: `bar * count * stack` counts
    the rows, then piles the counts.
"""

dodge = _helped(Atom("transform", transform="dodge"),
                "The dodge modifier: marks that share a place, set side by side.",
                _COLLISION_FAMILY, """
    `dodge` places the marks of each group side by side, so each is measured
    from the same baseline and they can be compared directly. On `point` it
    moves each point across its category as far as needed so that no two
    points overlap: the beeswarm.

    It needs groups: a `color`, `pattern` or `group` binding that says which
    mark belongs to which group. Without one there is nothing to dodge, and
    the engine says so. `point * dodge` needs none, since the points in one
    category already overlap.
""", """
    Side by side: each bar measured from the same baseline.

    >>> titanic = {"travel_class": ["1st", "2nd", "3rd", "Crew"] * 2,
    ...            "survived": ["No"] * 4 + ["Yes"] * 4,
    ...            "people": [122, 167, 528, 673, 203, 118, 178, 212]}
    >>> d = (data(titanic) + bar * sum * dodge + x(col.travel_class)
    ...      + y(col.people) + color(col.survived))
""")
# The fourth collision modifier, and the one whose collision is ink: `text * repel`
# moves labels that overlap each other, where `dodge`/`stack`/`jitter` move marks
# that share a position. Bare like `dodge`, and for the same reason — a label moves
# as far as the overlap requires and no further, so there is no free spread to knob.
repel = _helped(Atom("transform", transform="repel"),
                "The repel modifier: overlapping labels moved apart until they can be read.",
                _COLLISION_FAMILY, """
    `text * repel` finds where the labels and points are drawn and moves each
    label in a different direction until none overlap. When a label has moved
    away from its point, a thin line joins them. When the labels cannot all
    be placed apart, every label is still drawn, and the plot reports how
    many still overlap. `style(nudge=)` moves every label the same way by the
    same distance; `repel` moves each one only as far as needed.
""", """
    Each car's name, pushed away from the others so that no two overlap.

    >>> cars = {"car": ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
    ...                 "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
    ...         "wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = (data(cars) + point + text * repel
    ...      + x(col.wt) + y(col.mpg) + label(col.car))
""")


def _whole_number(value: Any, atom: str, argument: str, example: str) -> int:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise GogError(
            f"gog: `{atom}({argument}=)` needs one positive whole number, e.g. `{example}`."
        )
    if value < 1 or value != int(value):
        raise GogError(
            f"gog: `{atom}({argument}=)` needs one positive whole number, e.g. `{example}`."
        )
    return int(value)


def _one_word(value: Any, argument: str) -> str:
    """A named reading, checked for *shape* only — which words exist is the
    engine's question, so every binding forwards the string and one refusal
    message covers all four."""
    if not isinstance(value, str):
        raise GogError(
            f'gog: `density({argument}=)` takes one word — "shape" or "count".'
        )
    return value


def _positive(value: Any, atom: str, argument: str, example: str,
              unit: str = "", purpose: str = "") -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or value <= 0:
        raise GogError(
            f"gog: `{atom}({argument}=)` needs one positive number{unit}, "
            f"e.g. `{example}`{purpose}."
        )
    return float(value)


# `bin` — equal-width buckets. How many dimensions it cuts is the mark's answer.
class _Bin(CallableAtom):
    __slots__ = ()

    def __call__(
        self,
        bins: Optional[int] = None,
        width: Optional[float] = None,
        tiling: Optional[str] = None,
    ) -> Atom:
        if bins is not None and width is not None:
            raise GogError(
                "gog: `bin()` takes either `bins` or `width`, not both. Write `bin(30)` "
                "for a bin count or `bin(width=5)` for a bin width."
            )
        if tiling is not None and not isinstance(tiling, str):
            raise GogError('gog: `bin(tiling=)` needs one name, `"rect"` or `"hex"`.')
        return Atom(
            "transform",
            transform="bin",
            bins=None if bins is None else _whole_number(bins, "bin", "bins", "bin(30)"),
            width=None if width is None else _positive(width, "bin", "width", "bin(width=5)"),
            tiling=tiling,
        )


bin = with_help(_Bin("transform", transform="bin"), """
    bin(bins=None, width=None, tiling=None)

    Cut a number column into equal-width bins: the histogram transform.

    `bar * bin` cuts the column on `x` into bins and counts the rows in each,
    which is the histogram. On a `zone` with a number on both axes, the same
    `bin` cuts both axes into cells, which is the heatmap. Used bare,
    `bar * bin`, it chooses the number of bins from the number of rows
    (Sturges' rule); called, `bar * bin(20)`, it takes the number given.

    After `from gog import *`, the name `bin` is this transform and not
    Python's builtin, and `bin(5)` asks for five bins rather than returning
    the string '0b101'. For Python's, write `from builtins import bin`.

    Parameters
    ----------
    bins : int, optional
        The number of bins, one positive whole number. Positional:
        `bin(30)`.
    width : float, optional
        The width of each bin in the column's own units, e.g.
        `bin(width=5)`. Not together with `bins`.
    tiling : str, optional
        How the plane is cut when both axes are binned: "rect" (the default)
        or "hex". Two-dimensional bins only: `zone * bin(tiling="hex")`.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    A histogram: each bar counts the rows that fall in its bin.

    >>> geyser = {"eruptions": [3.6, 1.8, 3.333, 2.283, 4.533, 2.883,
    ...                         4.7, 3.6, 1.95, 4.35, 1.833, 3.917]}
    >>> h = data(geyser) + bar * bin + x(col.eruptions)

    Twenty bins, or bins a quarter of a minute wide.

    >>> h20 = data(geyser) + bar * bin(20) + x(col.eruptions)
    >>> hw = data(geyser) + bar * bin(width=0.25) + x(col.eruptions)
""")


# `density` — the smooth estimate; `levels` cuts a field into bands;
# `compare` says what a violin's width means from one slot to the next.
class _Density(CallableAtom):
    __slots__ = ()

    def __call__(
        self,
        adjust: Optional[float] = None,
        bandwidth: Optional[float] = None,
        levels: Optional[int] = None,
        compare: Optional[str] = None,
        reach: Optional[float] = None,
    ) -> Atom:
        if adjust is not None and bandwidth is not None:
            raise GogError(
                "gog: `density()` takes either `adjust` or `bandwidth`, not both. Write "
                "`density(2)` to scale the automatic bandwidth, or `density(bandwidth=5)` "
                "to set it in the data's own units."
            )
        return Atom(
            "transform",
            transform="density",
            adjust=None if adjust is None else _positive(adjust, "density", "adjust", "density(2)"),
            bandwidth=None
            if bandwidth is None
            else _positive(bandwidth, "density", "bandwidth", "density(bandwidth=5)"),
            levels=None
            if levels is None
            else _whole_number(levels, "density", "levels", "path * density(levels=8)"),
            # One of two words, checked here only for *shape* — the word itself is
            # the engine's question, so a typo gets one message in all four bindings
            # rather than four (`legality::check_density_params`).
            compare=None if compare is None else _one_word(compare, "compare"),
            reach=None
            if reach is None
            else _positive(reach, "density", "reach", "density(reach=2.5)",
                                " of slots", " for overlapping ridges"),
        )


density = with_help(_Density("transform", transform="density"), """
    density(adjust=None, bandwidth=None, levels=None, compare=None, reach=None)

    Estimate the smooth density of a number column (a kernel density estimate).

    `line * density + x(col.a)` draws one smooth curve of where the rows lie.
    With a number on both axes it estimates a two-dimensional density:
    `path * density` traces its contours, and `zone * density` fills between
    them. With a category on
    one axis it draws a violin for each category: `ribbon * density`. Used
    bare, it chooses the bandwidth itself; called, it takes the values given.

    Parameters
    ----------
    adjust : float, optional
        A multiplier on the automatically chosen bandwidth: `density(2)`
        doubles the bandwidth, so the curve is smoother; `density(0.5)` halves
        it. Positional: `density(2)`.
    bandwidth : float, optional
        The bandwidth in the data's own units, e.g. `density(bandwidth=5)`.
        Not together with `adjust`. On a two-dimensional density, such as
        `zone * density`, the two axes measure different things, so one width
        cannot fit both. gog refuses it there and points you to `adjust`.
    levels : int, optional
        How many levels to cut a two-dimensional density into:
        `path * density(levels=8)` traces their boundaries, and
        `zone * density(levels=8)` fills between them. Refused on a density
        curve, which is one line with nothing to cut.
    compare : str, optional
        What a violin's width means from one category to the next. "count"
        (the default) scales each violin by its group's row count, so a thin
        violin is a small group. "shape" draws every violin to the same area,
        so only the shapes are compared. Violins only
        (`ribbon * density + x(<category>) + y(<number>)`); refused on a
        curve and on a two-dimensional density, which have no categories to
        compare.
    reach : float, optional
        How far each violin reaches from its category's line, measured in
        slots (the space one category takes on its axis). The default, 0.4,
        keeps each shape inside its slot, leaving a gap as bars do. Above
        0.5, shapes overlap their neighbors: that is the ridgeline plot,
        `area * density(reach=2.5) + x(<number>) + y(<category>)`. A
        `ribbon` reaches this far on each side and an `area` on one, so a
        half violin is exactly half a violin. Like `compare`, it needs a
        category to give it slots.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    A smooth estimate of how eruption times are spread.

    >>> geyser = {"eruptions": [3.6, 1.8, 3.333, 2.283, 4.533, 2.883,
    ...                         4.7, 3.6, 1.95, 4.35, 1.833, 3.917]}
    >>> d = data(geyser) + line * density + x(col.eruptions)

    Twice the bandwidth: a smoother curve.

    >>> d2 = data(geyser) + line * density(2) + x(col.eruptions)
""")


# `confidence` — the mean's interval per group, 0.95 unless told otherwise.
class _Confidence(CallableAtom):
    __slots__ = ()

    def __call__(self, level: Optional[float] = None) -> Atom:
        if level is not None and (
            isinstance(level, bool)
            or not isinstance(level, (int, float))
            or not 0 < level < 1
        ):
            raise GogError(
                "gog: `confidence(level=)` needs one number strictly between 0 and 1, "
                "e.g. `confidence(0.95)`."
            )
        return Atom(
            "transform",
            transform="confidence",
            level=None if level is None else float(level),
        )


confidence = with_help(_Confidence("transform", transform="confidence"), """
    confidence(level=None)

    The confidence interval of the mean in each group.

    `confidence` gives each group's mean and the interval around it, which
    shows how precisely the mean is known. It gives each group a low and a
    high value, so it is drawn by a mark that spans two values:
    `interval * confidence` draws a whisker with a dot at the mean, and
    `style(center=False)` leaves the dot out. Used bare it
    is the 95% interval; called, it takes the level given.

    Parameters
    ----------
    level : float, optional
        The confidence level, one number strictly between 0 and 1; 0.95 when
        not given. Positional: `confidence(0.99)`.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    The mean of each species, with its 95% confidence interval.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = data(flowers) + interval * confidence + x(col.species) + y(col.petal_length)

    A wider interval, at 99%.

    >>> q = data(flowers) + interval * confidence(0.99) + x(col.species) + y(col.petal_length)
""")


# `smooth_band` — the band around a `smooth` line, 0.95 unless told otherwise.
#
# `smooth`'s pair form, as `confidence` is `mean`'s: `ribbon * smooth_band +
# line * smooth` draws the trend with its band.
class _SmoothBand(CallableAtom):
    __slots__ = ()

    def __call__(self, level: Optional[float] = None) -> Atom:
        if level is not None and (
            isinstance(level, bool)
            or not isinstance(level, (int, float))
            or not 0 < level < 1
        ):
            raise GogError(
                "gog: `smooth_band(level=)` needs one number strictly between 0 and 1, "
                "e.g. `smooth_band(0.95)`."
            )
        return Atom(
            "transform",
            transform="smooth_band",
            level=None if level is None else float(level),
        )


smooth_band = with_help(_SmoothBand("transform", transform="smooth_band"), """
    smooth_band(level=None)

    The band around a `smooth` curve: the fit's confidence band.

    At each point of `smooth`'s curve (a LOESS fit), `smooth_band` gives the
    low and high ends of the confidence band around it, so
    `ribbon * smooth_band + line * smooth` draws the trend with its band.
    Used bare it is the 95% band; called, it takes the level given.

    Parameters
    ----------
    level : float, optional
        The confidence level, one number strictly between 0 and 1; 0.95 when
        not given. Positional: `smooth_band(0.99)`.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    The trend, and how precisely it is known.

    >>> geyser = {"eruptions": [3.6, 1.8, 3.333, 2.283, 4.533, 2.883,
    ...                         4.7, 3.6, 1.95, 4.35, 1.833, 3.917],
    ...           "waiting": [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84]}
    >>> p = (data(geyser) + x(col.eruptions) + y(col.waiting) + point
    ...      + ribbon * smooth_band + line * smooth)
""")


# `straight_band` — the band around a `straight` line, 0.95 unless told otherwise.
#
# `straight`'s pair form, as `smooth_band` is `smooth`'s: the interval of the
# least-squares line's mean, so `ribbon * straight_band + line * straight` draws the
# line with its band. It needs three rows in each group, and two different `x`.
class _StraightBand(CallableAtom):
    __slots__ = ()

    def __call__(self, level: Optional[float] = None) -> Atom:
        if level is not None and (
            isinstance(level, bool)
            or not isinstance(level, (int, float))
            or not 0 < level < 1
        ):
            raise GogError(
                "gog: `straight_band(level=)` needs one number strictly between 0 and 1, "
                "e.g. `straight_band(0.95)`."
            )
        return Atom(
            "transform",
            transform="straight_band",
            level=None if level is None else float(level),
        )


straight_band = with_help(_StraightBand("transform", transform="straight_band"), """
    straight_band(level=None)

    The band around a `straight` line: the interval of the fitted line's mean.

    At each point of `straight`'s least-squares line, `straight_band` gives
    the low and high ends of the confidence interval for the line's mean,
    so
    `ribbon * straight_band + line * straight` draws the line with its band.
    It needs three rows in each group, and two different values of `x`. Used
    bare it is the 95% band; called, it takes the level given.

    Parameters
    ----------
    level : float, optional
        The confidence level, one number strictly between 0 and 1; 0.95 when
        not given. Positional: `straight_band(0.99)`.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    The least-squares line, with its 95% confidence band.

    >>> geyser = {"eruptions": [3.6, 1.8, 3.333, 2.283, 4.533, 2.883,
    ...                         4.7, 3.6, 1.95, 4.35, 1.833, 3.917],
    ...           "waiting": [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84]}
    >>> p = (data(geyser) + x(col.eruptions) + y(col.waiting) + point
    ...      + ribbon * straight_band + line * straight)

    A wider band, at 99%.

    >>> q = (data(geyser) + x(col.eruptions) + y(col.waiting) + point
    ...      + ribbon * straight_band(0.99) + line * straight)
""")


# `deviation` — the spread band per group, one standard deviation unless told.
class _Deviation(CallableAtom):
    __slots__ = ()

    def __call__(self, multiplier: Optional[float] = None) -> Atom:
        if multiplier is not None and (
            isinstance(multiplier, bool)
            or not isinstance(multiplier, (int, float))
            or multiplier <= 0
        ):
            raise GogError(
                "gog: `deviation(multiplier=)` needs one positive number — it counts "
                "standard deviations out from the mean. `deviation` is one, "
                "`deviation(2)` is two."
            )
        return Atom(
            "transform",
            transform="deviation",
            multiplier=None if multiplier is None else float(multiplier),
        )


deviation = with_help(_Deviation("transform", transform="deviation"), """
    deviation(multiplier=None)

    The spread around the mean in each group: the mean, plus and minus some
    standard deviations.

    `deviation` gives each group's mean and a band of standard deviations
    around it, which shows how far the rows lie from the mean. It gives each
    group a low and a high value, so it is drawn by a mark that spans two
    values: `interval * deviation`. Used bare it is one standard deviation on
    each side; called, it takes the number given.

    Parameters
    ----------
    multiplier : float, optional
        How many standard deviations reach each side of the mean, one
        positive number; 1 when not given. Positional: `deviation(2)`.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    The mean of each species, plus and minus one standard deviation.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = data(flowers) + interval * deviation + x(col.species) + y(col.petal_length)

    Two standard deviations.

    >>> q = data(flowers) + interval * deviation(2) + x(col.species) + y(col.petal_length)
""")


# `quantile` — the value at one probability. No default; 0.5 is `median`.
class _Quantile(CallableAtom):
    __slots__ = ()

    def __call__(self, p: Optional[float] = None) -> Atom:
        if p is not None:
            if isinstance(p, bool) or not isinstance(p, (int, float)):
                raise GogError(
                    "gog: `quantile()` takes one number between 0 and 1, the "
                    "probability it reduces to, e.g. `quantile(0.9)`."
                )
            if not 0 <= p <= 1:
                raise GogError(
                    f"gog: `quantile({p})` is not a probability — a quantile is "
                    "between 0 and 1. `quantile(0.9)` is the 90th percentile, "
                    "`quantile(0.5)` the middle."
                )
        return Atom("transform", transform="quantile", p=None if p is None else float(p))


quantile = with_help(_Quantile("transform", transform="quantile"), """
    quantile(p=None)

    The p-th quantile of the measure in each group.

    `quantile(0.9)` reduces the measure, usually the column on `y`, to the
    value that 90% of each group's rows fall below. It has no default: bare `quantile` is refused
    when the plot is drawn, because the only sensible default is the middle,
    and that already has a plain name, `median`.

    Parameters
    ----------
    p : float
        The probability, one number from 0 to 1. Positional: `quantile(0.9)`
        is the 90th percentile. At 0, 0.5 and 1 the plot draws, and gog says
        that `min`, `median` and `max` are the plain names for the same
        numbers.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    The 90th percentile and the median of three trees' circumference at each
    age.

    >>> trees = {"age": [118, 484, 664, 1004] * 3,
    ...          "circumference": [30, 58, 87, 115, 33, 69, 111, 156,
    ...                            30, 51, 75, 108]}
    >>> p = (data(trees) + x(col.age) + y(col.circumference)
    ...      + line * quantile(0.9) + line * median)
""")


# `range` — the band per group, the whole group unless told otherwise.
class _Range(CallableAtom):
    __slots__ = ()

    def __call__(
        self, low: Optional[float] = None, high: Optional[float] = None
    ) -> Atom:
        # Two different mistakes reach here and they want different directions,
        # so the message splits on the shape Python's own builtin is called
        # with. `range(10)` is a whole number out of 0..1, which is nobody's
        # quantile, so that reading is named. A float out of 0..1 is a mistyped
        # band end and gets told about quantiles instead. Both refuse either
        # way; only the direction is chosen.
        #
        # The call is judged whole, not value by value. Python's own `range` takes
        # whole numbers, passed by position, so a float anywhere in the call, or
        # `high` named on its own, can only be meant for gog's: `range(0.25, 75)`
        # and `range(high=75)` were pointed at `builtins.range`.
        builtin_hint = GogError(
            "gog: `range()` takes the band's two ends, each one number "
            "between 0 and 1, e.g. `range(0.25, 0.75)`. For a sequence of "
            "integers, gog shadows that name: use `builtins.range` for "
            "Python's."
        )
        given = [(name, value) for name, value in (("low", low), ("high", high))
                 if value is not None]
        if any(isinstance(value, bool) or not isinstance(value, (int, float))
               for _, value in given):
            raise builtin_hint
        if (low is not None and all(isinstance(value, int) for _, value in given)
                and any(not 0 <= value <= 1 for _, value in given)):
            raise builtin_hint
        for name, value in given:
            if not 0 <= value <= 1:
                raise GogError(
                    f"gog: `range({name}={value})` is not a probability — the "
                    "band's ends are quantiles, so each is between 0 and 1. "
                    "`range(0.25, 0.75)` is the middle half, `range(0.1, 0.9)` the "
                    "middle 80 percent, and bare `range` the whole group."
                )
        return Atom(
            "transform",
            transform="range",
            low=None if low is None else float(low),
            high=None if high is None else float(high),
        )


range = with_help(_Range("transform", transform="range"), """
    range(low=None, high=None)

    A band between two quantiles in each group, for `interval` and `ribbon`.

    `range` gives each group a low and a high value, which is the pair that
    `interval` and `ribbon` need. Used bare it runs from the lowest value in
    each group to the highest; called, it runs between the two quantiles
    given.

    After `from gog import *`, the name `range` is this transform and not
    Python's builtin. A call such as `range(10)` is refused, and the message
    says how to reach Python's: `builtins.range`.

    Parameters
    ----------
    low, high : float, optional
        The band's two ends, as quantile probabilities from 0 to 1.
        Positional: `range(0.25, 0.75)` is the middle half. An end not given
        is that side's extreme, so bare `range` is the whole group and
        `range(high=0.9)` runs from the minimum to the 90th percentile.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    The middle half of each species, from the 25th to the 75th percentile.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = data(flowers) + interval * range(0.25, 0.75) + x(col.species) + y(col.petal_length)
""")


# `jitter` — the categorical-axis spread, a multiple of the default.
class _Jitter(CallableAtom):
    __slots__ = ()

    def __call__(self, amount: Optional[float] = None) -> Atom:
        if amount is not None and (
            isinstance(amount, bool) or not isinstance(amount, (int, float)) or amount < 0
        ):
            raise GogError(
                "gog: `jitter(amount=)` needs one non-negative number — the spread as a "
                "multiple of the default, e.g. `jitter(0.5)` for half, up to `jitter(1.25)`, "
                "the widest spread that keeps every point in its own category."
            )
        return Atom(
            "transform",
            transform="jitter",
            amount=None if amount is None else float(amount),
        )


jitter = with_help(_Jitter("transform", transform="jitter"), """
    jitter(amount=None)

    Move points that fall on the same place apart, along the category axis
    only.

    When many rows share one category, their points are drawn on one line
    and cover each other. `point * jitter` moves each point a little to the
    side, within its own category, so that they can be seen. The measured
    axis is never moved. Used bare it is the default spread; called, it
    takes the multiple given.

    Parameters
    ----------
    amount : float, optional
        The spread as a multiple of the default, one number from 0 to 1.25;
        1 when not given. Positional: `jitter(0.5)` for half the spread. At
        1.25 a point reaches the edge of its category's slot, and more is
        refused when the plot is drawn.

    Returns
    -------
    Atom
        A collision modifier, joined to a mark with `*`.

    Examples
    --------
    Points moved a little sideways, so that they do not hide each other.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "sepal_length": [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5,
    ...                             6.3, 5.8, 7.1, 6.3]}
    >>> p = data(flowers) + point * jitter + x(col.species) + y(col.sepal_length)
""")


# `stack` — the measure-axis pile, optionally filled to 1.
#
# `stack(share=True)` is the 100% stacked bar: every pile divided by its own
# slot's total, so a slot reads as its split's composition rather than its
# size. A parameter here rather than a second reading of `proportion` because
# the two divide by different totals — `proportion` by the whole frame's, this
# by the slot's — and because it composes with any measurement, including a
# `sum` that `proportion` has no column to take.
#
# `stack(baseline=)` says where each pile *hangs*, which is the other free
# choice once the heights are fixed. `"zero"` stands every pile on the axis
# (the default), `"center"` hangs each so its middle is at zero, `"wiggle"`
# chooses the foot that makes the bands as flat as it can — the streamgraph.
# Orthogonal to `share`, which scales the heights rather than placing them.
class _Stack(CallableAtom):
    __slots__ = ()

    def __call__(
        self, share: Optional[bool] = None, baseline: Optional[str] = None
    ) -> Atom:
        if share is not None and not isinstance(share, bool):
            raise GogError(
                "gog: `stack(share=)` is true or false — true fills every pile to 1 "
                "(the 100% stacked bar), false piles the values themselves. For shares "
                "of the whole plot rather than of each slot, `proportion` is the "
                "transform you want."
            )
        if baseline is not None and not isinstance(baseline, str):
            raise GogError(
                'gog: `stack(baseline=)` is one of "zero", "center" or "wiggle" — '
                '"zero" stands every pile on the axis, "center" hangs each pile so its '
                'middle is at zero, "wiggle" chooses the foot that makes the bands as '
                "flat as it can (the streamgraph)."
            )
        return Atom("transform", transform="stack", share=share, baseline=baseline)


stack = with_help(_Stack("transform", transform="stack"), """
    stack(share=None, baseline=None)

    Pile the marks of each group end to end, so the total is the height of
    the pile.

    When several marks land in the same place, something has to decide where
    they go. `stack`, `dodge`, `jitter` and `repel` are collision modifiers:
    they compute no statistic, they move marks that would otherwise cover
    each other. Like transforms they are joined with `*`, so they compose
    with a statistic rather than replacing it: `bar * count * stack` counts
    the rows, then piles the counts.

    `stack` piles the marks of each group end to end, so the total is the
    height of the pile. It needs groups: a `color`, `pattern` or `group`
    binding that says which mark belongs to which group. Without one there is
    nothing to stack, and the engine says so. `box` and `interval` take
    `dodge` but not `stack`. Used bare it piles the values from zero; called,
    it takes the settings given.

    Parameters
    ----------
    share : bool, optional
        True fills every pile to the same height, 1, so the plot shows each
        group's share of its pile rather than its amount: the 100% stacked
        bar. False when not given.
    baseline : str, optional
        Where each pile starts: "zero" (the default) starts every pile at the
        axis, "center" places each pile so its middle is at zero, and "wiggle"
        chooses the start that keeps the bands as flat as it can, which is the
        streamgraph.

    Returns
    -------
    Atom
        A collision modifier, joined to a mark with `*`.

    Examples
    --------
    Piled: the height of the pile is the total.

    >>> titanic = {"travel_class": ["1st", "2nd", "3rd", "Crew"] * 2,
    ...            "survived": ["No"] * 4 + ["Yes"] * 4,
    ...            "people": [122, 167, 528, 673, 203, 118, 178, 212]}
    >>> s = (data(titanic) + bar * sum * stack + x(col.travel_class)
    ...      + y(col.people) + color(col.survived))

    Every pile the same height, so the plot shows shares.

    >>> f = (data(titanic) + bar * sum * stack(share=True) + x(col.travel_class)
    ...      + y(col.people) + color(col.survived))
""")


# `cumulative` — the running total along the measured axis.
#
# Each position's value added to the ones before it, in the order the axis
# draws them, within each split. It adds up what the layer already measures,
# the column on `y` or what a transform before it made, and every mark draws
# the total as it draws any measure. `cumulative(share=True)` ends every
# group's total at 1, which `proportion` cannot say: it divides by the whole
# plot's total.
class _Cumulative(CallableAtom):
    __slots__ = ()

    def __call__(self, share: Optional[bool] = None) -> Atom:
        if share is not None and not isinstance(share, bool):
            raise GogError(
                "gog: `cumulative(share=)` is true or false — true ends every group's "
                "total at 1, false adds the values themselves."
            )
        return Atom("transform", transform="cumulative", share=share)


cumulative = with_help(_Cumulative("transform", transform="cumulative"), """
    cumulative(share=None)

    The running total of a measure, along its axis.

    `cumulative` adds each position's value to the values before it, in the
    order the axis draws its positions: left to right on a number, in the
    categories' own order on a category. It adds up what the layer already
    measures, the column named on `y` or what a transform before it made, and
    every mark draws the total as it draws any measure. The total is the
    column `cumulative`, while the measure's own column keeps each row's
    value. So after `bar * bin * cumulative`, `label(col.count)` writes each
    bin's own count and `label(col.cumulative)` the total so far.

    Each group made by `color` or `group` adds up its own total. A category
    axis is added up in its own order, so sort it in the table rather than
    with `order()`. Used bare it adds up the values themselves; called, it
    takes the setting given.

    Parameters
    ----------
    share : bool, optional
        True divides each group's total by its last, so every curve ends at 1:
        the empirical distribution function of each group. False when not
        given.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    Each year's sales added to the years before it.

    >>> sales = {"year": [2019, 2020, 2021, 2022, 2023],
    ...          "sales": [120, 135, 128, 150, 162]}
    >>> r = data(sales) + line * cumulative + x(col.year) + y(col.sales)

    The cumulative histogram, and one distribution function for each species.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "sepal_length": [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5,
    ...                             6.3, 5.8, 7.1, 6.3]}
    >>> h = data(flowers) + step * bin * cumulative + x(col.sepal_length)
    >>> e = (data(flowers) + step * bin * cumulative(share=True)
    ...      + x(col.sepal_length) + color(col.species))
""")


def bounds(
    lower: Optional[Column] = None,
    upper: Optional[Column] = None,
    start: Optional[Column] = None,
    end: Optional[Column] = None,
) -> Atom:
    """Bounds the table already holds, in named columns.

    `bounds` gives a low and a high value that were computed before the
    plot: a model's intervals, a target band, a period of time. It names the
    columns that hold them, so a mark such as `interval` draws them as they
    are. At least one column is required.

    Parameters
    ----------
    lower, upper : Column, optional
        The columns that give the low and high ends on the axis that
        measures. Required by `ribbon`, `interval`, `line` and `step`.
    start, end : Column, optional
        The columns that give the two ends on the other axis, `zone` only:
        they give a rectangle its other two sides. Leave out a pair and a
        `zone` spans the whole panel on that axis.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    Intervals computed before the plot: the 95% confidence intervals of three
    coefficients of a model, `mpg ~ wt + am + qsec`, fitted to all 32 cars of
    R's `mtcars` table.

    >>> coefs = {"term": ["wt", "am", "qsec"],
    ...          "lo": [-5.37, 0.05, 0.63],
    ...          "hi": [-2.46, 5.83, 1.82]}
    >>> p = data(coefs) + interval * bounds(col.lo, col.hi) + y(col.term)
    """
    if lower is None and upper is None and start is None and end is None:
        raise GogError(
            "gog: `bounds()` needs column names — `bounds(col.lo, col.hi)` bounds the "
            "measure axis, and on a `zone` `bounds(start=col.a, end=col.b)` bounds the "
            "domain axis."
        )
    return Atom(
        "transform",
        transform="bounds",
        lower=None if lower is None else column_name(lower, "bounds"),
        upper=None if upper is None else column_name(upper, "bounds"),
        start=None if start is None else column_name(start, "bounds"),
        end=None if end is None else column_name(end, "bounds"),
    )


def partition(*levels: Column, cross: bool = False) -> Atom:
    """Divide a whole among nested parts: one ring for each level of a hierarchy.

    The hierarchy is given as columns, outermost first. One row of the table
    is one leaf, and `partition(col.group, col.item, col.detail)` says which
    columns spell the path down to it. A missing value ends that branch
    early, so the branches of a real hierarchy can end at different rings.

    `zone * partition(...)` on its own is the icicle, and the same sentence
    with `+ polar()` is the sunburst: adding `polar()` is the only
    difference. `text * partition(...) + label(col.name)` writes each part's
    name on its region.

    The size of each branch comes from the column on `x`: `+ x(col.amount)`
    sizes each part by that column, and with no `x` every leaf counts as 1.
    Compose with
    `proportion` for shares. `y(col.depth, limits=(0, 4))` puts a hole in the
    middle: `depth` is the column the transform writes, each part's ring
    number.

    Every number belongs to a leaf. A part that has children cannot have a
    number of its own: gog could not tell whether its arc is the children's
    total or its own number beside them, so it refuses.

    Parameters
    ----------
    *levels : Column
        The hierarchy's columns, outermost first, such as
        `partition(col.group, col.item)`. At least one.
    cross : bool, default False
        True crosses the levels instead of nesting them down one axis: the
        first divides the width, and the second divides the height within
        each of those columns. That is the mosaic, and because both
        directions are used for the hierarchy, only the leaves are drawn.
        It is a keyword: `partition(col.a, col.b, cross=True)`.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    Rings: the people aboard the Titanic by class, and within each class by
    survival.

    >>> titanic = {"travel_class": ["1st", "2nd", "3rd", "Crew"] * 2,
    ...            "survived": ["No"] * 4 + ["Yes"] * 4,
    ...            "people": [122, 167, 528, 673, 203, 118, 178, 212]}
    >>> r = (data(titanic) + zone * partition(col.travel_class, col.survived)
    ...      + x(col.people) + color(col.travel_class) + polar())

    A mosaic: each class is as wide as its count, and split by survival.

    >>> m = (data(titanic)
    ...      + zone * partition(col.travel_class, col.survived, cross=True)
    ...      + x(col.people) + color(col.survived)
    ...      + style(border_color="white", border_size=2))
    """
    if not levels:
        raise GogError(
            "gog: `partition()` needs the hierarchy's columns, outermost first — "
            "`partition(col.group, col.item, col.detail)` puts `group` on the "
            "innermost ring and `detail` on the rim."
        )
    if not isinstance(cross, bool):
        raise GogError(
            "gog: `partition(cross=)` is true or false — true crosses the levels "
            "(the mosaic: the first divides the width, the second the height "
            "within each column), false nests them down one axis (the icicle, and "
            "the sunburst in `polar()`)."
        )
    return Atom(
        "transform",
        transform="partition",
        levels=[column_name(level, "partition") for level in levels],
        # Sent only when True, so a nested partition's wire form is byte-identical
        # to what it was before this existed — `carry` drops a `None`.
        cross=True if cross else None,
    )


def flow(*stages: Column, shared: bool = False, layered: bool = False) -> Atom:
    """Draw how an amount moves through a series of stages: the flow diagram.

    The stages are given as columns, in reading order. One row of the table
    is one path through all of them, and
    `flow(col["class"], col.sex, col.survived)` runs each row from its first
    stage to its last. Rows with the same path are added together, and any
    column `flow()` does not name is ignored.

    Three marks can draw it. `ribbon * flow(...)` draws the bands, a path's
    amount carried between neighboring stages; `zone * flow(...)` draws each
    stage's slots, stacked with no gaps, so lengths on the count axis stay
    exact; and `text * flow(...) + label(col.name)` writes each slot's name
    on the slot. Layer any two or all three.

    The size of each path comes from the column on `y`: `+ y(col.count)` uses
    that column, and with no `y` every row counts as 1. Bound to `x` instead,
    the count runs the stages from top to bottom. On the band layer,
    `color()` of a stage column colors each band by its category at that
    stage. `color()` of any other categorical column splits each band by its
    values. The slots take their color from `style()`.

    The stage axis comes from the columns given to `flow()`, so `x()` or
    `y()` carries only the count, never a stage. To reorder the stages,
    reorder the arguments.

    Parameters
    ----------
    *stages : Column
        The stage columns, in reading order. At least two, since a flow goes
        from one stage to the next.
    shared : bool, default False
        True when the two columns name one set of places: the place each
        amount leaves and the place it arrives at.
        `flow(col.exporter, col.importer, shared=True)` gives each place one
        slot, whichever column names it, and lays every slot on the one count
        axis. On its own, that draws the arc diagram; in `polar()` the count
        goes around the circle and it draws the chord diagram. Each slot is in
        two parts. The first part, in the slot's color, holds the bands that
        leave the place; the second, a shade darker, holds the bands that
        arrive at it. The panel is white and has no frame unless `theme()` says
        otherwise, since the plot has no numbers to read. It is a keyword.
    layered : bool, default False
        True when each row is a link from one place to another, and the
        places are put in layers computed from the links: the Sankey diagram.
        Each place is in the first layer after every place that sends to
        it, so a band may skip a layer, and a flow may stop before the last
        layer. Links that form a cycle have no layers and are refused. Not
        together with `shared=True`. It is a keyword.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    The people aboard the Titanic, from class to survival. Each band is as
    wide as its count.

    >>> titanic = {"travel_class": ["1st", "2nd", "3rd", "Crew"] * 2,
    ...            "survived": ["No"] * 4 + ["Yes"] * 4,
    ...            "people": [122, 167, 528, 673, 203, 118, 178, 212]}
    >>> p = (data(titanic) + ribbon * flow(col.travel_class, col.survived)
    ...      + y(col.people))
    """
    if len(stages) < 2:
        raise GogError(
            "gog: `flow()` needs at least two stage columns, in reading order — "
            '`flow(col["class"], col.sex, col.survived)` runs each row from its '
            '`col["class"]` to its `col.survived`. One column has no between.'
        )
    if not isinstance(shared, bool):
        raise GogError(
            "gog: `flow(shared=)` is true or false — true lays two columns that name "
            "one set of places on one axis, `flow(col.exporter, col.importer, "
            "shared=True)`; false runs the columns as stages side by side."
        )
    if not isinstance(layered, bool):
        raise GogError(
            "gog: `flow(layered=)` is true or false — true reads each row as a link "
            "and stands the places in layers worked out from the links, "
            "`flow(col.source, col.target, layered=True)`; false runs the columns "
            "as stages side by side."
        )
    return Atom(
        "transform",
        transform="flow",
        stages=[column_name(stage, "flow") for stage in stages],
        # Sent only when True, as `partition`'s `cross` is, so a stage flow's
        # wire form is the bytes it always was.
        shared=True if shared else None,
        layered=True if layered else None,
    )


def layout(from_, to):
    """Place the nodes of a network, computed from a table of links.

    The two columns name each row's two ends, and one row of the table is one
    link between its two values. gog makes one node for every name in either
    column. Each node has a `name` and a `degree`, the number of rows that
    name it. The engine computes every position, the same from any of the
    four languages, so one table is one picture wherever it is drawn.

    Three marks can draw this placement, inside `network()`.
    `edge * layout(col.a, col.b)` draws the links, `point * layout(...)` the
    nodes, and `text * layout(...) + label(col.name)` names them. On the node
    layers, `size(col.degree)` and `color(col.degree)` show that count.

    The positions mean nothing as quantities (the picture can be mirrored
    without changing what the graph says), so nothing binds to `x`, `y` or
    `z`. For three dimensions, give the space an angle:
    `network(turn=30, tilt=25)`.

    Parameters
    ----------
    from_ : Column
        The column naming each link's first end. It is spelled `from_`
        because `from` is a word Python keeps for itself; the column it names
        is an ordinary one.
    to : Column
        The column naming each link's second end.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    Who sends work to whom: the links, the people as points, and their names.

    >>> links = {"sender": ["Ann", "Ann", "Bob", "Cy", "Dee"],
    ...          "receiver": ["Bob", "Cy", "Cy", "Dee", "Ann"]}
    >>> p = (data(links)
    ...      + edge * layout(col.sender, col.receiver)
    ...      + point * layout(col.sender, col.receiver)
    ...      + text * layout(col.sender, col.receiver) * repel + label(col.name)
    ...      + network())
    """
    return Atom(
        "transform",
        transform="layout",
        **{"from": column_name(from_, "layout"), "to": column_name(to, "layout")},
    )


def cluster(value=None, *, over=None):
    """Join the closest leaves, one pair at a time: the cluster tree.

    The leaves are the categories on the categorical position, `x` or `y`,
    and each one is described by a profile: its `value` at every category of
    the `over` column. The engine measures the distance between every pair
    of profiles, joins the closest two, and keeps joining until one tree
    holds them all. Leave the other position out: the engine puts the
    joining distance on it and names that axis Distance.

    Two marks can draw the tree.
    `path * cluster(col.amount, over=col.nutrient) + x(col.food)` draws it:
    each join is drawn at the distance where its branches met, so the reader
    can see which leaves are close and where the groups separate.
    `y(col.food)` instead lays the same tree on its side.
    `zone * cluster(over=col.nutrient)` is the other reading: the tile plot
    unchanged, with the leaf axis's slots put in the tree's leaf order, so
    similar rows are next to each other. On a tile plot the values are
    already on `color`, which is why `value` is left out there.

    Composed with `|` and `/`, a clustered panel decides the order of any
    axis it shares, so a tree drawn beside a tile plot lines up with its
    rows.

    The statistic is fixed: distances are Euclidean on the values as given,
    and branches join at their average distance. Rescale the value column in
    the table first if its units should not affect the distances.

    Parameters
    ----------
    value : Column, optional
        The number column the distances are measured on. Leave it out on a
        tile plot, where `color()` already names the values.
    over : Column, optional
        The profile column: each leaf holds one value at every category of
        it. Leave it out only when the table has one row for each leaf. It is
        a keyword: `cluster(col.value, over=col.measure)`.

    Returns
    -------
    Atom
        A transform, joined to a mark with `*`.

    Examples
    --------
    Eight cars, each measured three ways, ordered so that similar cars are
    next to each other. Each measure is rescaled first, so that horsepower,
    in the hundreds, does not outweigh the other two.

    >>> import statistics
    >>> cars = {"car": ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
    ...                 "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
    ...         "wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
    ...         "hp": [110, 110, 93, 110, 175, 105, 245, 62]}
    >>> def scaled(values):
    ...     center, spread = statistics.mean(values), statistics.stdev(values)
    ...     return [(value - center) / spread for value in values]
    >>> long = {"car": cars["car"] * 3,
    ...         "measure": ["mpg"] * 8 + ["hp"] * 8 + ["wt"] * 8,
    ...         "value": scaled(cars["mpg"]) + scaled(cars["hp"]) + scaled(cars["wt"])}
    >>> p = data(long) + path * cluster(col.value, over=col.measure) + y(col.car)
    """
    fields = {}
    if value is not None:
        fields["value"] = column_name(value, "cluster")
    if over is not None:
        fields["over"] = column_name(over, "cluster")
    return Atom("transform", transform="cluster", **fields)


# ---------------------------------------------------------------------------
# Positions and coordinate spaces — always the plot's, unless a layer says so
# ---------------------------------------------------------------------------

SCALE_NAMES = ("linear", "log", "time", "category")


def _check_scale(scale: Optional[str]) -> Optional[str]:
    if scale is None:
        return None
    if not isinstance(scale, str):
        raise GogError(
            'gog: `scale=` needs a single string, e.g. `x(col.gdp, scale="log")`.'
        )
    if scale not in SCALE_NAMES:
        names = ", ".join(f'"{name}"' for name in SCALE_NAMES)
        raise GogError(f'gog: `scale="{scale}"` is not a scale. gog has {names}.')
    return scale


def _check_base(base: Optional[float]) -> Optional[float]:
    if base is None:
        return None
    if isinstance(base, bool) or not isinstance(base, (int, float)):
        raise GogError(
            'gog: `base=` needs a single number, e.g. `x(col.bits, scale="log", base=2)`.'
        )
    if base <= 1:
        raise GogError(
            f"gog: `base={base}` is not a base a logarithm can have — it must be greater "
            f"than 1. Use 10 (the default), 2 for doublings, or `math.e` for e-foldings."
        )
    return float(base)


Limits = Optional[Sequence[Optional[float]]]


def _check_limits(limits: Limits) -> Limits:
    """The domain the channel runs over, when the data is not the authority.

    Two numbers, either of which may be ``None`` on its own to leave that end to
    the data: ``limits=(0, None)`` pins a baseline and lets the top follow.
    ``None`` is already JSON ``null``, which is the engine's shape for an
    unstated end, so Python's spelling and the wire agree with no special case
    (spec §10).
    """
    if limits is None:
        return None
    if isinstance(limits, (str, bytes)) or not isinstance(limits, Sequence) or len(limits) != 2:
        raise GogError(
            "gog: `limits=` needs two numbers, e.g. `x(col.hour, limits=(0, 24))`. "
            "On a date axis use dates: `(date(2024, 1, 1), date(2024, 12, 31))`. "
            "Use `None` for an end the data should decide: `(0, None)`."
        )
    out: list = []
    for end in limits:
        if end is None:
            out.append(None)
            continue
        # A domain on a temporal axis is written in dates, not epoch arithmetic:
        # `limits=(date(2024, 1, 1), date(2024, 12, 31))`. Converted by the same
        # function `to_wire` converts the *column* by, because representation is
        # the binding's job — and because the two disagreeing silently would put
        # the domain off by a factor of 86400 and exclude every row.
        if isinstance(end, (date, datetime)):
            out.append(_epoch_seconds(end))
            continue
        if isinstance(end, bool) or not isinstance(end, (int, float)):
            raise GogError(
                "gog: `limits=` needs two numbers, e.g. `x(col.hour, limits=(0, 24))`. "
                "On a date axis use dates: `(date(2024, 1, 1), date(2024, 12, 31))`. "
                "Use `None` for an end the data should decide: `(0, None)`."
            )
        out.append(float(end))
    lo, hi = out
    if lo is not None and hi is not None and not lo < hi:
        # Written out rather than with `min`/`max`: inside this module those
        # names are gog's transforms, so calling them here raises the shadowing
        # error at the user instead of the message they need.
        low, high = (lo, hi) if lo < hi else (hi, lo)
        raise GogError(
            f"gog: `limits=({lo}, {hi})` runs backwards or has no width — the first "
            f"number is the low end. Write `({low}, {high})`."
        )
    return out


def _check_decimals(decimals: Any) -> Optional[int]:
    """How many decimal places a label's numbers are printed with (spec §10).

    Fixed places, so a column of labels reads as one column of figures. Checked
    here as a count; the engine says where it is drawn and refuses it on text.
    """
    if decimals is None:
        return None
    if isinstance(decimals, bool) or not isinstance(decimals, (int, float)):
        raise GogError(
            "gog: `decimals=` needs one number, e.g. `label(col.life, decimals=1)`. "
            "It is how many decimal places each number is printed with."
        )
    if int(decimals) != decimals or decimals < 0:
        raise GogError(
            f"gog: `decimals={decimals}` is not a count of decimal places. "
            "Write a whole number from 0 up, e.g. `decimals=1`."
        )
    return int(decimals)


def _check_percent(percent: Any) -> Optional[bool]:
    """Whether a channel's numbers are written as percents (spec §10):
    `y(col.proportion, percent=True)` prints 0.25 as 25%. Checked here as true or
    false; the engine says where it is drawn and refuses it beside `short`."""
    if percent is None:
        return None
    if percent is not True and percent is not False:
        raise GogError(
            "gog: `percent=` is true or false — true writes a number as a percent, "
            "e.g. `y(col.proportion, percent=True)` prints 0.25 as 25%."
        )
    return percent


def _check_short(short: Any) -> Optional[bool]:
    """Whether a label's large numbers are written short, with K, M, B or T
    (spec §10): `label(col.population, short=True)` prints 3.81B. Checked here
    as true or false; the engine says where it is drawn and refuses it on text.
    """
    if short is None:
        return None
    if short is not True and short is not False:
        raise GogError(
            "gog: `short=` is true or false — true writes a large number short, with "
            "K, M, B or T, e.g. `label(col.population, short=True)` prints "
            "3811953827 as 3.81B."
        )
    return short


def _check_tick_count(tick_count: Any) -> Optional[int]:
    """How many ticks an axis should aim for (spec §10).

    A *target*, not a promise: the count picks a step and the step is then
    rounded to a human number, so 8 on a 0..100 axis gets a step of 10 and nine
    ticks. Two is the floor — one tick shows a place but no direction — and the
    engine says so as well, because a binding is not the only way in.
    """
    if tick_count is None:
        return None
    if isinstance(tick_count, bool) or not isinstance(tick_count, (int, float)):
        raise GogError(
            "gog: `tick_count=` needs one number, e.g. `x(col.gdp, tick_count=8)`. "
            "It is how many ticks the axis aims for."
        )
    if int(tick_count) != tick_count:
        raise GogError(
            f"gog: `tick_count={tick_count}` is not a whole number — an axis cannot "
            f"have a fraction of a tick. Try `tick_count={round(tick_count)}`."
        )
    if tick_count < 2:
        raise GogError(
            f"gog: `tick_count={int(tick_count)}` — an axis needs at least two ticks "
            "to show a direction as well as a place. Ask for 2 or more, or leave "
            "`tick_count` off for the default of 5."
        )
    return int(tick_count)


def _check_free(free: Any, name: str) -> bool:
    """`free=True` — fit this axis from each panel's own rows (spec §11).

    A flag rather than a value, because the rest of the question is answered by
    *where* it was written: `y(col.life, free=True)` frees y, `x(...)` frees x.

    Every channel takes it and forwards it, as every channel does `legend=`, and
    the engine decides where it means something: only a position draws an axis
    in each panel, and on any other channel the engine refuses it with that
    direction. Leaving it out of `color()` would answer
    `color(col.continent, free=True)` with a bare `TypeError` that names no fix.
    Only the shape is checked here, and its example is a position's.
    """
    if free is None or free is False:
        return False
    if free is not True:
        axis = name if name in ("x", "y", "z") else "y"
        raise GogError(
            "gog: `free=` is true or false — it says whether this axis is fitted "
            "per panel. Which axis is up to which binding you write it on: "
            f"`{axis}(col.<name>, free=True)` frees {axis}."
        )
    return True


def _check_legend(legend: Any) -> Optional[bool]:
    """`legend=False` — leave this channel's legend out of the plot (spec §10).

    Every channel takes it and forwards it, positions included, and the engine
    decides where it means something: a legend is drawn for `color`, `size`,
    `shape`, `pattern` and `opacity`, and on any other channel the engine
    refuses it with that direction. Deciding here would be a rule the other
    three bindings get wrong, and leaving it out of `x()` would answer
    `x(col.gdp, legend=False)` with a bare `TypeError` that names no fix. Only
    the shape is checked here, so the error lands on the line that wrote it.
    """
    if legend is None:
        return None
    if legend is not True and legend is not False:
        raise GogError(
            "gog: `legend=` is true or false — false leaves this channel's legend out "
            "of the plot, e.g. `color(col.continent, legend=False)` when the names "
            "are written on the plot instead. To choose the side of the plot the "
            "legends sit on, write `theme(legend=\"bottom\")`."
        )
    return legend


def _check_axis(axis: Any) -> Optional[bool]:
    """`axis=False` — leave this position's axis out of the plot (spec §10): its
    tick marks, its numbers and its name.

    `legend=`'s rule one guide over: every channel takes it and forwards it, and
    the engine decides where it means something. Only `x`, `y` and `z` draw an
    axis, and on any other channel the engine refuses it with that direction, in
    all four bindings at once. Only the shape is checked here.
    """
    if axis is None:
        return None
    if axis is not True and axis is not False:
        raise GogError(
            "gog: `axis=` is true or false — false leaves this position's axis out "
            "of the plot, its numbers and its name, e.g. `x(col.lon, axis=False)` on "
            "a map. To remove the gridlines as well, write `theme(grid=\"none\")`."
        )
    return axis


def _position(kind: str, name: str, field: Any, scale: Any, base: Any, limits: Any = None,
              tick_count: Any = None, free: Any = None, legend: Any = None,
              speed: Any = None, axis: Any = None, decimals: Any = None,
              short: Any = None, percent: Any = None) -> Atom:
    return Atom(
        kind,
        field=column_name(field, name),
        scale=_check_scale(scale),
        base=_check_base(base),
        limits=_check_limits(limits),
        tick_count=_check_tick_count(tick_count),
        speed=_check_speed(speed),
        free=_check_free(free, name),
        legend=_check_legend(legend),
        axis=_check_axis(axis),
        decimals=_check_decimals(decimals),
        short=_check_short(short),
        percent=_check_percent(percent),
    )


def _channel(kind: str, field: Any, scale: Any = None, base: Any = None, limits: Any = None,
             tick_count: Any = None, speed: Any = None, free: Any = None,
             legend: Any = None, axis: Any = None, decimals: Any = None,
             short: Any = None, percent: Any = None) -> Atom:
    """A channel with every parameter a binding can carry, each checked for shape
    and forwarded: the engine decides where one means nothing, and says so in the
    same words in all four bindings, where Python would only raise `TypeError`."""
    return Atom(
        kind,
        field=column_name(field, kind),
        scale=_check_scale(scale),
        base=_check_base(base),
        limits=_check_limits(limits),
        tick_count=_check_tick_count(tick_count),
        speed=_check_speed(speed),
        free=_check_free(free, kind),
        legend=_check_legend(legend),
        axis=_check_axis(axis),
        decimals=_check_decimals(decimals),
        short=_check_short(short),
        percent=_check_percent(percent),
    )


# ---------------------------------------------------------------------------
# The parameters every channel takes
#
# Every channel takes the same twelve parameters and forwards them, and the
# engine says where one means nothing. So each channel's help names all twelve,
# and most of what it says is shared by its family: the three positions, the
# three channels that measure, and the five that tell categories apart. The
# entries are written once here and put into each channel's docstring, at its
# `@parameters` line, in the order its signature lists them. An entry left
# empty is one a grouped entry (`scale, base, limits`) already covers.
# ---------------------------------------------------------------------------


def _with_parameters(family: Dict[str, str], **own: str):
    """Write a docstring's Parameters entries at its `@parameters` line: the
    family's entries, with this channel's own in place of any it names."""
    entries = {**family, **own}

    def fill(func):
        if func.__doc__ is None:  # `python -OO` drops docstrings
            return func
        order = list(signature(func).parameters)
        text = "\n".join(dedent(entries[name]).strip("\n") for name in order if entries[name])
        # Python 3.13 strips a docstring's indentation when it compiles it, and
        # older versions keep it, so the entries take the marker's own.
        lines = func.__doc__.splitlines()
        at = next(i for i, line in enumerate(lines) if line.strip() == "@parameters")
        indent = lines[at][: len(lines[at]) - len(lines[at].lstrip())]
        lines[at:at + 1] = [indent + line if line else "" for line in text.splitlines()]
        func.__doc__ = "\n".join(lines)
        return func
    return fill


_SPEED_NOT_TAKEN = """
    speed : float, optional
        Not taken: only `play` changes the plot over time, so only `play` has
        a speed. gog refuses it when the plot is drawn, and says to write it
        in `play()`.
"""
_TICK_COUNT_NOT_TAKEN = """
    tick_count : int, optional
        Not taken: only a position, `x`, `y` or `z`, has an axis with ticks to
        count. gog refuses it when the plot is drawn.
"""
_FREE_NOT_TAKEN = """
    free : bool, default False
        Not taken: `free` fits an axis to each panel's own rows, and only a
        position, `x`, `y` or `z`, has an axis. gog refuses it when the plot
        is drawn, and says to write it on a position.
"""
_AXIS_NOT_TAKEN = """
    axis : bool, optional
        Not taken: only a position, `x`, `y` or `z`, draws an axis. gog
        refuses it when the plot is drawn, and names the three positions.
"""
_LEGEND_TAKEN = """
    legend : bool, optional
        False leaves this channel's legend out of the plot. The column is
        still mapped; only the legend that decodes it is not drawn. It is for
        a plot that says the same thing another way, such as names written at
        the ends of the lines. `theme(legend=)` chooses the side of the plot
        where the legends are drawn.
"""

_POSITION_PARAMETERS = dict(
    field="""
        field : Column
            The column to bind, written `col.name`. Where a transform makes
            the measure, the column is the transform's own, such as
            `col.count`.
    """,
    scale="""
        scale : str, optional
            How a number becomes a position: "linear" (the default) or
            "log". "time" and "category" are accepted only when the column
            already holds dates or categories, and there they change nothing.
            To show numbers as categories, store them as text first, or use
            `ordered()`. A log scale leaves the data in its own units and
            labels the ticks 1, 10, 100. It is not the same as
            plotting the logarithm of the column, which would run the axis 0,
            1, 2, and the reader would have to convert each number back.
    """,
    base="""
        base : float, optional
            The base of a "log" scale, 10 when not given. Use 2 when each
            step doubles, and `math.e` for the natural logarithm. The base
            does not move the data. It changes only where the gridlines fall
            and how the ticks are labeled.
    """,
    limits="""
        limits : tuple of two numbers, optional
            The two ends of the axis, as `(low, high)`; a list works too.
            `None` at either end leaves that end fitted to the data:
            `(0, None)`. On a date axis the ends are dates:
            `(date(2024, 1, 1), date(2024, 12, 31))`.
    """,
    tick_count="""
        tick_count : int, optional
            About how many ticks to draw, 2 or more. The axis may draw a few
            more or fewer, because it rounds the tick values to readable
            numbers.
    """,
    speed=_SPEED_NOT_TAKEN,
    free="""
        free : bool, default False
            When the plot is faceted, True lets each panel fit this axis to
            its own rows instead of sharing one scale across all of them. A
            plot with one panel refuses it.
    """,
    legend="""
        legend : bool, optional
            Not taken by a position, which is read off its axis rather than
            from a legend. gog refuses it when the plot is drawn, and names
            the five channels that draw a legend: `color`, `size`, `shape`,
            `pattern` and `opacity`.
    """,
    axis="""
        axis : bool, optional
            False leaves this axis out of the plot: its tick marks, its
            numbers and its name, and the margin they used. The scale is
            unchanged, so the gridlines and the frame stay;
            `theme(grid="none")` and `theme(frame="none")` remove them.
    """,
    decimals="""
        decimals : int, optional
            How many decimal places each number on the axis is printed with,
            a whole number from 0 to 10. Large numbers keep the letter the axis
            already uses (K, M, B or T). If rounding would print two ticks as
            the same number, the axis draws fewer ticks.
    """,
    short="""
        short : bool, optional
            True writes each number on the axis short, with K (thousands), M
            (millions), B (billions) or T (trillions). With it, an axis that
            reads 1000K, 1500K reads 1M, 1.5M.
    """,
    percent="""
        percent : bool, optional
            True writes each number on the axis as a percent: 0.25 is 25%.
            `limits` stays in the column's own units. Not together with
            `short`.
    """,
)


@_with_parameters(_POSITION_PARAMETERS)
def x(field: Optional[Column] = None, scale: Optional[str] = None, base: Optional[float] = None,
      limits: Limits = None, tick_count: Optional[int] = None, speed: Optional[float] = None,
      free: bool = False, legend: Optional[bool] = None,
      axis: Optional[bool] = None,
      decimals: Optional[int] = None,
      short: Optional[bool] = None,
      percent: Optional[bool] = None) -> Atom:
    """Bind the x axis to a column.

    `x` is the horizontal position. A column of numbers gives a continuous
    axis, and a column of text gives one slot for each category. Written
    before any mark, a position is shared by every layer. Written after a
    mark, it belongs to that mark, and every other layer that names no
    column of its own for this axis reads it too. Either way the plot has
    one x axis.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A position, added to a plot with `+`.

    Examples
    --------
    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
    ...         "hp": [110, 110, 93, 110, 175, 105, 245, 62]}
    >>> p = data(cars) + point + x(col.wt) + y(col.mpg)

    A logarithmic axis, in powers of 10 or of 2.

    >>> q = data(cars) + point + x(col.hp, scale="log") + y(col.mpg)
    >>> r = data(cars) + point + x(col.hp, scale="log", base=2) + y(col.mpg)
    """
    return _position("coord_x", "x", field, scale, base, limits, tick_count, free, legend,
                     speed, axis, decimals, short, percent)


@_with_parameters(_POSITION_PARAMETERS)
def y(field: Optional[Column] = None, scale: Optional[str] = None, base: Optional[float] = None,
      limits: Limits = None, tick_count: Optional[int] = None, speed: Optional[float] = None,
      free: bool = False, legend: Optional[bool] = None,
      axis: Optional[bool] = None,
      decimals: Optional[int] = None,
      short: Optional[bool] = None,
      percent: Optional[bool] = None) -> Atom:
    """Bind the y axis to a column.

    `y` is the vertical position. A column of numbers gives a continuous
    axis, and a column of text gives one slot for each category. Written
    before any mark, a position is shared by every layer. Written after a
    mark, it belongs to that mark, and every other layer that names no
    column of its own for this axis reads it too. Either way the plot has
    one y axis.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A position, added to a plot with `+`.

    Examples
    --------
    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
    ...         "hp": [110, 110, 93, 110, 175, 105, 245, 62]}
    >>> p = data(cars) + point + x(col.wt) + y(col.mpg)

    A logarithmic axis.

    >>> q = data(cars) + point + x(col.wt) + y(col.hp, scale="log")

    An axis that starts at zero.

    >>> r = data(cars) + point + x(col.wt) + y(col.mpg, limits=(0, 40))
    """
    return _position("coord_y", "y", field, scale, base, limits, tick_count, free, legend,
                     speed, axis, decimals, short, percent)


@_with_parameters(_POSITION_PARAMETERS)
def z(field: Optional[Column] = None, scale: Optional[str] = None, base: Optional[float] = None,
      limits: Limits = None, tick_count: Optional[int] = None, speed: Optional[float] = None,
      free: bool = False, legend: Optional[bool] = None,
      axis: Optional[bool] = None,
      decimals: Optional[int] = None,
      short: Optional[bool] = None,
      percent: Optional[bool] = None) -> Atom:
    """Bind the z axis to a column: the third position.

    Binding `z` is what makes a plot three-dimensional: one more position,
    not a chart type. The plot is drawn in a cube, which can be turned on a
    web page, and `space()` sets the angle it is seen from.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A position, added to a plot with `+`.

    Examples
    --------
    A third position puts the plot in a cube.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "sepal_length": [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5,
    ...                             6.3, 5.8, 7.1, 6.3],
    ...            "sepal_width": [3.5, 3, 3.2, 3.1, 3.2, 3.2, 3.1, 2.3,
    ...                            3.3, 2.7, 3, 2.9],
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = (data(flowers) + point + x(col.sepal_length) + y(col.sepal_width)
    ...      + z(col.petal_length) + color(col.species))
    """
    return _position("coord_z", "z", field, scale, base, limits, tick_count, free, legend,
                     speed, axis, decimals, short, percent)


def _degrees(value, atom: str, name: str) -> float:
    """A viewing angle: one finite number of degrees, refused with direction.

    `float("left")` raised a bare `ValueError` here once, which `except
    GogError` missed — the one class every other refusal in the package
    arrives as. Julia and JavaScript run the same check in the same words.
    """
    # The four atoms that take an angle, each with its own call to show.
    # JavaScript has carried these since it was written and the other three did
    # not, which is four copies of one rule drifting rather than four idioms.
    if (
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or value != value
        or value in (float("inf"), float("-inf"))
    ):
        raise GogError(
            f"gog: `{atom}({name}=)` needs a single number of degrees, "
            f"e.g. `{_DEGREE_EXAMPLES[atom]}`."
        )
    return float(value)


_DEGREE_EXAMPLES = {
    "space": "space(45, 20)", "polar": "polar(90)",
    "globe": "globe(178, -18)", "network": "network(30, 25)",
}


# `space` — the angle a 3-D plot is viewed from.
class _Space(CallableAtom):
    __slots__ = ()

    def __call__(self, turn: float = 30, tilt: float = 25) -> Atom:
        return Atom(
            "coord_space",
            turn=_degrees(turn, "space", "turn"),
            tilt=_degrees(tilt, "space", "tilt"),
        )


space = with_help(_Space("coord_space", turn=30.0, tilt=25.0), """
    space(turn=30, tilt=25)

    View a 3-D plot from an angle.

    Binding `z` is what makes a plot three-dimensional: one more position, not
    a chart type. `space` sets the angle that plot is seen from. The angle
    belongs to the space, as `polar()`'s start angle does. It maps no column,
    so it is not a channel. Leave it
    out and a 3-D plot takes a view from above and to one side that shows all
    three axes; bare `+ space` is that same view. On a web page that can run
    gog's engine, a drag turns the cube.

    Parameters
    ----------
    turn : float, default 30
        Degrees the scene is turned around the vertical axis: which side you
        view the scene from.
    tilt : float, default 25
        Degrees the eye is lifted above the floor: how steeply you look down.

    Returns
    -------
    Atom
        A coordinate space, added to a plot with `+`.

    Examples
    --------
    Three measurements of each flower, in a cube turned 60 degrees.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "sepal_length": [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5,
    ...                             6.3, 5.8, 7.1, 6.3],
    ...            "sepal_width": [3.5, 3, 3.2, 3.1, 3.2, 3.2, 3.1, 2.3,
    ...                            3.3, 2.7, 3, 2.9],
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = (data(flowers) + point + x(col.sepal_length) + y(col.sepal_width)
    ...      + z(col.petal_length) + color(col.species) + space(turn=60))
""")


# `polar` — the plane bent into a circle: x is the angle, y the radius.
class _Polar(CallableAtom):
    __slots__ = ()

    def __call__(self, start: float = 0) -> Atom:
        return Atom("coord_polar", start=_degrees(start, "polar", "start"))


polar = with_help(_Polar("coord_polar", start=0.0), """
    polar(start=0)

    Bend the plane into a circle.

    The polar coordinate space. `x` becomes the angle and `y` the radius, so
    the categories along the bottom of a bar chart go around the circle in a
    rose chart. Nothing else about the sentence changes:
    `bar * count + x(col.month)` draws a bar chart on its own and a rose chart
    with `polar()`, from the same words. Bare `+ polar` is the same as
    `+ polar()`.

    The angle axis has no ends: one turn spans exactly the fitted range of
    `x`, so the end of the range meets its start with no gap. Angles run
    clockwise from twelve o'clock.

    Parameters
    ----------
    start : float, default 0
        Degrees clockwise from the top where the circle begins. 0 starts the
        axis at twelve o'clock; -22.5 centers the first of eight categories
        there instead of starting it there.

    Returns
    -------
    Atom
        A coordinate space, added to a plot with `+`.

    Examples
    --------
    Counts of cars by number of cylinders, bent into a circle.

    >>> cars = {"cylinders": ["6", "6", "4", "6", "8", "6", "8", "4"]}
    >>> p = data(cars) + bar * count + x(col.cylinders) + polar()
""")


# `nest` — the panel packed with nested regions: the measure becomes an area.
#
# Callable and bare both work, like every other space. It takes no argument
# because it has no view to set: `space` and `polar` carry an angle you could
# turn the same picture through, and a packing has nothing underneath to turn.
class _Nest(CallableAtom):
    __slots__ = ()

    def __call__(self) -> Atom:
        return Atom("coord_nest")


nest = with_help(_Nest("coord_nest"), """
    nest()

    Pack the panel with nested regions: the treemap.

    The nested coordinate space, and the treemap is what it draws. Every
    row's measure becomes an area rather than a length, and the areas divide
    the panel among them: a region's share of the picture is its share of
    the total. The mark, the statistic and the channels stay the same, which
    is what makes this a space and not a chart type.
    `bar * sum * stack + y(col.revenue) + color(col.region)` is a stacked
    bar on its own, and a pie with `polar()`. Here the same sentence without
    `stack` is a treemap: the packing gives every region its own place, so
    `stack` has nothing to do and is refused.

    A packed panel has no axes: its two directions carry no variable, and
    two neighboring regions are not near each other in the data. So there is
    nothing to label and nothing to tick, and the color legend decodes the
    regions. Bind a category position to split the packing one level
    further: each category gets a region of its own, and its rows are packed
    inside it.

    `dodge`, `stack` and `jitter` are refused here: they decide what happens
    when two marks land in the same place, and in a packing none can. `nest`
    takes no arguments, since it has no view to set, so bare `+ nest` is the
    same as `+ nest()`.

    Returns
    -------
    Atom
        A coordinate space, added to a plot with `+`.

    Examples
    --------
    Each class aboard the Titanic as an area in proportion to its people.

    >>> titanic = {"travel_class": ["1st", "2nd", "3rd", "Crew"] * 2,
    ...            "people": [122, 167, 528, 673, 203, 118, 178, 212]}
    >>> p = (data(titanic) + bar * sum + y(col.people)
    ...      + color(col.travel_class) + nest())
""")


# `globe` — the sphere itself, viewed: x is longitude, y is latitude.
#
# The marks that draw on a `map` draw here, standing at their places on the
# sphere's surface: a point is a place, a path bends along great circles, a
# rule is a meridian or a parallel, and a zone with `group` fills each region
# of a boundary. `turn` and `tilt` name the place the view faces — a bearing
# that wraps and a latitude that stops at the poles — and rows on the far
# half are hidden behind the sphere, with the count said out loud. The
# graticule is the panel grid.
class _Globe(CallableAtom):
    __slots__ = ()

    def __call__(self, turn: float = 0, tilt: float = 0) -> Atom:
        return Atom(
            "coord_globe",
            turn=_degrees(turn, "globe", "turn"),
            tilt=_degrees(tilt, "globe", "tilt"),
        )


globe = with_help(_Globe("coord_globe", turn=0.0, tilt=0.0), """
    globe(turn=0, tilt=0)

    View the sphere itself.

    The globe coordinate space. `x` is longitude and `y` is latitude, both
    in degrees, and each mark is drawn at its place on the sphere's surface.
    The marks that draw on a `map` draw here. `point` is a place, `path` is
    a route bending along great circles, and `text` is a name at a place.
    `rule` draws a line at one coordinate (a meridian from pole to pole, or
    a parallel all the way around), and `zone` with `group()` fills each
    region of a boundary. `bar` with `z()` also draws here, as a spike at
    each place. The graticule, the grid of meridians and parallels, is the
    panel grid, turned off like any other with `theme(grid="none")`.

    A globe shows one half of the earth. `turn` and `tilt` name the place
    the view faces, in the words `space()` uses: `globe(turn=178, tilt=-18)`
    puts Fiji at the center of the disk. Rows on the far half are hidden
    behind the sphere, and gog says how many rather than dropping them in
    silence. On a web page that can run gog's engine, dragging the globe
    turns it.

    Both positions are used for the place. A `bar` can still measure along
    the radius with `z()`, one spike for each row; any other quantity goes
    on `size()` or `color()`. A binned field on the sphere is not drawn yet:
    its correct tiling is hexagonal, because rectangular bins do not cover
    equal areas on a sphere, and that grid is not built.

    Parameters
    ----------
    turn : float, default 0
        Degrees of longitude the view faces. Any number is accepted: 538
        faces the same place as 178.
    tilt : float, default 0
        Degrees of latitude the view faces, from -90 to 90.

    Returns
    -------
    Atom
        A coordinate space, added to a plot with `+`.

    Examples
    --------
    Earthquakes near Fiji, on a globe turned to face them.

    >>> quakes = {"lat": [-20.42, -20.62, -26, -17.97, -20.42, -19.68, -11.7, -28.11],
    ...           "long": [181.62, 181.03, 184.1, 181.66, 181.96, 184.31, 166.1, 181.93]}
    >>> p = (data(quakes) + point + x(col.long) + y(col.lat)
    ...      + globe(turn=178, tilt=-18))
""")


# `network` — the graph-theoretic space, where a `layout` is drawn.
#
# A layout's positions are the graph's, not the data's, so this space draws
# no axes, no ticks and no grid. Stating a viewing angle states the third
# dimension: bare `network()` draws the graph flat, `network(turn=30,
# tilt=25)` computes the placement in a cube and draws it from that angle.
class _Network(CallableAtom):
    __slots__ = ()

    def __call__(self, turn: Optional[float] = None, tilt: Optional[float] = None) -> Atom:
        fields = {}
        if turn is not None:
            fields["turn"] = _degrees(turn, "network", "turn")
        if tilt is not None:
            fields["tilt"] = _degrees(tilt, "network", "tilt")
        return Atom("coord_network", **fields)


network = with_help(_Network("coord_network"), """
    network(turn=None, tilt=None)

    The network space, where the nodes that `layout` places are drawn.

    A layout's positions are the graph's, not the data's, so this space
    draws no axes, no ticks and no grid: there is nothing there for them to
    number. It is the one space a `layout` draws in, and a `layout` is the
    only thing drawn in it.

    Giving a viewing angle makes the network three-dimensional. Bare
    `network`, or
    `network()`, draws the graph flat. `network(turn=30, tilt=25)` computes
    the placement in a cube and draws it from that angle, with an angle not
    given taking the standard view. On a web page that can run gog's engine,
    the cube turns with a drag, the way the cube in `space()` turns.

    Parameters
    ----------
    turn : float, optional
        Degrees the scene is turned around the vertical axis. Give it only
        for a 3-D network.
    tilt : float, optional
        Degrees of elevation. Give it only for a 3-D network.

    Returns
    -------
    Atom
        A coordinate space, added to a plot with `+`.

    Examples
    --------
    >>> links = {"sender": ["Ann", "Ann", "Bob", "Cy", "Dee"],
    ...          "receiver": ["Bob", "Cy", "Cy", "Dee", "Ann"]}
    >>> p = (data(links) + edge * layout(col.sender, col.receiver)
    ...      + point * layout(col.sender, col.receiver) + network())

    The same network, turned and tilted to give it depth.

    >>> q = (data(links) + edge * layout(col.sender, col.receiver)
    ...      + point * layout(col.sender, col.receiver)
    ...      + network(turn=35, tilt=20))
""")


# `map` — the sphere flattened onto the page: x is longitude, y is latitude.
#
# Both positions are spent on the place, so a mark that measures along an axis
# has none left. Carry a quantity on a channel instead: ``size(col.x)`` gives
# the proportional-symbol map and ``color(col.x)`` shades each place.
#
# A sphere cannot be laid flat without giving something up, and area and angle
# cannot both survive. ``preserve`` names which one does: ``"area"`` (the
# default, Equal Earth) keeps every region's true size, which is what a map
# read by area needs; ``"angle"`` (Mercator) keeps every small shape's true
# form and pays for it in area.
class _Map(CallableAtom):
    __slots__ = ()

    def __call__(self, *args: Any, **kwargs: Any) -> Atom:
        # `from gog import *` puts this over Python's builtin, so `map(str, xs)`
        # arrives here, and a one-argument signature answered it with Python's
        # count of arguments, which says nothing about the shadowing. A call
        # shaped like the builtin's, a function first or a second argument, is
        # answered as `sum(...)` is: with what `map` is here and how to reach
        # Python's.
        if len(args) > 1 or (args and callable(args[0])):
            raise GogError(_shadowed(
                "map", "gog's projected coordinate space",
                'add it to a plot: `+ map` or `+ map(preserve="angle")`'))
        # A misspelled `preserve` is a call to gog's `map`, so it is answered as
        # JavaScript answers it, naming the word that was written.
        for key in kwargs:
            if key != "preserve":
                raise GogError(f"gog: `map()` has no `{key}` \u2014 it takes `preserve`.")
        preserve = args[0] if args else kwargs.get("preserve", "area")
        # Validated at the line the caller wrote, rather than at the wire. The
        # engine checks it too — a rule implemented in one binding is a rule the
        # other three get wrong — but a reader is owed the error where they typed
        # it.
        # `GogError`, as every refusal in this package is: `errors.py` records
        # why — `except ValueError` around a plot would swallow this one and
        # keep the other hundred, and a refusal a user cannot catch uniformly
        # is a refusal they stop catching.
        if preserve not in ("area", "angle"):
            raise GogError(
                'gog: `map(preserve=)` takes "area" or "angle". '
                '"area" keeps every region\'s true size, which is what a map read '
                'by area needs; "angle" keeps every small shape\'s true form and '
                "pays for it in area. A sphere cannot do both."
            )
        return Atom("coord_map", preserve=preserve)


# The call a reader writes is `map(preserve="area")`, and `*args, **kwargs` is
# only how a builtin-shaped call reaches its refusal above. So that is the
# signature `inspect.signature(map)`, an editor and `help()` report.
_Map.__call__.__signature__ = Signature([  # type: ignore[attr-defined]
    Parameter("self", Parameter.POSITIONAL_OR_KEYWORD),
    Parameter("preserve", Parameter.POSITIONAL_OR_KEYWORD, default="area"),
])

map = with_help(_Map("coord_map", preserve="area"), """
    map(preserve='area')

    Flatten the sphere onto the page.

    The map coordinate space. `x` is longitude and `y` is latitude, both in
    degrees, and the projection decides where each place lands. Nothing else
    about the sentence changes: `point` is a place, `path` is a route through
    places in the table's order, `text` is a name at a place, and `rule`
    spans the axis it does not name, which on a map is a meridian or a
    parallel.

    Both positions are used for the place, so a mark that measures along an
    axis has none left. Carry a quantity on a channel instead:
    `size(col.amount)` gives the proportional-symbol map, and
    `color(col.amount)` shades each place.

    The panel is shaped by the projection rather than by the page, so
    `theme(ratio=)` is refused here: an equal-area map stretched to fit a box
    is no longer equal-area, though it still looks like a map. Bare `+ map`
    is the same as `+ map()`.

    After `from gog import *`, the name `map` is this space and not Python's
    builtin. A call shaped like the builtin's, such as `map(str, values)`,
    is refused, and the message says how to reach Python's:
    `from builtins import map`.

    Parameters
    ----------
    preserve : str, default 'area'
        What the flattening keeps, since a sphere cannot be laid flat with
        both area and angle intact. "area" draws every region with an area
        in proportion to its true size. It is the default, because a map is
        usually read by area, and it uses the Equal Earth projection, which
        reaches both poles. "angle" keeps every small shape's true form, and
        areas are distorted instead: it uses Mercator, so Greenland is drawn
        as large as Africa, which is fourteen times larger. On Mercator the
        poles are infinitely far away, so the map stops at 85.05 degrees of
        latitude. A row beyond that is drawn at the edge, and gog says how
        many were.

    Returns
    -------
    Atom
        A coordinate space, added to a plot with `+`.

    Examples
    --------
    Earthquakes near Fiji, at their longitude and latitude.

    >>> quakes = {"lat": [-20.42, -20.62, -26, -17.97, -20.42, -19.68, -11.7, -28.11],
    ...           "long": [181.62, 181.03, 184.1, 181.66, 181.96, 184.31, 166.1, 181.93]}
    >>> p = data(quakes) + point + x(col.long) + y(col.lat) + map

    The same places on a map that keeps their shapes rather than their areas.

    >>> q = data(quakes) + point + x(col.long) + y(col.lat) + map(preserve="angle")
""")

# ---------------------------------------------------------------------------
# Channels — they map a column, and earn a legend to decode it
# ---------------------------------------------------------------------------


# What a channel that measures says about each parameter: `color`, `size` and
# `opacity`. Only the column and the scale's ends differ between the three.
_MEASURE_PARAMETERS = dict(
    scale="""
        scale : str, optional
            How a number is read along the scale: "linear" (the default) or
            "log". "time" is accepted only on a column of dates, where it
            changes nothing; "category" is refused, since this channel needs
            numbers. On a log scale, equal ratios get equal steps (1 to 10 is
            the same step as 10 to 100), and the legend still shows the
            column's own values.
    """,
    base="""
        base : float, optional
            The base of a "log" scale, 10 when not given. Use 2 when each
            step doubles, and `math.e` for the natural logarithm. The base
            does not change the picture: the scale is fitted to its own range,
            and the legend shows the same values whatever the base.
    """,
    tick_count=_TICK_COUNT_NOT_TAKEN,
    speed=_SPEED_NOT_TAKEN,
    free=_FREE_NOT_TAKEN,
    legend=_LEGEND_TAKEN,
    axis=_AXIS_NOT_TAKEN,
    decimals="""
        decimals : int, optional
            How many decimal places each number in the legend is printed with,
            a whole number from 0 to 10.
    """,
    short="""
        short : bool, optional
            True writes each number in the legend short, with K (thousands),
            M (millions), B (billions) or T (trillions), as `label()` does.
    """,
    percent="""
        percent : bool, optional
            True writes each number in the legend as a percent: 0.25 is 25%.
            Not together with `short`.
    """,
)

# What a channel that tells categories apart says: `group`, `shape`, `pattern`,
# and with their own exceptions `label` and `play`.
_CATEGORY_PARAMETERS = dict(
    scale="""
        scale, base, limits : optional
            Not taken: this channel tells categories apart rather than
            measuring them, so there is no scale or range along it. gog refuses
            each of them when the plot is drawn.
    """,
    base="",
    limits="",
    tick_count=_TICK_COUNT_NOT_TAKEN,
    speed=_SPEED_NOT_TAKEN,
    free=_FREE_NOT_TAKEN,
    legend=_LEGEND_TAKEN,
    axis=_AXIS_NOT_TAKEN,
    decimals="""
        decimals, short, percent : optional
            Not taken: this channel tells categories apart and prints no
            numbers. gog refuses each of them when the plot is drawn.
    """,
    short="",
    percent="",
)


@_with_parameters(_MEASURE_PARAMETERS, field="""
    field : Column
        The column to bind, written `col.name`. Text picks colors from the
        palette; a number runs along a ramp from low to high.
""", scale="""
    scale : str, optional
        How a number is read along the scale: "linear" (the default) or
        "log". "time" and "category" are accepted only when the column
        already holds dates or categories, and there they change nothing. To
        show numbers as categories, store them as text first, or use
        `ordered()`. On a log scale, equal ratios get equal steps of color
        (1 to 10 is the same step as 10 to 100), and the legend still shows
        the column's own values.
""", limits="""
    limits : tuple of two numbers, optional
        For a number column, the two ends of the color scale, as
        `(low, high)`. `None` at either end leaves that end fitted to the
        data.
""")
def color(field: Optional[Column] = None, scale: Optional[str] = None, base: Optional[float] = None,
          limits: Limits = None, tick_count: Optional[int] = None,
          speed: Optional[float] = None, free: bool = False,
          legend: Optional[bool] = None,
          axis: Optional[bool] = None,
          decimals: Optional[int] = None,
          short: Optional[bool] = None,
          percent: Optional[bool] = None) -> Atom:
    """Map fill or stroke color to a column.

    A channel maps a column to something the reader can see, and gets a
    legend that decodes it. `color` gives each category of a categorical column its
    own color from the palette (`palette()` chooses it), and runs a number
    column along a ramp from low to high. On a `line` or a `path` it also
    splits the rows into one line for each category. To set one color for
    the whole layer instead, with no legend, write the setting
    `style(color="tomato")`.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A channel, added to a plot with `+`.

    Examples
    --------
    A category: one color for each species, and a legend.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "sepal_length": [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5,
    ...                             6.3, 5.8, 7.1, 6.3],
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = (data(flowers) + point + x(col.sepal_length) + y(col.petal_length)
    ...      + color(col.species))

    A number: colors that run from low to high.

    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
    ...         "hp": [110, 110, 93, 110, 175, 105, 245, 62]}
    >>> q = data(cars) + point + x(col.wt) + y(col.mpg) + color(col.hp)
    """
    return _channel("color", field, scale, base, limits, tick_count, speed, free, legend, axis,
                    decimals, short, percent)


# The British spelling of `color()`, refused with direction.
#
# gog writes American English throughout and accepts no second spelling,
# which is Law 2 applied to the vocabulary itself: two ways to write one word
# is a silent letter, and the reader pays for it. ggplot2 accepts both, so a
# reader arriving from there types `colour` and, unexported, would meet a
# bare `NameError` — a message that names no fix. Exported for the same
# reason JavaScript still exports `facet` (spec §13).
def colour(*args: Any, **kwargs: Any) -> Atom:
    """The British spelling of `color()`, refused with a direction.

    gog writes American English and accepts no second spelling of a word:
    two ways to write one word is one more thing every reader has to learn.
    ggplot2 accepts `colour` and `color` both, so someone who has used
    ggplot2 may type `colour`. The name exists so that the call is answered
    with what to write instead, rather than with Python's `NameError`. The
    settings `style(colour=)`, `style(border_colour=)` and `style(centre=)`
    are refused the same way.

    Parameters
    ----------
    *args, **kwargs
        Ignored. The call is always refused.

    Returns
    -------
    None
        It never returns. It raises `GogError`, and the message names
        `color()`.

    Examples
    --------
    gog spells it `color`, and the error says so.

    >>> try:
    ...     c = colour(col.species)
    ... except GogError as refusal:
    ...     message = str(refusal)
    """
    raise GogError(
        "gog: there is no `colour()` channel. gog spells it `color(col.<name>)`: "
        "American English is the grammar's only spelling, and unlike ggplot2 "
        "there is no British alternative."
    )


@_with_parameters(_MEASURE_PARAMETERS, field="""
    field : Column
        The number column to bind, written `col.name`.
""", limits="""
    limits : tuple of two numbers, optional
        The two ends of the size scale, as `(low, high)`. `None` at either
        end leaves that end fitted to the data.
""")
def size(field: Optional[Column] = None, scale: Optional[str] = None, base: Optional[float] = None,
         limits: Limits = None, tick_count: Optional[int] = None,
         speed: Optional[float] = None, free: bool = False,
         legend: Optional[bool] = None,
         axis: Optional[bool] = None,
         decimals: Optional[int] = None,
         short: Optional[bool] = None,
         percent: Optional[bool] = None) -> Atom:
    """Map size to a number column.

    On a `point` it is the radius, and on `text` the size of the letters. A
    larger value draws a larger mark, and a legend decodes the sizes. To set
    one size for the whole layer instead, write the setting `style(size=6)`.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A channel, added to a plot with `+`.

    Examples
    --------
    Cars with more horsepower are drawn larger.

    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
    ...         "hp": [110, 110, 93, 110, 175, 105, 245, 62]}
    >>> p = data(cars) + point + x(col.wt) + y(col.mpg) + size(col.hp)
    """
    return _channel("size", field, scale, base, limits, tick_count, speed, free, legend, axis,
                    decimals, short, percent)


@_with_parameters(_MEASURE_PARAMETERS, field="""
    field : Column
        The number column to bind, written `col.name`.
""", limits="""
    limits : tuple of two numbers, optional
        The two ends of the scale, as `(low, high)`. `None` at either end
        leaves that end fitted to the data.
""")
def opacity(field: Optional[Column] = None, scale: Optional[str] = None, base: Optional[float] = None,
            limits: Limits = None, tick_count: Optional[int] = None,
            speed: Optional[float] = None, free: bool = False,
            legend: Optional[bool] = None,
            axis: Optional[bool] = None,
            decimals: Optional[int] = None,
            short: Optional[bool] = None,
            percent: Optional[bool] = None) -> Atom:
    """Map opacity to a number column.

    A larger value draws a more solid mark, and a smaller one a fainter mark.
    To set one opacity for the whole layer instead, write the setting
    `style(opacity=0.5)`.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A channel, added to a plot with `+`.

    Examples
    --------
    Cars with more horsepower are drawn more solid.

    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
    ...         "hp": [110, 110, 93, 110, 175, 105, 245, 62]}
    >>> p = data(cars) + point + x(col.wt) + y(col.mpg) + opacity(col.hp)
    """
    return _channel("opacity", field, scale, base, limits, tick_count, speed, free, legend, axis,
                    decimals, short, percent)


@_with_parameters(_CATEGORY_PARAMETERS, field="""
    field : Column
        The column to group by, written `col.name`.
""", legend="""
    legend : bool, optional
        Not taken: `group` splits the rows without encoding anything, so it
        draws no legend. gog refuses it when the plot is drawn, and names the
        five channels that draw one.
""")
def group(field: Optional[Column] = None, scale: Optional[str] = None, base: Optional[float] = None,
          limits: Limits = None, tick_count: Optional[int] = None,
          speed: Optional[float] = None, free: bool = False,
          legend: Optional[bool] = None,
          axis: Optional[bool] = None,
          decimals: Optional[int] = None,
          short: Optional[bool] = None,
          percent: Optional[bool] = None) -> Atom:
    """Split a layer into groups without coloring them.

    `group()` splits the rows by a column. A `line` or `path` then draws one
    stroke for each group, and `stack` or `dodge` has groups to place.
    Unlike `color()`, it draws every group the same way and adds no legend.
    Use `color()` when the groups should also look different; it splits the
    rows as well.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A channel, added to a plot with `+`.

    Examples
    --------
    One line for each tree, all drawn in one color.

    >>> trees = {"tree": ["1"] * 4 + ["2"] * 4 + ["3"] * 4,
    ...          "age": [118, 484, 664, 1004] * 3,
    ...          "circumference": [30, 58, 87, 115, 33, 69, 111, 156,
    ...                            30, 51, 75, 108]}
    >>> p = (data(trees) + line + x(col.age) + y(col.circumference)
    ...      + group(col.tree))
    """
    return _channel("group", field, scale, base, limits, tick_count, speed, free, legend, axis,
                    decimals, short, percent)


@_with_parameters(_CATEGORY_PARAMETERS, field="""
    field : Column
        The categorical column to bind to the glyph shape, written `col.name`.
""")
def shape(field: Optional[Column] = None, scale: Optional[str] = None, base: Optional[float] = None,
          limits: Limits = None, tick_count: Optional[int] = None,
          speed: Optional[float] = None, free: bool = False,
          legend: Optional[bool] = None,
          axis: Optional[bool] = None,
          decimals: Optional[int] = None,
          short: Optional[bool] = None,
          percent: Optional[bool] = None) -> Atom:
    """Map a categorical column to the shape of each point.

    Each category of the column gets its own symbol on `point`: a circle, a
    square, a triangle, a diamond, a cross, a star or a wye (a Y shape). A
    `scale` is refused, because there is no distance between a circle and a
    square to measure. To set
    one glyph for the whole layer instead, write the setting
    `style(shape="square")`.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A channel, added to a plot with `+`.

    Examples
    --------
    A different shape for each species.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "sepal_length": [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5,
    ...                             6.3, 5.8, 7.1, 6.3],
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = (data(flowers) + point + x(col.sepal_length) + y(col.petal_length)
    ...      + shape(col.species))
    """
    return _channel("shape", field, scale, base, limits, tick_count, speed, free, legend, axis,
                    decimals, short, percent)


@_with_parameters(_CATEGORY_PARAMETERS, field="""
    field : Column
        The categorical column to bind to the texture, written `col.name`.
""")
def pattern(field: Optional[Column] = None, scale: Optional[str] = None, base: Optional[float] = None,
            limits: Limits = None, tick_count: Optional[int] = None,
            speed: Optional[float] = None, free: bool = False,
            legend: Optional[bool] = None,
            axis: Optional[bool] = None,
            decimals: Optional[int] = None,
            short: Optional[bool] = None,
            percent: Optional[bool] = None) -> Atom:
    """Map paint texture to a categorical column: like `shape`, for fills and strokes.

    `pattern` tells categories apart by texture rather than by hue. On a filled
    region (`bar`, `box`, `area`, `ribbon`, `zone`) each category draws as a
    hatch, and on a stroke (`line`, `step`, `interval`, `path`, `rule`, `edge`)
    as a dash. It separates categories without color, so they stay distinct in
    a gray printout or for a color-blind reader. Using `pattern` and `color` on
    the same column shows each category two ways, which is the usual advice for
    accessible plots. A `scale` is refused: like `shape`, it answers "which
    one?", not "how much?".

    The setting `style(pattern=)` is different: it fixes one texture for the
    whole layer, where `pattern()` maps a column to several.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A channel, added to a plot with `+`.

    Examples
    --------
    A different hatch for each species, so the bars differ without color.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = (data(flowers) + bar * mean + x(col.species) + y(col.petal_length)
    ...      + pattern(col.species))
    """
    return _channel("pattern", field, scale, base, limits, tick_count, speed, free, legend, axis,
                    decimals, short, percent)


@_with_parameters(_CATEGORY_PARAMETERS, field="""
    field : Column
        The column whose values are drawn as the text, written `col.name`.
""", scale="""
    scale, base, limits : optional
        Not taken: a label prints each value as text rather than measuring
        it, so there is no scale or range along it. gog refuses each of them
        when the plot is drawn.
""", legend="""
    legend : bool, optional
        Not taken: a label is read where it is written, so it draws no
        legend. gog refuses it when the plot is drawn, and names the five
        channels that draw one.
""", decimals="""
    decimals : int, optional
        How many decimal places each number is printed with, a whole number
        from 0 to 10: `label(col.life, decimals=1)` prints 70.72848 as 70.7.
        The places are fixed, so 77 prints as 77.0 and a column of labels
        reads as one column of figures. A column of text has nothing to round.
""", short="""
    short : bool, optional
        True writes a large number short, with K for thousands, M for
        millions, B for billions and T for trillions:
        `label(col.population, short=True)` prints 3811953827 as 3.81B. It
        keeps three significant digits unless `decimals` gives the places
        after the point, so `decimals=1` prints 3.8B. A column of text has no
        number to write short.
""", percent="""
    percent : bool, optional
        True prints each number as a percent:
        `label(col.proportion, percent=True)` prints 0.25 as 25%. With
        `decimals` it gives the places after the point. Not together with
        `short`.
""")
def label(field: Optional[Column] = None, scale: Optional[str] = None, base: Optional[float] = None,
          limits: Limits = None, tick_count: Optional[int] = None,
          speed: Optional[float] = None, free: bool = False,
          legend: Optional[bool] = None,
          axis: Optional[bool] = None,
          decimals: Optional[int] = None,
          short: Optional[bool] = None,
          percent: Optional[bool] = None) -> Atom:
    """Draw a column's values as text: the `text` mark's content.

    `label` supplies the string a `text` mark draws, the way `x` and `y`
    supply its position: `text + x(col.a) + y(col.b) + label(col.name)`. A
    column of text is drawn as it is, and a column of numbers is formatted. It is
    `text`'s required channel, and no other mark takes it, so a scatter plot
    with labels is two layers: `point + text`. A `scale` is refused: a label is
    content, like `shape`, not a quantity to run a scale along.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A channel, added to a plot with `+`.

    Examples
    --------
    Each car's name, written at its weight and mileage.

    >>> cars = {"car": ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
    ...                 "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
    ...         "wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = data(cars) + text + x(col.wt) + y(col.mpg) + label(col.car)
    """
    return _channel("label", field, scale, base, limits, tick_count, speed, free, legend, axis,
                    decimals, short, percent)


def _check_speed(speed: Optional[float]) -> Optional[float]:
    if speed is None:
        return None
    if isinstance(speed, bool) or not isinstance(speed, (int, float)):
        raise GogError(
            "gog: `speed=` needs a single number, e.g. `play(col.year, speed=2)`. "
            "It is how many times faster than normal the frames run."
        )
    if speed <= 0:
        raise GogError(
            f"gog: `speed={speed}` — a speed is a multiple of the normal pace, so it "
            f"has to be above zero. `speed=2` is twice as fast, `speed=0.5` half."
        )
    return float(speed)


@_with_parameters(_CATEGORY_PARAMETERS, field="""
    field : Column
        The column whose distinct values become the frames, written
        `col.name`.
""", speed="""
    speed : float, optional
        How many times faster than the default pace the frames run, one
        positive number: 2 is twice as fast, 0.5 half.
""", scale="""
    scale, base, limits : optional
        Not taken: each value is a frame of its own rather than a quantity
        to measure, so there is no scale or range along it. gog refuses each
        of them when the plot is drawn.
""", legend="""
    legend : bool, optional
        Not taken: each frame is named in the strip above the panel as it is
        shown, so `play` draws no legend. gog refuses it when the plot is
        drawn, and names the five channels that draw one.
""", decimals="""
    decimals, short, percent : optional
        Not taken yet: the strip above the panel writes each frame's name in
        its own way. gog refuses each of them when the plot is drawn.
""")
def play(field: Optional[Column] = None, speed: Optional[float] = None, scale: Optional[str] = None,
         base: Optional[float] = None, limits: Limits = None,
         tick_count: Optional[int] = None, free: bool = False,
         legend: Optional[bool] = None,
         axis: Optional[bool] = None,
         decimals: Optional[int] = None,
         short: Optional[bool] = None,
         percent: Optional[bool] = None) -> Atom:
    """Cut the plot into frames and play them: the time dimension.

    `play` works like `facet`, but in time. Both split the rows by the values
    of a column. `| facet(col.continent)` shows the pieces side by side;
    `play(col.year)` shows them one after another, as frames.

    Every scale, the color map and every legend are fitted across the whole
    sequence, never one frame at a time. A scale fitted again for each frame
    would move the axis under the data, and make a still point look like a
    moving one. A layer that does not bind `play` is drawn in every frame, so
    a reference line stays in place behind the marks that move.

    Unlike `facet`, `play` accepts a column of numbers. A hundred panels on
    one page are too small to read, but a hundred frames only make the
    animation longer. The frames run in the order of the column: numbers
    from low to high, and text in a declared order. A column of text with no
    declared order plays in the order of its rows, and gog says so; declare
    the order with
    `ordered()`. A program that turns the SVG into a still picture shows the
    first frame. `save_gif()` writes every frame, and `save_png()` refuses a
    plot that plays, since a PNG holds one picture.

    Parameters
    ----------
    @parameters

    Returns
    -------
    Atom
        A channel, added to a plot with `+`.

    Examples
    --------
    One frame for each tree. On a web page, the plot shows them in turn. The
    trees' names are text, so their order is declared.

    >>> trees = {"tree": ordered(["1"] * 4 + ["2"] * 4 + ["3"] * 4, ["1", "2", "3"]),
    ...          "age": [118, 484, 664, 1004] * 3,
    ...          "circumference": [30, 58, 87, 115, 33, 69, 111, 156,
    ...                            30, 51, 75, 108]}
    >>> p = (data(trees) + point + x(col.age) + y(col.circumference)
    ...      + play(col.tree))
    """
    return _channel("play", field, scale, base, limits, tick_count, speed, free, legend, axis,
                    decimals, short, percent)


def _check_brush_at(at):
    """What `at=` was given, and which of the two readings it is.

    One argument rather than two, because the *value* answers the question the
    way a column answers it everywhere else in this grammar: numbers are a
    range, names are a set of slots.
    """
    if at is None:
        return {}
    if isinstance(at, str):
        at = [at]
    seq = list(at)
    if seq and all(isinstance(v, str) for v in seq):
        return {"levels": seq}
    if len(seq) != 2 or not all(isinstance(v, (int, float)) and v == v for v in seq):
        raise GogError(
            "gog: `at=` is where the selection opens: two numbers on a column that "
            "measures, e.g. `brush(col.gdp, at=(1200, 45000))`, or the names to "
            "select on a column of categories."
        )
    return {"at": [float(seq[0]), float(seq[1])]}


# `brush` — bare for a region over the plot's positions, called for a
# bound on one named column. The same both-shapes pattern `bin` uses, and
# for the same reason: `+ brush` must add the atom rather than the function.
class _Brush(CallableAtom):
    __slots__ = ()

    def __call__(self, field: Optional[Column] = None, at=None) -> Atom:
        name = "" if field is None else column_name(field, "brush")
        return Atom("brush", field=name, **_check_brush_at(at))


brush = with_help(_Brush("brush", field=""), """
    brush(field=None, at=None)

    Let the reader select rows, and dim the rest.

    `brush(col.wt)` lets the reader select rows by one column's values: a
    range of numbers, or some of its categories. Rows inside the selection
    keep the plot's colors; rows outside it are dimmed, so the reader sees
    the selected rows beside all the others. Where the page can run gog's
    engine, dragging moves the selection; in a still picture, such as an SVG
    file or a PNG, it stays where the sentence put it.
    Bare `brush` means the positions this plot binds: the reader drags a
    region over them, and nothing is selected until they do.

    A brush highlights. It never removes rows. Removing rows before the
    statistics run is what `limits` does, and it counts what it dropped. The
    two look alike and do different things: with `limits`, a histogram bins
    only the rows that are left; with a brush, every row stays and the
    selected ones are drawn in full color.

    One column for each `brush`. Write two for a rectangle:
    `brush(col.gdp, at=(1200, 45000)) + brush(col.life, at=(55, 78))`.

    A mark can be brushed when one row is one shape: `point`, `bar`, `text`,
    `rule`, `zone`, `interval * bounds` and `ribbon * flow`. A `line` draws
    one shape through many rows, so there is no single row to select, and gog
    says so rather than guessing. A summarized layer, such as `bar * bin`, has
    no single row to select either. Alone it is refused. Beside a layer that
    can be brushed, it is drawn whole, because one bar cannot show twelve of
    its forty rows as selected.

    Parameters
    ----------
    field : Column, optional
        The column the selection is made on, written `col.name`. Left out,
        as in bare `brush`, the selection is a region over the plot's
        positions.
    at : tuple of two numbers, or list of str, optional
        The selection drawn before the reader drags: two numbers on a column
        that measures, such as `at=(20, 30)`, or the names to select on a
        categorical column, such as `at=["setosa", "virginica"]`. Left out,
        nothing is selected and the plot draws exactly as it would with no
        `brush` at all.

    Returns
    -------
    Atom
        A selection, added to a plot with `+`.

    Examples
    --------
    On a web page, a drag across the plot selects rows by weight.

    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = data(cars) + point + x(col.wt) + y(col.mpg) + brush(col.wt)

    A selection written in the sentence: the cars from 20 to 30 miles per
    gallon.

    >>> q = data(cars) + point + x(col.wt) + y(col.mpg) + brush(col.mpg, at=(20, 30))
""")


# ---------------------------------------------------------------------------
# Settings — they fix a value, map nothing, and earn no legend (spec §7)
# ---------------------------------------------------------------------------

_STYLE_STRINGS = ("color", "shape", "border_color")
_STYLE_NUMBERS = ("opacity", "size", "border_size", "angle", "gap")
_STYLE_FLAGS = ("caps", "center")
_STYLE_VALUES: Dict[str, Sequence[str]] = {
    "nudge": ("up", "down", "left", "right"),
    "pattern": ("solid", "dashed", "dotted", "dotdash", "longdash",
                "hatch", "crosshatch", "stripes", "grid", "dots"),
    "arrow": ("end", "start", "both"),
    "reach": ("panel", "edge"),
}
_STYLE_PROPS = (
    _STYLE_STRINGS + _STYLE_NUMBERS + _STYLE_FLAGS + tuple(_STYLE_VALUES)
)
# An example the setting itself would take: `style(shape="tomato")` was the
# example a refused shape was given. The same values in all four bindings.
_STYLE_EXAMPLES = {"color": '"tomato"', "shape": '"square"', "border_color": '"black"',
                   "opacity": "0.3", "size": "6", "border_size": "1.5", "angle": "90",
                   "gap": "0.2"}
# What a setting does, said after the values it takes, in the same words as the
# other three bindings.
_STYLE_MEANINGS = {
    "caps": " — `caps=False` draws a bare linerange, `caps=True` (the default) an "
            "error bar.",
    "center": " — `center=False` hides a confidence interval's center dot, "
              "`center=True` (the default) draws it.",
    "nudge": " — which way a text label sits from its point.",
    "arrow": " — which end of a `path` carries the head.",
}

# The British spelling of a setting, and what gog spells it instead. One entry
# per gog word that has a British form; there are three, and `colour()` the
# channel is the fourth word in the grammar with one.
_BRITISH_SETTINGS = {
    "colour": "color",
    "border_colour": "border_color",
    "centre": "center",
}


def style(**props: Any) -> Atom:
    """Set fixed visual properties on the mark written before it.

    A channel maps a column: `color(col.species)` gives each species its own
    color and draws a legend. `style()` sets one value for the whole layer
    instead. It maps no column and draws no legend.

    `style()` belongs to the mark written before it, so it comes after a
    mark: `point + style(color="tomato")`. Two `style()` calls on one mark
    are combined; where both set the same property, the later one wins. A
    setting applies to every mark whose shape can carry it, and the
    engine refuses one a mark cannot carry. A name that is not a setting is
    refused by name rather than ignored, and so is a column where a value
    belongs, with the channel that would map it.

    Parameters
    ----------
    **props
        The settings, each written by name. These are all of them:
    color : str
        A color name, such as "steelblue", or a hex code, such as "#4e79a7".
    opacity : float
        A number from 0 to 1, used as it is: unlike the `opacity` channel,
        there is no data range to rescale from.
    size : float
        Pixels: a point's radius (4.5 when unset), or a line's stroke width
        (2 when unset).
    shape : str
        A point's glyph: "circle", "square", "triangle", "diamond", "cross",
        "star" or "wye" (a Y shape).
    border_color : str
        The outline color of a closed shape or a filled region. On `text`,
        setting `border_color` or `border_size` draws a halo around each
        label. Its color is the panel's background unless `border_color`
        names one, and its width is 3 unless `border_size` gives one.
    border_size : float
        The outline width of a closed shape or a filled region. On `text`,
        the width of the halo (see `border_color`).
    caps : bool
        True (the default) draws the short crossbars at the ends of an
        `interval`, which makes an error bar; False leaves it bare.
    center : bool
        True (the default) draws the dot at the center of an `interval` whose
        transform gives one, such as `confidence`; False leaves it out.
    nudge : str
        Which side of its point a `text` label is moved to: "up", "down",
        "left" or "right". The distance comes from the size of the letters.
    pattern : str
        One texture for the whole layer. On a stroke (`line`, `step`,
        `interval`, `path`, `rule`, `edge`) it is a dash: "solid", "dashed",
        "dotted", "dotdash" or "longdash". On a filled region (`bar`, `box`,
        `area`, `ribbon`, `zone`) it is a hatch: "hatch", "crosshatch",
        "stripes", "grid" or "dots". The `pattern()` channel maps a column to
        several instead.
    arrow : str
        Which end of a `path` or an `edge` carries an arrowhead: "end",
        "start" or "both".
    reach : str
        How far a `rule` reaches across the axis it does not name: "panel"
        (the default, a reference line all the way across) or "edge" (a short
        tick at the start of that axis, which makes a rug plot: one short tick
        at each value).
    angle : float
        Degrees to turn a `text` label, counterclockwise from horizontal: 90
        reads upward. Each label turns about the point it names.
    gap : float
        The share of the space each category gives a `bar` or a `box` that is
        left empty beside it, from 0 (neighbors touch) to just under 1. On a
        dodged mark it is a share of the mark's part of that space. Unset, a
        category bar leaves 0.2, a binned bar 0, and a box keeps its usual
        width.

    Returns
    -------
    Atom
        A setting, added to a plot with `+` after the mark it sets.

    Examples
    --------
    One color and opacity for every point: settings, not mappings.

    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = (data(cars) + point + x(col.wt) + y(col.mpg)
    ...      + style(color="tomato", opacity=0.5))

    A thin gray line for each tree.

    >>> trees = {"tree": ["1"] * 4 + ["2"] * 4 + ["3"] * 4,
    ...          "age": [118, 484, 664, 1004] * 3,
    ...          "circumference": [30, 58, 87, 115, 33, 69, 111, 156,
    ...                            30, 51, 75, 108]}
    >>> q = (data(trees) + line + x(col.age) + y(col.circumference)
    ...      + group(col.tree) + style(color="gray", size=1))
    """
    if not props:
        raise GogError(
            "gog: `style()` sets nothing. Name at least one property, e.g. "
            "`style(color=\"tomato\")`."
        )

    for name, value in props.items():
        if name not in _STYLE_PROPS:
            # The British spelling and the ordinary typo part on the *message*
            # and not on the check: one names the word to write, the other
            # lists what exists.
            if name in _BRITISH_SETTINGS:
                raise GogError(
                    f"gog: `style({name}=)` is not a setting. gog spells it "
                    f"`{_BRITISH_SETTINGS[name]}`: American English is the "
                    "grammar's only spelling, and unlike ggplot2 there is no "
                    "British alternative."
                )
            known = ", ".join(sorted(_STYLE_PROPS))
            raise GogError(
                f"gog: `style({name}=)` is not a setting. gog sets: {known}."
            )
        # A column where a value belongs — the mirror of a string where a column
        # belongs, and the same §7 distinction seen from the other side.
        if isinstance(value, Column):
            raise GogError(
                f"gog: `style({name}=)` fixes one value for the whole layer, and "
                f"`{value!r}` is a column. To *map* it — one value per category, with a "
                f"legend to decode it — that is a channel: `{name}({value!r})`."
                if name in ("color", "size", "opacity", "shape", "pattern")
                else f"gog: `style({name}=)` fixes one value, and `{value!r}` is a column."
            )
        if name in _STYLE_STRINGS and not isinstance(value, str):
            raise GogError(
                f"gog: `style({name}=)` needs a single string, e.g. "
                f"`style({name}={_STYLE_EXAMPLES[name]})`."
            )
        if name in _STYLE_NUMBERS and (
            isinstance(value, bool) or not isinstance(value, (int, float))
        ):
            raise GogError(
                f"gog: `style({name}=)` needs a single number, e.g. "
                f"`style({name}={_STYLE_EXAMPLES[name]})`."
            )
        if name in _STYLE_FLAGS and not isinstance(value, bool):
            raise GogError(
                f"gog: `style({name}=)` needs true or false"
                f"{_STYLE_MEANINGS.get(name, '.')}"
            )
        if name in _STYLE_VALUES and value not in _STYLE_VALUES[name]:
            # `pattern` is the one setting whose values split by geometry: five
            # are a stroke's dash and five a fill's texture, and the engine
            # refuses each on the other. A flat list of ten hides that, so a
            # reader learns the split only by being refused twice.
            if name == "pattern":
                raise GogError(
                    'gog: `style(pattern=)` needs a stroke\'s dash '
                    '("solid", "dashed", "dotted", "dotdash", "longdash") '
                    'or a fill\'s texture '
                    '("hatch", "crosshatch", "stripes", "grid", "dots").'
                )
            # `reach` says what each of its two values draws, which is the
            # whole of what a reader needs to choose between them.
            if name == "reach":
                raise GogError(
                    'gog: `style(reach=)` needs "panel" (the default — a `rule` all '
                    'the way across, a reference line) or "edge" (a short tick at the '
                    "start of that axis, a rug)."
                )
            allowed = ", ".join(f'"{v}"' for v in _STYLE_VALUES[name])
            raise GogError(
                f"gog: `style({name}=)` needs one of {allowed}"
                f"{_STYLE_MEANINGS.get(name, '.')}"
            )

    clean = {
        name: float(value) if name in _STYLE_NUMBERS else value
        for name, value in props.items()
    }
    return Atom("style", props=clean)


# ---------------------------------------------------------------------------
# Plot-level atoms
# ---------------------------------------------------------------------------


# The column is required. `order(desc=True)` alone reads as a way to run a
# category axis backward, and it never was; naming the category column itself
# is, so the refusal says that rather than Python's missing-argument error.
def order(field: Optional[Column] = None, desc: bool = False) -> Atom:
    """Order a category axis by a column.

    `order()` sorts the categories on a categorical axis by the values of a
    column: `order(col.mpg)` puts the categories in order of their `mpg`,
    smallest first.

    Parameters
    ----------
    field : Column
        The column to order by, written `col.name`. A number column, such as
        the one on `y`, orders the categories by value, and this overrides a
        declared order. The categorical column itself asks for that column's own
        order: alphabetical for plain text, and the declared order for a
        column made with `ordered()` or a pandas `Categorical`. It is
        required: `order(desc=True)` alone is refused, with this direction.
    desc : bool, default False
        True puts the largest value first (or Z before A); False keeps the
        smallest first.

    Returns
    -------
    Atom
        A plot setting, added to a plot with `+`.

    Examples
    --------
    Bars in order of their height, lowest first.

    >>> cars = {"car": ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
    ...                 "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = data(cars) + bar + x(col.car) + y(col.mpg) + order(col.mpg)

    Highest first.

    >>> q = data(cars) + bar + x(col.car) + y(col.mpg) + order(col.mpg, desc=True)
    """
    # `bool(desc)` read any non-empty string as true and any other value by
    # Python's truthiness, so `desc="no"` sorted largest first without a word.
    if not isinstance(desc, bool):
        raise GogError(
            "gog: `order(desc=)` is true or false — true reverses the order, so the "
            "largest value comes first, and false keeps it, smallest first."
        )
    if field is None:
        written = "order(desc=True)" if desc else "order()"
        tail = ", desc=True" if desc else ""
        raise GogError(
            f"gog: `{written}` names no column to order by. Name the category column "
            f"itself to sort the axis by its own categories, `order(col.<category>{tail})`, "
            f"or a column of values to sort them by, `order(col.<value>{tail})`."
        )
    return Atom("order", field=column_name(field, "order"), descending=desc)


def facet(field: Optional[Column] = None, wrap: Optional[int] = None) -> Atom:
    """Name the column that splits the plot into panels: small multiples.

    `facet()` is joined to a plot with `|` or `/`, not with `+`:

        plot | facet(col.a)                  one panel per category, side by side
        plot / facet(col.a)                  one panel per category, stacked
        plot | facet(col.a) / facet(col.b)   crossed into a grid, `a` across
                                             and `b` down

    The panels share one scale, which is what makes them comparable, and any
    transform in the plot runs within each panel's rows. The facet column
    must be a categorical column: a number is a position along an axis, not a
    name for a panel, and is refused. `free=True` on a position lets each
    panel fit that axis to its own rows.

    `wrap` breaks a long line of panels into several lines: twelve countries
    side by side make twelve narrow panels, and `wrap=4` makes them a 4 by 3
    grid. The number is how many panels go in each line, and the operator
    gives the direction, not `wrap`:

        plot | facet(col.country, wrap=4)    the panels run across, 4 to a row
        plot / facet(col.country, wrap=4)    the panels run down, 4 to a column

    That is one number where ggplot2 has `nrow` and `ncol`, so it cannot
    contradict itself. Wrapping a crossed facet is refused: a crossing
    already fills a rectangle.

    Parameters
    ----------
    field : Column
        The categorical column to facet by, written `col.name`.
    wrap : int, optional
        How many panels go in each line. Left out, the panels stay in one
        line.

    Returns
    -------
    Atom
        A facet, joined to a plot with `|` or `/`.

    Examples
    --------
    One panel for each species, side by side.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "sepal_length": [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5,
    ...                             6.3, 5.8, 7.1, 6.3],
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = (data(flowers) + point + x(col.sepal_length) + y(col.petal_length)
    ...      | facet(col.species))

    The same panels, one above the other.

    >>> q = (data(flowers) + point + x(col.sepal_length) + y(col.petal_length)
    ...      / facet(col.species))

    Two panels in each row.

    >>> w = (data(flowers) + point + x(col.sepal_length) + y(col.petal_length)
    ...      | facet(col.species, wrap=2))
    """
    # A second column is a crossing, not a count: one `facet()` for each.
    if isinstance(wrap, Column):
        raise GogError(
            f"gog: `facet()` splits the plot by one column, and `{wrap!r}` is a second "
            f"one. To cross two columns, give each its own `facet()`: "
            f"`| facet({field!r}) / facet({wrap!r})`."
        )
    # `wrap=4.0` means 4, as it does in R and JavaScript.
    if isinstance(wrap, float) and wrap.is_integer():
        wrap = int(wrap)
    if wrap is not None and (isinstance(wrap, bool) or not isinstance(wrap, int)):
        raise GogError(
            "gog: `facet(wrap=)` takes the number of panels to draw before the "
            "line of them turns — one whole number, e.g. `wrap=4`."
        )
    return Atom("facet", field=column_name(field, "facet"), wrap=wrap)


def palette(pal: Union[str, Sequence[str], Mapping[str, str]]) -> Atom:
    """Set the colors the `color` channel uses.

    For a categorical column, the palette's colors go to the categories in
    order: the declared order of a column made with `ordered()` or a pandas
    `Categorical`, or, for plain text, the order in which the values first
    appear. If there are
    more categories than colors, more are made around the color wheel so
    that no two categories share a color, and gog says so. For a number
    column, a named ramp runs from low to high instead.

    Parameters
    ----------
    pal : str, list of str, or dict
        One of three things. A name: for categories, "gog" (the default),
        "okabe" (the Okabe-Ito colors, chosen to stay distinct for readers
        with color blindness) or "soft". For a number column, "blue" (the
        default), "viridis", "magma", "inferno", "plasma", "cividis" or
        "gray"; or "blue_red" and "brown_teal", which run out from a neutral
        middle (with `color(col.change, limits=(-40, 40))` that middle is 0).
        A list of colors, such as `["#1b9e77", "#d95f02"]`. Or a dict that
        binds a color to each category by name, such as
        `{"Asia": "tomato", "Europe": "steelblue"}`: each category keeps its
        color whatever the order of the rows. Every name must be a
        category of the column on `color`, and every category drawn must have
        a name; the engine refuses either mismatch and lists what is missing.

    Returns
    -------
    Atom
        A plot setting, added to a plot with `+`.

    Examples
    --------
    The Okabe-Ito colors, chosen to stay distinct for readers with color
    blindness.

    >>> flowers = {"species": ["setosa"] * 4 + ["versicolor"] * 4 + ["virginica"] * 4,
    ...            "sepal_length": [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5,
    ...                             6.3, 5.8, 7.1, 6.3],
    ...            "petal_length": [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4,
    ...                             6, 5.1, 5.9, 5.6]}
    >>> p = (data(flowers) + point + x(col.sepal_length) + y(col.petal_length)
    ...      + color(col.species) + palette("okabe"))

    A color written beside each species.

    >>> q = (data(flowers) + point + x(col.sepal_length) + y(col.petal_length)
    ...      + color(col.species)
    ...      + palette({"setosa": "tomato", "versicolor": "steelblue",
    ...                 "virginica": "seagreen"}))
    """
    if isinstance(pal, str):
        value: Dict[str, Any] = {"named": pal}
    elif isinstance(pal, Mapping):
        # Which names are levels is the engine's question, since only the table
        # can answer it; this checks the shape of what was written. A dict keeps
        # its insertion order, so a refusal quotes the names back as typed.
        levels: Dict[str, str] = {}
        for level, color in pal.items():
            if not isinstance(level, str) or not isinstance(color, str):
                raise GogError(
                    "gog: `palette()` with names binds a color to each level, both "
                    'written as text, e.g. `palette({"Asia": "tomato", '
                    '"Europe": "steelblue"})`.'
                )
            levels[level] = color
        value = {"levels": levels}
    elif isinstance(pal, (list, tuple)) and all(isinstance(c, str) for c in pal):
        value = {"custom": list(pal)}
    else:
        # The same sentence in all four bindings, and only the examples spelled
        # in each. `str(c)` used to turn a number into a color's name, which the
        # engine then refused as a color it had never heard of.
        raise GogError(
            "gog: `palette()` takes a palette name, a list of colors, or a color for "
            'each level by name, e.g. `palette("okabe")`, '
            '`palette(["#1b9e77", "#d95f02"])` or `palette({"Asia": "tomato"})`.'
        )
    return Atom("palette", value=value)


def _text_value(value: Any, atom: str) -> str:
    if not isinstance(value, str):
        raise GogError(f"gog: `{atom}()` needs a string, e.g. `{atom}('Life expectancy')`.")
    return value


THEME_PRESETS = ("gog", "minimal", "bw")
_GRID_VALUES = ("both", "x", "y", "none")
_FRAME_VALUES = ("full", "axes", "none")
_LEGEND_SIDES = ("right", "left", "top", "bottom")


# Everything here maps no column, so each is a *setting*; but none of it
# belongs to a mark either, which is why it is not `style()`. A layer has no
# gridlines and a plot has no fill, so the two property sets are disjoint, and
# telling them apart by where they were written would make a sub-expression
# mean different things in different places (Law 6). Spec §7 is the ruling.
#
# A named preset comes first and anything named adjusts it, because a preset
# you cannot adjust sends you straight back to asking for knobs.
#
# `width` and `height` mean one thing in both places they are written (Law 6):
# on its own a plot's size is the image, and composed it is the plot's cell.
def theme(
    preset: Optional[str] = None,
    *,
    grid: Optional[str] = None,
    ratio: Optional[float] = None,
    tick_angle: Optional[float] = None,
    font_size: Optional[float] = None,
    background: Optional[str] = None,
    strip: Optional[str] = None,
    strip_text: Optional[str] = None,
    frame: Optional[str] = None,
    axis_label: Optional[str] = None,
    legend: Optional[str] = None,
    width: Optional[float] = None,
    height: Optional[float] = None,
    gap: Optional[float] = None,
) -> Atom:
    """Set the parts of the plot that are not data: gridlines, background, frame, text size and legends.

    Nothing here maps a column, so each is a setting. None of it belongs to a
    mark either, which is why it is not `style()`: `style()` sets how a mark
    is drawn and `theme()` sets everything around the marks, so the two never
    set the same property. Two `theme()` calls on one plot are combined;
    where both set the same property, the later one wins.

    A named preset comes first and anything named after it adjusts it, so a
    preset can always be changed in part: `theme("bw", grid="none")`.

    On an arrangement of plots, made with `|` or `/`, `theme()` takes only
    `width`, `height` and `gap`, which say how big the figure is and how far
    apart its plots are: `(p | q) + theme(gap=8)`. Everything else belongs
    to one of the plots, and is refused on the arrangement.

    Parameters
    ----------
    preset : str, optional
        "gog" (the default look), "minimal" (no gridlines) or "bw" (a white
        panel inside a full rectangle, with white facet strips: the journal
        look. Only the parts around the data change; the data keep their
        colors). The only one given by position: `theme("minimal")`. Every
        other parameter is a keyword.
    grid : str, optional
        Which gridlines are drawn: "both", "x", "y" or "none", named by the
        axis whose ticks they mark, so the setting means the same in
        `polar()`, where the y axis's gridlines are rings.
    ratio : float, optional
        The panel's width divided by its height, one positive number. 1 is a
        square.
    tick_angle : float, optional
        Degrees to turn the x tick labels through, from -90 to 90. Category
        names that do not fit side by side are turned to 90 without it, and
        some are left out when even that is not enough, so give an angle only
        to choose one.
    font_size : float, optional
        How many pixels a tick label is, at least 4, and through it the size
        of every other piece of text the plot draws. The axis names and the
        title grow from it by a fixed ratio, 1.2 for each step, rounded: 11
        (the default) gives 11, 13 and 16, and 16 gives 16, 19 and 23. It is
        a measurement, not a multiplier, so
        `font_size=1.5` is refused. It names no typeface: the engine measures
        text with its own table of widths and has no typeface to choose.
    background : str, optional
        The panel's fill: any color the rest of the grammar accepts,
        including "transparent".
    strip : str, optional
        The fill of a facet's strip, the band above a panel that names the
        category it holds. The same colors as `background`. `theme("bw")`
        sets it white, because a gray band prints poorly.
    strip_text : str, optional
        The color of the strip's text. Left out, gog picks whichever of its
        two text colors is easier to read on the strip's fill, so
        `theme(strip="black")` already gives white text. Set it only to
        choose a color yourself, such as gold text on a navy strip.
    frame : str, optional
        How the panel is bounded: "full" (a rectangle, which `theme("bw")`
        sets and a journal usually asks for), "axes" (the default, bottom
        and left only) or "none".
    axis_label : str, optional
        Where each axis's name is written: "beside" (the default, centered
        along its own axis, which turns the y name through 90 degrees) or
        "end" (at the axis's far end and level, so nothing is read sideways).
    legend : str, optional
        Which side of the plot the legends are drawn on: "right" (the
        default), "left", "top" or "bottom". Above or below the plot the
        legend entries are written in a row, so the panel can use the full
        width. To
        leave one legend out, write `legend=False` on its channel instead,
        as in `color(col.continent, legend=False)`.
    width, height : float, optional
        How many pixels the plot asks for, at least 40 each; a plot is 800 by
        600 unless it asks. For a plot on its own, this is the size of the
        image. In an arrangement made with `|` or `/`, it is the size of the
        plot's cell, and plots that give no size share the space that is
        left. So a small histogram beside a larger plot can be made narrow by
        giving it a width. Not to be confused with `ratio`, which shapes the
        panel inside whatever room the plot was given and never resizes the
        image.
    gap : float, optional
        The space between an arrangement's plots, in pixels, 0 or more; 20
        when not given. It is the arrangement's own setting, like its size,
        so it is written on the arrangement: `(a | b) + theme(gap=8)`. A
        single plot refuses it; the engine spaces a plot's own panels,
        a facet's included.

    Returns
    -------
    Atom
        A plot setting, added to a plot or an arrangement with `+`.

    Examples
    --------
    A square panel, with no gridlines.

    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = data(cars) + point + x(col.wt) + y(col.mpg) + theme(ratio=1, grid="none")

    A smaller picture.

    >>> q = data(cars) + point + x(col.wt) + y(col.mpg) + theme(width=400, height=300)

    The journal look, with the legends below the plot.

    >>> r = data(cars) + point + x(col.wt) + y(col.mpg) + theme("bw", legend="bottom")
    """
    if (preset is None and grid is None and ratio is None and tick_angle is None
            and font_size is None and background is None and strip is None
            and strip_text is None and frame is None and axis_label is None
            and legend is None and width is None and height is None and gap is None):
        raise GogError(
            "gog: `theme()` sets nothing. Name a preset or a property, e.g. "
            "`theme(\"minimal\")` or `theme(grid=\"none\", ratio=1)`."
        )
    if preset is not None and not isinstance(preset, str):
        raise GogError(
            "gog: `theme()` takes a preset name first — `theme(\"minimal\")` — and "
            "everything else by name: `theme(grid=\"none\")`."
        )
    # Checked in the engine too (`check_theme`), which is what makes the rule the
    # grammar's rather than this binding's. Checking here as well is what puts
    # the error on the line that wrote it.
    if grid is not None and grid not in _GRID_VALUES:
        raise GogError(
            "gog: `theme(grid=)` is one of "
            + ", ".join(f'"{v}"' for v in _GRID_VALUES[:-1])
            + f' or "{_GRID_VALUES[-1]}".'
        )
    if ratio is not None and (
        isinstance(ratio, bool) or not isinstance(ratio, (int, float)) or ratio <= 0
    ):
        raise GogError(
            "gog: `theme(ratio=)` is the panel's width divided by its height, so it "
            "needs one positive number. `ratio=1` is a square."
        )
    if tick_angle is not None and (
        isinstance(tick_angle, bool)
        or not isinstance(tick_angle, (int, float))
        or abs(tick_angle) > 90
    ):
        raise GogError(
            "gog: `theme(tick_angle=)` turns the x tick labels between -90 and 90 "
            "degrees. Names that do not fit side by side are turned without it, so "
            "state an angle only to choose one, such as `tick_angle=45`."
        )
    if font_size is not None and (
        isinstance(font_size, bool)
        or not isinstance(font_size, (int, float))
        or font_size < 4
    ):
        raise GogError(
            "gog: `theme(font_size=)` is how many pixels a tick label is, not a "
            "multiplier, so it needs one number of at least 4. The default is 11, "
            "and the axis names and the title are derived from it."
        )
    if axis_label is not None and axis_label not in ("end", "beside"):
        raise GogError(
            "gog: `theme(axis_label=)` is \"end\" or \"beside\" \u2014 where each axis's "
            "name sits: at the axis's far end, or centered along it."
        )
    # `legend=False` is how a channel leaves its own key out, so it is the
    # likeliest thing to be written here too; the refusal points at the channel.
    if isinstance(legend, bool):
        raise GogError(
            "gog: `theme(legend=)` says which side of the plot the legends sit on: "
            "\"right\", \"left\", \"top\" or \"bottom\". To leave a legend out, write "
            "`legend=False` on the channel it decodes, as in "
            "`color(col.continent, legend=False)`."
        )
    if legend is not None and legend not in _LEGEND_SIDES:
        raise GogError(
            "gog: `theme(legend=)` is one of \"right\", \"left\", \"top\" or "
            "\"bottom\": the side of the plot its legends sit on. To leave a legend "
            "out, write `legend=False` on its channel."
        )
    if frame is not None and frame not in _FRAME_VALUES:
        raise GogError(
            "gog: `theme(frame=)` is one of \"full\" (a rectangle round the panel), "
            "\"axes\" (bottom and left only) or \"none\"."
        )
    if background is not None and not isinstance(background, str):
        raise GogError(
            "gog: `theme(background=)` needs a single color, e.g. "
            "`theme(background=\"white\")` or `\"transparent\"`."
        )
    if strip is not None and not isinstance(strip, str):
        raise GogError(
            "gog: `theme(strip=)` needs a single color for the band above each "
            "panel, e.g. `theme(strip=\"white\")`."
        )
    if strip_text is not None and not isinstance(strip_text, str):
        raise GogError(
            "gog: `theme(strip_text=)` needs a single color for the strip's label. "
            "Leave it out and gog picks the one that reads on the band."
        )
    # One loop for both, because they are one property asked twice — see the
    # engine's `check_theme`, which states the same rule for every binding.
    for name, value in (("width", width), ("height", height)):
        if value is not None and (
            isinstance(value, bool) or not isinstance(value, (int, float)) or value < 40
        ):
            raise GogError(
                f"gog: `theme({name}=)` is how many pixels the plot asks for, so it "
                f"needs one number of at least 40. On its own it sizes the image; "
                f"composed with `|` or `/` it sizes the plot's cell in the arrangement."
            )
    # How far apart an arrangement's plots are: the arrangement's own setting, as its
    # size is.
    if gap is not None and (
        isinstance(gap, bool) or not isinstance(gap, (int, float)) or not gap >= 0
        or gap == float("inf")
    ):
        raise GogError(
            "gog: `theme(gap=)` is how many pixels apart an arrangement's plots stand, so it "
            "needs one number of 0 or more, e.g. `theme(gap=8)`."
        )
    return Atom(
        "theme",
        preset=preset,
        grid=grid,
        ratio=None if ratio is None else float(ratio),
        tick_angle=None if tick_angle is None else float(tick_angle),
        font_size=None if font_size is None else float(font_size),
        background=background,
        strip=strip,
        strip_text=strip_text,
        frame=frame,
        axis_label=axis_label,
        legend=legend,
        width=None if width is None else float(width),
        height=None if height is None else float(height),
        gap=None if gap is None else float(gap),
    )


def title(text: str) -> Atom:
    """Set the plot's title.

    The title is drawn above the plot, in larger letters than any other text
    on it.

    Parameters
    ----------
    text : str
        The title.

    Returns
    -------
    Atom
        A plot setting, added to a plot with `+`.

    Examples
    --------
    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = (data(cars) + point + x(col.wt) + y(col.mpg)
    ...      + title("Heavier cars go fewer miles on a gallon"))
    """
    return Atom("title", value=_text_value(text, "title"))


def subtitle(text: str) -> Atom:
    """Set a subtitle, a second line under the title.

    It is smaller and lighter than the title. Use it for detail that would
    make the title too long, such as which years the numbers cover.

    Parameters
    ----------
    text : str
        The subtitle.

    Returns
    -------
    Atom
        A plot setting, added to a plot with `+`.

    Examples
    --------
    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = (data(cars) + point + x(col.wt) + y(col.mpg)
    ...      + title("Heavier cars go fewer miles on a gallon")
    ...      + subtitle("8 cars from 1973 and 1974"))
    """
    return Atom("subtitle", value=_text_value(text, "subtitle"))


def caption(text: str) -> Atom:
    """Set a caption, a line at the bottom of the plot.

    A small line at the bottom of the picture, lined up with the left side of
    the panel: the source of the numbers, as a printed figure carries one.

    Parameters
    ----------
    text : str
        The caption.

    Returns
    -------
    Atom
        A plot setting, added to a plot with `+`.

    Examples
    --------
    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = (data(cars) + point + x(col.wt) + y(col.mpg)
    ...      + title("Heavier cars go fewer miles on a gallon")
    ...      + caption("Source: Motor Trend, 1974"))
    """
    return Atom("caption", value=_text_value(text, "caption"))


def x_label(text: str) -> Atom:
    """Name the x axis.

    An axis is named after its column unless it is given a name of its own.
    `x_label()` gives the x axis one, such as the column's meaning and its
    unit.

    Parameters
    ----------
    text : str
        The axis name.

    Returns
    -------
    Atom
        A plot setting, added to a plot with `+`.

    Examples
    --------
    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = data(cars) + point + x(col.wt) + y(col.mpg) + x_label("Weight (1000 lb)")
    """
    return Atom("x_label", value=_text_value(text, "x_label"))


def y_label(text: str) -> Atom:
    """Name the y axis.

    An axis is named after its column unless it is given a name of its own.
    `y_label()` gives the y axis one, such as the column's meaning and its
    unit.

    Parameters
    ----------
    text : str
        The axis name.

    Returns
    -------
    Atom
        A plot setting, added to a plot with `+`.

    Examples
    --------
    >>> cars = {"wt": [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> p = data(cars) + point + x(col.wt) + y(col.mpg) + y_label("Miles per gallon")
    """
    return Atom("y_label", value=_text_value(text, "y_label"))


def z_label(text: str) -> Atom:
    """Name the z axis.

    Sets the name of the z axis, as `x_label()` and `y_label()` do for the
    other two. It is used only in a 3-D plot, where the axis names are
    written along the edges of the cube. Without `z()`, gog warns that the
    label is ignored.

    Parameters
    ----------
    text : str
        The axis name.

    Returns
    -------
    Atom
        A plot setting, added to a plot with `+`.

    Examples
    --------
    A small hill, sampled on a 4 by 4 grid, with its height axis named.

    >>> hill = {"x": [1, 2, 3, 4] * 4,
    ...         "y": [1] * 4 + [2] * 4 + [3] * 4 + [4] * 4,
    ...         "height": [0.223, 0.435, 0.435, 0.223, 0.435, 0.846, 0.846, 0.435,
    ...                    0.435, 0.846, 0.846, 0.435, 0.223, 0.435, 0.435, 0.223]}
    >>> p = (data(hill) + surface + x(col.x) + y(col.y) + z(col.height)
    ...      + z_label("Height"))
    """
    return Atom("z_label", value=_text_value(text, "z_label"))
