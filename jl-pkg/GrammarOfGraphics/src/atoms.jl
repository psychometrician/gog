# atoms.jl — the vocabulary: marks, transforms, channels, settings
#
# The mirror of `r-pkg/gog/R/atoms.R`, and the words are the same words: the
# grammar is the engine's, not the binding's, so anything that differs here is a
# bug in one of the four front ends. What each atom *means* is documented once,
# in the book and the spec; this file only says how Julia spells it.
#
# Julia spells it almost exactly as R does. It has real keyword arguments, so
# `bin(30)` and `bin(width = 5)` and `x(:gdp, scale = "log")` are R's own forms,
# where JavaScript needed a trailing options object. Two things differ, and both
# are the language's rather than the grammar's:
#
#   * a column is a symbol, `:gdp` (see `columns.jl`);
#   * `bounds(start = …, end = …)` needs `var"end"`, because `end` is reserved
#     syntax in Julia. Six sentences in the manual use it. That `bounds` is also
#     the atom that reads worst in JavaScript — for an unrelated reason, all its
#     arguments being columns — is the second sign that this atom wants a shape of
#     its own (spec §9.2 of the publishing plan said so first).
#
# The value checks live here rather than in Rust for the reason R's do: the caller
# gets the error at the line that wrote it, and a misspelling never reaches the
# wire as an enum serde cannot decode. What is *legal* — which mark takes which
# channel, whether this transform means anything on that mark — stays in
# the engine's `legality`, where every binding inherits it.

# One positional argument that may also be given by name, which is R's calling
# convention and not Julia's. `bin(30)` and `bin(bins = 30)` both reach `bins`.
function one_of(args::Tuple, named, atom::AbstractString, name::AbstractString)
    isempty(args) && return named
    if length(args) > 1
        throw(GogError("gog: `$atom()` takes one positional argument, `$name`. " *
                       "Anything else is given by name, e.g. `$atom($name = …)`."))
    end
    named === nothing || throw(GogError("gog: `$atom()` was given `$name` twice."))
    args[1]
end

function whole_number(value, atom::AbstractString, argument::AbstractString,
                      example::AbstractString)
    if !(value isa Real) || value isa Bool || !isfinite(value) ||
       value < 1 || value != round(value)
        throw(GogError("gog: `$atom($argument = )` needs one positive whole number, " *
                       "e.g. `$example`."))
    end
    Int(value)
end

# A named reading, checked for *shape* only — which words exist is the engine's
# question, so every binding forwards the string and one refusal covers all four.
function one_word(value, argument::AbstractString)
    if !(value isa AbstractString)
        throw(GogError("gog: `density($argument = )` takes one word — " *
                       "\"shape\" or \"count\"."))
    end
    String(value)
end

function positive(value, atom::AbstractString, argument::AbstractString,
                  example::AbstractString; unit = "", purpose = "")
    if !(value isa Real) || value isa Bool || !isfinite(value) || value <= 0
        throw(GogError("gog: `$atom($argument = )` needs one positive number$unit, " *
                       "e.g. `$example`$purpose."))
    end
    Float64(value)
end

# ---------------------------------------------------------------------------
# Marks — the geometric forms
# ---------------------------------------------------------------------------

mark_atom(name::AbstractString) = Atom(:mark, Dict{Symbol,Any}(:mark => name))

# The help text a family of words shares, written once and placed in each word's
# docstring, so that every mark's help says the same thing about marks and the
# copies cannot drift apart. R keeps each family on one page; Julia gives every
# word a help entry of its own.
const MARK_HELP = """
A mark is the shape a layer draws, and every layer has exactly one. A mark
carries no data of its own. Channels say which column goes to which part of the
picture, and the engine then draws every line and shape.

A mark is written bare, without parentheses:
`data(table) + point + x(:column) + y(:column)`. gog calls a line like this a
sentence. A mark and its required positions together are the smallest sentence
that draws; neither draws alone. Each mark accepts its own set of channels. A
channel the mark cannot draw is refused, and the message says what to write
instead.

A mark is an [`Atom`](@ref) itself, so there is nothing to call and nothing it
returns. Add it to a plot with `+`, or join a transform to it with `*`."""

# The paragraph a word's help gives when Julia's `Base` exports the same name.
base_name_help(name::AbstractString) = """
Julia's `Base` also has a `$name`, so your script must say which one it uses.
The first line of the example imports gog's word, together with the eight other
words of the grammar that `Base` also has:
`using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map`.
After that line, `Base.$name` still gives Julia's own `$name`. At the REPL,
`?$name` shows this help after that line; before it, `?GrammarOfGraphics.$name`
does."""

# And when the `Statistics` standard library has the name, which matters only in a
# session that loads it.
statistics_name_help(name::AbstractString) = """
Julia's `Statistics` library also has a `$name`. A session that loads it with
`using Statistics` must say which one it uses, as for the nine names `Base`
shares with gog: `using GrammarOfGraphics: mean, median, quantile`."""

"""
    point

One symbol for each row, a dot unless a shape is chosen: how two measurements
vary together.

$MARK_HELP

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

# A mark and its positions are the smallest sentence that draws.
p = data(cars) + point + x(:wt) + y(:mpg)

# The same table, a different shape: one word is all that changes.
q = data(cars) + line + x(:wt) + y(:mpg)
```
"""
const point    = mark_atom("point")

"""
    line

Joins the rows in the order of `x`: how one value changed over time.

$MARK_HELP

# Examples
```julia
sales = (year = [2019, 2020, 2021, 2022, 2023],
         sales = [120, 135, 128, 150, 162])

p = data(sales) + line + x(:year) + y(:sales)

# A line and its points, two layers over one table.
q = data(sales) + line + point + x(:year) + y(:sales)
```
"""
const line     = mark_atom("line")

"""
    path

Joins the rows in the order of the table rather than the order of `x`, so a
path may turn back on itself.

$MARK_HELP

# Examples
```julia
geyser = (eruptions = [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95,
                       4.35, 1.833, 3.917],
          waiting = [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84])

# Each eruption joined to the next one, in the order they happened.
p = data(geyser) + path + point + x(:eruptions) + y(:waiting)
```
"""
const path     = mark_atom("path")

"""
    rule

A reference line across the panel, the line everything else is compared with.

A rule is placed by one column and runs across the whole panel on the other
axis. With `style(reach = "edge")` it is a short tick at the edge of the panel
instead; one tick for each row makes a rug.

$MARK_HELP

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

# A line across the panel at each car's mileage.
p = data(cars) + point + x(:wt) + y(:mpg) + rule + y(:mpg)

# A rug: one short tick for each car's weight, at the bottom of the panel.
q = data(cars) + point + x(:wt) + y(:mpg) + rule + x(:wt) + style(reach = "edge")
```
"""
const rule     = mark_atom("rule")

"""
    zone

Shades a region of the panel: a rectangle, or, on a map or a globe, the shape of
a boundary.

A zone needs something to say where its sides are: a category on a position,
[`bounds`](@ref) that name the sides from columns, [`bin`](@ref) or
[`density`](@ref) cutting the panel into cells, or a boundary named by `group`
on a [`map`](@ref) or a [`globe`](@ref).

$MARK_HELP

# Examples
```julia
geyser = (eruptions = [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95,
                       4.35, 1.833, 3.917],
          waiting = [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84])

# The panel cut into cells, each shaded by how many rows fall in it.
p = data(geyser) + zone * bin + x(:eruptions) + y(:waiting)
```
"""
const zone     = mark_atom("zone")

"""
    area

Fills the region between the line and the baseline.

$MARK_HELP

# Examples
```julia
sales = (year = [2019, 2020, 2021, 2022, 2023],
         sales = [120, 135, 128, 150, 162])

p = data(sales) + area + x(:year) + y(:sales)
```
"""
const area     = mark_atom("area")

"""
    bar

A rectangle from the baseline to the value: how large each category is.

$MARK_HELP

# Examples
```julia
cars = (car = ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
               "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

p = data(cars) + bar + x(:car) + y(:mpg)
```
"""
const bar      = mark_atom("bar")

"""
    step

The line drawn with right angles: a value stays the same until it changes.

$MARK_HELP

$(base_name_help("step"))

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

sales = (year = [2019, 2020, 2021, 2022, 2023],
         sales = [120, 135, 128, 150, 162])

p = data(sales) + step + x(:year) + y(:sales)
```
"""
const step     = mark_atom("step")

"""
    interval

A segment from a low value to a high one. It needs a pair of values, so it is
usually written with a transform that makes one, such as [`confidence`](@ref)
or [`range`](@ref).

$MARK_HELP

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# The mean of each species, with its 95% confidence interval.
p = data(flowers) + interval * confidence + x(:species) + y(:petal_length)
```
"""
const interval = mark_atom("interval")

"""
    ribbon

A band between two values across a span.

$MARK_HELP

# Examples
```julia
geyser = (eruptions = [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95,
                       4.35, 1.833, 3.917],
          waiting = [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84])

# A trend, and the band that says how precisely it is known.
p = data(geyser) + x(:eruptions) + y(:waiting) + point +
    ribbon * smooth_band + line * smooth
```
"""
const ribbon   = mark_atom("ribbon")

"""
    text

Draws the text given by [`label`](@ref) at each position.

$MARK_HELP

# Examples
```julia
cars = (car = ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
               "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
        wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

# Each car's name, written at its weight and mileage.
p = data(cars) + text + x(:wt) + y(:mpg) + label(:car)
```
"""
const text     = mark_atom("text")

# A sheet through the samples, and the one mark that draws in the cube alone. Its
# rows are nodes: the grid the two position columns describe is recovered rather
# than declared, so it wants one row per (x, y) crossing. Three positions, all
# required and all numeric — a face asserts every value *between* two nodes, and
# between two categories there is nothing to assert (for a mesh over categories,
# `bar * bin + space()`). One transform, `density`, which makes it the third
# geometry of one field: `zone * density` paints it as cells, `path * density`
# traces its contours, `surface * density` raises it with the estimate as height.
"""
    surface

A sheet over a grid of samples, for data in three dimensions.

It takes three positions, `x`, `y` and `z`, all numbers, and one row for each
place where an `x` value meets a `y` value. It draws only in the cube.

$MARK_HELP

# Examples
```julia
hill = (x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4],
        y = [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4],
        height = [0.223, 0.435, 0.435, 0.223, 0.435, 0.846, 0.846, 0.435,
                  0.435, 0.846, 0.846, 0.435, 0.223, 0.435, 0.435, 0.223])

p = data(hill) + surface + x(:x) + y(:y) + z(:height)
```
"""
const surface  = mark_atom("surface")

# The stroke whose two endpoints the layout supplies — one row is one edge of a
# graph. Nothing binds to its positions; its minimum syllable is
# `edge * layout(:a, :b)` inside `network()`, and like `surface` it does not
# draw flat.
"""
    edge

The links of a network: one row is one link, and [`layout`](@ref) supplies its
two ends. Nothing binds to its positions, and it draws only inside
[`network`](@ref).

$MARK_HELP

# Examples
```julia
links = (sender = ["Ann", "Ann", "Bob", "Cy", "Dee"],
         receiver = ["Bob", "Cy", "Cy", "Dee", "Ann"])

# The links, and the people as points.
p = data(links) + edge * layout(:sender, :receiver) +
    point * layout(:sender, :receiver) + network()
```
"""
const edge = mark_atom("edge")

# `box` — the box-and-whisker mark, with its one knob.
"""
    box
    box(; whiskers)

The box-and-whisker mark: the median, the quartiles and the whiskers of each
group.

Bare `box` draws the standard box plot. Call it to choose how far the whiskers
reach.

# Arguments
- `whiskers`: `"tukey"` (the default) runs the whiskers to the most extreme
  value no more than 1.5 times the interquartile range (IQR) beyond the box, and
  draws the values past that as outliers: the standard box plot. `"range"` runs
  them to the true minimum and maximum, with no outliers: the plain five-number
  summary. It may also be given by position:
  `box("range")`.

Returns an [`Atom`](@ref), a mark, which `+` adds to a plot.

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3])

# One box for each species.
b = data(flowers) + box + x(:species) + y(:sepal_length)

# Whiskers that reach the smallest and the largest value.
r = data(flowers) + box(whiskers = "range") + x(:species) + y(:sepal_length)
```
"""
const box = Atom(:mark, Dict{Symbol,Any}(:mark => "box"),
    function (args...; whiskers = nothing)
        whiskers = one_of(args, whiskers, "box", "whiskers")
        if whiskers !== nothing && !(whiskers in ("tukey", "range"))
            throw(GogError(
                "gog: `box(whiskers = )` is either \"tukey\" (the default — whiskers " *
                "to 1.5*IQR, points beyond drawn as outliers) or \"range\" (whiskers " *
                "to the true min and max, no outliers)."))
        end
        atom = Atom(:mark, Dict{Symbol,Any}(:mark => "box"))
        whiskers === nothing ||
            (atom.fields[:box] = Dict{String,Any}("whiskers" => whiskers))
        atom
    end)

# ---------------------------------------------------------------------------
# Transforms — used with `*`:  bar * bin,  line * smooth
# ---------------------------------------------------------------------------

transform_atom(name::AbstractString) = Atom(:transform, Dict{Symbol,Any}(:transform => name))

# The help every bare transform shares, as `MARK_HELP` is for the marks.
const TRANSFORM_HELP = """
A transform replaces the rows with a summary of them, and the mark then draws
that summary. Join it to a mark with `*`: `bar * count` draws one bar for each
category, as tall as its number of rows. A chart comes from a mark and a
transform together, not from a list of chart types.

The same transform behaves the same way on every mark that accepts it. Keep the
statistic and change the mark, and the same numbers draw a line, an area or a
step.

This transform is an [`Atom`](@ref) itself, written bare, so there is nothing to
call and nothing it returns. Join it to a mark with `*`. The transforms that take
an argument have help of their own: [`bin`](@ref), [`density`](@ref),
[`quantile`](@ref), [`range`](@ref), [`deviation`](@ref), [`confidence`](@ref),
[`smooth_band`](@ref), [`straight_band`](@ref), [`bounds`](@ref),
[`partition`](@ref), [`cumulative`](@ref), [`flow`](@ref), [`layout`](@ref) and
[`cluster`](@ref). [`repel`](@ref), which moves text labels apart, has help of
its own too."""

# The help the words that resolve overlap share: `dodge`, `stack` and `jitter`.
const COLLISION_HELP = """
When several marks land in the same place, they hide each other. `stack`,
`dodge` and `jitter` move them apart, and compute no statistic. Like transforms
they join a mark with `*`, so they work together with a statistic rather than
replace it: `bar * sum * stack` adds up each group and then piles the results.

`stack` and `dodge` both need groups: a `color`, `pattern` or `group` that says
which mark belongs to which group. Without one there is nothing to stack or
dodge, and the engine says so. `point * dodge` needs none, since the points in
one category already overlap. `box` and `interval` take `dodge` but not
`stack`."""

"""
    smooth

A fitted trend through the points, rather than the points.

$TRANSFORM_HELP

# Examples
```julia
geyser = (eruptions = [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95,
                       4.35, 1.833, 3.917],
          waiting = [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84])

# The points, and the trend through them.
p = data(geyser) + point + line * smooth + x(:eruptions) + y(:waiting)
```
"""
const smooth     = transform_atom("smooth")

# `straight` — the least-squares line of `y` against `x`, `smooth`'s straight sibling.
#
# `line * straight` draws one fitted line per group, and `ribbon * straight_band` its
# band. It needs two different values of `x`, and it is refused on a log `y` axis, where
# the line would be drawn as a curve.
"""
    straight

The least-squares line through the points: a straight fit, where `smooth` may
bend.

`line * straight` draws one fitted line for each group, and
`ribbon * straight_band` draws its band ([`straight_band`](@ref)). It needs two
different values of `x`. It is refused on a log `y` axis, where the line would
be drawn as a curve.

$TRANSFORM_HELP

# Examples
```julia
geyser = (eruptions = [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95,
                       4.35, 1.833, 3.917],
          waiting = [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84])

# The points, and the least-squares line through them.
p = data(geyser) + point + line * straight + x(:eruptions) + y(:waiting)
```
"""
const straight   = transform_atom("straight")

"""
    count

The number of rows for each value of the position, such as `x(:cylinders)`.

$TRANSFORM_HELP

$(base_name_help("count"))

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

cars = (cylinders = ["6", "6", "4", "6", "8", "6", "8", "4"],)

# How many cars have each number of cylinders.
h = data(cars) + bar * count + x(:cylinders)

# The same statistic, a different shape.
l = data(cars) + line * count + x(:cylinders)
```
"""
const count      = transform_atom("count")

"""
    sum

The total of the measure within each group.

$TRANSFORM_HELP

$(base_name_help("sum"))

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

titanic = (travel_class = ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
           people = [122, 167, 528, 673, 203, 118, 178, 212])

# How many people traveled in each class.
p = data(titanic) + bar * sum + x(:travel_class) + y(:people)
```
"""
const sum        = transform_atom("sum")

"""
    mean

The mean of the measure within each group.

$TRANSFORM_HELP

$(statistics_name_help("mean"))

# Examples
```julia
cars = (mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
        cylinders = ["6", "6", "4", "6", "8", "6", "8", "4"])

# The mean mileage for each number of cylinders.
m = data(cars) + bar * mean + x(:cylinders) + y(:mpg)
```
"""
const mean       = transform_atom("mean")

"""
    median

The median of the measure within each group.

$TRANSFORM_HELP

$(statistics_name_help("median"))

# Examples
```julia
trees = (age = [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
         circumference = [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108])

# The median size of the three trees at each age.
p = data(trees) + point + line * median + x(:age) + y(:circumference)
```
"""
const median     = transform_atom("median")

"""
    max

The largest value of the measure within each group.

$TRANSFORM_HELP

$(base_name_help("max"))

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

trees = (age = [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
         circumference = [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108])

# The largest circumference among the three trees at each age.
p = data(trees) + point + line * max + x(:age) + y(:circumference)
```
"""
const max        = transform_atom("max")

"""
    min

The smallest value of the measure within each group.

$TRANSFORM_HELP

$(base_name_help("min"))

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

trees = (age = [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
         circumference = [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108])

# The smallest circumference among the three trees at each age.
p = data(trees) + point + line * min + x(:age) + y(:circumference)
```
"""
const min        = transform_atom("min")

"""
    proportion

Each group's share of the total, rather than its count.

$TRANSFORM_HELP

# Examples
```julia
cars = (cylinders = ["6", "6", "4", "6", "8", "6", "8", "4"],)

# The share of the cars that have each number of cylinders.
p = data(cars) + bar * proportion + x(:cylinders)
```
"""
const proportion = transform_atom("proportion")

"""
    dodge

Places the marks of each group side by side, so that each is measured from the
same baseline and they can be compared directly. On `point` it moves each point
across its category only as far as needed to stop it touching its neighbors: the
beeswarm.

$COLLISION_HELP

`dodge` is an [`Atom`](@ref) itself, written bare, so there is nothing to call
and nothing it returns. Join it to a mark with `*`.

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

titanic = (travel_class = ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
           survived = ["No", "No", "No", "No", "Yes", "Yes", "Yes", "Yes"],
           people = [122, 167, 528, 673, 203, 118, 178, 212])

# Side by side: each bar measured from the same baseline.
d = data(titanic) + bar * sum * dodge + x(:travel_class) + y(:people) +
    color(:survived)
```
"""
const dodge      = transform_atom("dodge")

# The fourth collision modifier: `text * repel` moves labels that overlap each
# other, where the other three move marks that share a position. Bare like `dodge`,
# because a label moves as far as the overlap requires and no further.
"""
    repel

Moves overlapping labels apart until they can be read.

`text * repel` finds where the labels and the points are, and pushes each label
a different way until the labels stop overlapping. When a label has moved away
from its point, a thin line leads back to the point. When the labels cannot all
be separated, every label is still drawn, and the plot reports how many still
overlap.

`style(nudge = )` moves every label the same way, by a fixed distance; `repel`
moves each label as far as its own overlap needs. [`jitter`](@ref) moves points
apart along a category axis.

`repel` is an [`Atom`](@ref) itself, written bare, so there is nothing to call
and nothing it returns. Join it to the `text` mark with `*`.

# Examples
```julia
cars = (car = ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
               "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
        wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

# Each car's name, pushed away from the others so that no two overlap.
p = data(cars) + point + text * repel + x(:wt) + y(:mpg) + label(:car)
```
"""
const repel      = transform_atom("repel")

# `bin` — equal-width buckets. How many dimensions it cuts is the mark's answer.
"""
    bin
    bin(bins; tiling)
    bin(; width, tiling)

Cut a continuous column into bins of equal width and count the rows in each:
the histogram transform.

The mark decides how many axes `bin` cuts. `bar * bin + x(:a)` cuts one axis and
draws a histogram. `zone * bin + x(:a) + y(:b)` cuts both and shades each cell by
its count. Bare `bin` lets the engine choose the number of bins.

# Arguments
- `bins`: the number of bins, one positive whole number. It is the one argument
  given by position: `bin(30)`.
- `width`: the width of each bin, in the column's own units:
  `bin(width = 5)`. Give `bins` or `width`, not both.
- `tiling`: how the plane is cut when both axes are binned: `"rect"` (the
  default) or `"hex"`. Only for bins in two dimensions:
  `zone * bin(tiling = "hex")`.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
geyser = (eruptions = [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95,
                       4.35, 1.833, 3.917],
          waiting = [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84])

# A histogram: each bar counts the rows that fall in its bin.
h = data(geyser) + bar * bin + x(:eruptions)

# Twenty bins, or bins a quarter of a minute wide.
h20 = data(geyser) + bar * bin(20) + x(:eruptions)
hw = data(geyser) + bar * bin(width = 0.25) + x(:eruptions)

# Both axes cut, into hexagons.
hex = data(geyser) + zone * bin(tiling = "hex") + x(:eruptions) + y(:waiting)
```
"""
const bin = Atom(:transform, Dict{Symbol,Any}(:transform => "bin"),
    function (args...; bins = nothing, width = nothing, tiling = nothing)
        bins = one_of(args, bins, "bin", "bins")
        if bins !== nothing && width !== nothing
            throw(GogError(
                "gog: `bin()` takes either `bins` or `width`, not both. Write " *
                "`bin(30)` for a bin count or `bin(width = 5)` for a bin width."))
        end
        tiling === nothing || tiling isa AbstractString ||
            throw(GogError("gog: `bin(tiling = )` needs one name, `\"rect\"` or `\"hex\"`."))
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "bin",
            :bins => bins === nothing ? nothing : whole_number(bins, "bin", "bins", "bin(30)"),
            :width => width === nothing ? nothing :
                      positive(width, "bin", "width", "bin(width = 5)"),
            :tiling => tiling))
    end)

# `density` — the smooth estimate; `levels` cuts a field into bands; `compare`
# says what a violin's width means from one slot to the next.
"""
    density
    density(adjust; levels, compare, reach)
    density(; bandwidth, levels, compare, reach)

Estimate how a continuous column is spread, as a smooth curve: a kernel density
estimate.

With one position, `line * density + x(:a)` draws the curve. With a category on
the other position, `ribbon * density + x(:group) + y(:a)` draws a violin for
each category. With two numeric positions, the estimate is a field over the
plane: `zone * density` shades it, `path * density` traces its contour lines,
and `surface * density` raises it in the cube.

# Arguments
- `adjust`: a multiplier on the bandwidth the engine chooses: `density(2)`
  doubles the bandwidth, so the curve is smoother; `density(0.5)` halves it. It
  is the one argument given by position.
- `bandwidth`: the bandwidth in the column's own units, as in
  `density(bandwidth = 5)`. Give `adjust` or `bandwidth`, not both. On a
  two-dimensional density, such as `zone * density`, the two axes measure
  different things, so one width cannot fit both. gog refuses it there and
  points you to `adjust`.
- `levels`: how many levels to cut a two-dimensional density into, one whole
  number:
  `path * density(levels = 8)` traces their boundaries, and
  `zone * density(levels = 8)` fills between them. Refused on a density curve,
  which is one line with nothing to cut.
- `compare`: what a violin's width means from one category to the next.
  `"count"` (the default) scales each violin by its group's number of rows, so a
  thin violin is a small group. `"shape"` draws every violin with the same area,
  so only the shapes are compared. For violins only.
- `reach`: how far each violin reaches from the line of its category, measured
  in slots (a slot is the space one category takes on the axis). `0.4` (the
  default) keeps every shape inside its own slot. Above `0.5` the shapes overlap
  their neighbors, which is the ridgeline plot:
  `area * density(reach = 2.5) + x(:number) + y(:category)`. Like `compare`, it
  needs a category to give it slots.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
geyser = (eruptions = [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95,
                       4.35, 1.833, 3.917],)

# A smooth estimate of how the eruption times are spread.
d = data(geyser) + line * density + x(:eruptions)

# Twice the bandwidth: a smoother curve.
d2 = data(geyser) + line * density(2) + x(:eruptions)
```
"""
const density = Atom(:transform, Dict{Symbol,Any}(:transform => "density"),
    function (args...; adjust = nothing, bandwidth = nothing, levels = nothing,
              compare = nothing, reach = nothing)
        adjust = one_of(args, adjust, "density", "adjust")
        if adjust !== nothing && bandwidth !== nothing
            throw(GogError(
                "gog: `density()` takes either `adjust` or `bandwidth`, not both. " *
                "Write `density(2)` to scale the automatic bandwidth, or " *
                "`density(bandwidth = 5)` to set it in the data's own units."))
        end
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "density",
            :adjust => adjust === nothing ? nothing :
                       positive(adjust, "density", "adjust", "density(2)"),
            :bandwidth => bandwidth === nothing ? nothing :
                          positive(bandwidth, "density", "bandwidth", "density(bandwidth = 5)"),
            :levels => levels === nothing ? nothing :
                       whole_number(levels, "density", "levels", "path * density(levels = 8)"),
            # One of two words, checked here only for *shape* — which words exist is
            # the engine's question, so a typo gets one message in all four bindings
            # rather than four (`legality::check_density_params`).
            :compare => compare === nothing ? nothing : one_word(compare, "compare"),
            :reach => reach === nothing ? nothing :
                      positive(reach, "density", "reach", "density(reach = 2.5)";
                               unit = " of slots", purpose = " for overlapping ridges")))
    end)

"""
    confidence
    confidence(level)

The confidence interval of the mean in each group, for a mark that draws a low
and a high value, such as `interval` or `ribbon`.

`interval * confidence` draws each group's mean as a dot with the interval
around it. It says how well the mean is known; [`deviation`](@ref) says how
spread the data is.

# Arguments
- `level`: the confidence level, one number strictly between 0 and 1; 0.95 when
  left out. Given by position or by name: `confidence(0.99)`.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# The mean of each species, with its 95% confidence interval.
p = data(flowers) + interval * confidence + x(:species) + y(:petal_length)

# A wider interval, at 99%.
q = data(flowers) + interval * confidence(0.99) + x(:species) + y(:petal_length)
```
"""
const confidence = Atom(:transform, Dict{Symbol,Any}(:transform => "confidence"),
    function (args...; level = nothing)
        level = one_of(args, level, "confidence", "level")
        if level !== nothing && (!(level isa Real) || level isa Bool ||
                                 !isfinite(level) || !(0 < level < 1))
            throw(GogError(
                "gog: `confidence(level = )` needs one number strictly between 0 and " *
                "1, e.g. `confidence(0.95)`."))
        end
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "confidence",
            :level => level === nothing ? nothing : Float64(level)))
    end)

"""
    smooth_band
    smooth_band(level)

The band around a [`smooth`](@ref) line: the confidence band of the fitted curve.

At each point of `smooth`'s fitted curve, `smooth_band` gives the low and the
high end of the confidence band around it, so
`ribbon * smooth_band + line * smooth` draws the trend with its band.

# Arguments
- `level`: the confidence level, one number strictly between 0 and 1; 0.95 when
  left out. Given by position or by name: `smooth_band(0.99)`.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
geyser = (eruptions = [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95,
                       4.35, 1.833, 3.917],
          waiting = [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84])

# The trend, and how precisely it is known.
p = data(geyser) + x(:eruptions) + y(:waiting) + point +
    ribbon * smooth_band + line * smooth
```
"""
const smooth_band = Atom(:transform, Dict{Symbol,Any}(:transform => "smooth_band"),
    function (args...; level = nothing)
        level = one_of(args, level, "smooth_band", "level")
        if level !== nothing && (!(level isa Real) || level isa Bool ||
                                 !isfinite(level) || !(0 < level < 1))
            throw(GogError(
                "gog: `smooth_band(level = )` needs one number strictly between 0 and " *
                "1, e.g. `smooth_band(0.95)`."))
        end
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "smooth_band",
            :level => level === nothing ? nothing : Float64(level)))
    end)

"""
    straight_band
    straight_band(level)

The band around a [`straight`](@ref) line: the interval of the fitted line's
mean.

At each point of `straight`'s least-squares line, `straight_band` gives the low
and the high end of the confidence interval for the line's mean, so
`ribbon * straight_band + line * straight` draws the line with its band. It
needs three rows in each group, and two different values of `x`.

# Arguments
- `level`: the confidence level, one number strictly between 0 and 1; 0.95 when
  left out. Given by position or by name: `straight_band(0.99)`.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
geyser = (eruptions = [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95,
                       4.35, 1.833, 3.917],
          waiting = [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84])

# The least-squares line, with its 95% confidence band.
p = data(geyser) + x(:eruptions) + y(:waiting) + point +
    ribbon * straight_band + line * straight

# A wider band, at 99%.
q = data(geyser) + x(:eruptions) + y(:waiting) + point +
    ribbon * straight_band(0.99) + line * straight
```
"""
const straight_band = Atom(:transform, Dict{Symbol,Any}(:transform => "straight_band"),
    function (args...; level = nothing)
        level = one_of(args, level, "straight_band", "level")
        if level !== nothing && (!(level isa Real) || level isa Bool ||
                                 !isfinite(level) || !(0 < level < 1))
            throw(GogError(
                "gog: `straight_band(level = )` needs one number strictly between 0 and " *
                "1, e.g. `straight_band(0.95)`."))
        end
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "straight_band",
            :level => level === nothing ? nothing : Float64(level)))
    end)

# `range` — the band per group, the whole group unless told otherwise.
#
# The only atom with *two* positional arguments, because a band has two ends and
# neither is the more common one to set. `range(0.25, 0.75)` is the middle half;
# bare `range` is the minimum to the maximum, which is what `range` has always
# drawn.
"""
    range
    range(low, high)

A band between two quantiles of each group, for marks that draw from a low
value to a high one, such as `interval` and `ribbon`.

Bare `range` runs from the smallest value of each group to the largest.

# Arguments
- `low`, `high`: the band's two ends, each a quantile probability between 0
  and 1. Given by position, `range(0.25, 0.75)` is the middle half. An end left
  out is that side's extreme, so `range(high = 0.9)` runs from the smallest
  value to the 90th percentile.

Returns an [`Atom`](@ref) that `*` joins to a mark.

$(base_name_help("range"))

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# Each species from its shortest petal to its longest.
p = data(flowers) + interval * range + x(:species) + y(:petal_length)

# The middle half of each species, from the 25th to the 75th percentile.
q = data(flowers) + interval * range(0.25, 0.75) + x(:species) + y(:petal_length)
```
"""
const range = Atom(:transform, Dict{Symbol,Any}(:transform => "range"),
    function (args...; low = nothing, high = nothing)
        if length(args) > 2
            throw(GogError(
                "gog: `range()` takes two positional arguments, the band's two " *
                "ends: `range(0.25, 0.75)`."))
        end
        if length(args) >= 1
            low === nothing || throw(GogError("gog: `range()` was given `low` twice."))
            low = args[1]
        end
        if length(args) == 2
            high === nothing || throw(GogError("gog: `range()` was given `high` twice."))
            high = args[2]
        end
        # Two different mistakes reach here and they want different directions,
        # so the message splits on the shape Julia's own builtin is called with.
        # `range(1, 10)` is whole numbers out of 0..1, which is nobody's
        # quantile, so that reading is named. A float out of 0..1 is a mistyped
        # band end and gets told about quantiles instead.
        for (name, value) in (("low", low), ("high", high))
            value === nothing && continue
            builtin_shaped = !(value isa Real) || value isa Bool ||
                             !isfinite(value) ||
                             (value isa Integer && !(0 <= value <= 1))
            if builtin_shaped
                throw(GogError(
                    "gog: `range()` takes the band's two ends, each one number " *
                    "between 0 and 1, e.g. `range(0.25, 0.75)`. For a sequence of " *
                    "numbers, gog shadows that name: use `Base.range` for Julia's, " *
                    "and `Base.extrema` for a collection's smallest and largest."))
            end
            if !(0 <= value <= 1)
                throw(GogError(
                    "gog: `range($name = $value)` is not a probability — the " *
                    "band's ends are quantiles, so each is between 0 and 1. " *
                    "`range(0.25, 0.75)` is the middle half, `range(0.1, 0.9)` the " *
                    "middle 80 percent, and bare `range` the whole group."))
            end
        end
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "range",
            :low => low === nothing ? nothing : Float64(low),
            :high => high === nothing ? nothing : Float64(high)))
    end)

"""
    deviation
    deviation(multiplier)

A band around the mean of each group, a number of standard deviations wide on
each side.

It gives the same pair of ends and center as [`confidence`](@ref), for a
different question: `confidence` says how well the mean is known, and
`deviation` says how spread the data is.

# Arguments
- `multiplier`: how many standard deviations reach each side of the mean, one
  positive number; 1 when left out. Given by position or by name:
  `deviation(2)`.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# The mean of each species, plus and minus one standard deviation.
p = data(flowers) + interval * deviation + x(:species) + y(:petal_length)

# Two standard deviations.
q = data(flowers) + interval * deviation(2) + x(:species) + y(:petal_length)
```
"""
const deviation = Atom(:transform, Dict{Symbol,Any}(:transform => "deviation"),
    function (args...; multiplier = nothing)
        multiplier = one_of(args, multiplier, "deviation", "multiplier")
        if multiplier !== nothing && (!(multiplier isa Real) || multiplier isa Bool ||
                                      !isfinite(multiplier) || multiplier <= 0)
            throw(GogError(
                "gog: `deviation(multiplier = )` needs one positive number \u2014 it " *
                "counts standard deviations out from the mean. `deviation` is one, " *
                "`deviation(2)` is two."))
        end
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "deviation",
            :multiplier => multiplier === nothing ? nothing : Float64(multiplier)))
    end)

"""
    quantile(p)

The value of the measure at one probability in each group: `quantile(0.9)` is
the 90th percentile.

There is no default, so bare `quantile` is refused. At 0, 0.5 and 1 the plot
draws and says that `min`, `median` and `max` are the plain names for the same
numbers.

# Arguments
- `p`: the probability, one number between 0 and 1. Given by position or by
  name: `quantile(0.9)`, `quantile(p = 0.9)`.

Returns an [`Atom`](@ref) that `*` joins to a mark.

$(statistics_name_help("quantile"))

# Examples
```julia
trees = (age = [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
         circumference = [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108])

# The 90th percentile and the median of the three trees at each age.
p = data(trees) + x(:age) + y(:circumference) +
    line * quantile(0.9) +
    line * median
```
"""
const quantile = Atom(:transform, Dict{Symbol,Any}(:transform => "quantile"),
    function (args...; p = nothing)
        p = one_of(args, p, "quantile", "p")
        if p !== nothing
            if !(p isa Real) || p isa Bool || !isfinite(p)
                throw(GogError(
                    "gog: `quantile()` takes one number between 0 and 1, the " *
                    "probability it reduces to, e.g. `quantile(0.9)`. For the " *
                    "quantiles of a collection, gog shadows that name: use " *
                    "`Statistics.quantile` for Julia's."))
            end
            if !(0 <= p <= 1)
                throw(GogError(
                    "gog: `quantile($p)` is not a probability \u2014 a quantile is " *
                    "between 0 and 1. `quantile(0.9)` is the 90th percentile, " *
                    "`quantile(0.5)` the middle."))
            end
        end
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "quantile", :p => p === nothing ? nothing : Float64(p)))
    end)

"""
    jitter
    jitter(amount)

Spread the points of a strip plot (points over categories) apart along the
category axis only, so that points with the same value do not hide each other.

$COLLISION_HELP

# Arguments
- `amount`: the spread as a multiple of the default, one number from 0 to 1.25;
  1 when left out. Given by position or by name: `jitter(0.5)` is half the
  spread. At 1.25 a point reaches the edge of its category's slot, and more is
  refused when the plot is drawn, because a point would land in the next
  category.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3])

# Points moved a little sideways, so that they do not hide each other.
p = data(flowers) + point * jitter + x(:species) + y(:sepal_length)

# Half the spread.
q = data(flowers) + point * jitter(0.5) + x(:species) + y(:sepal_length)
```
"""
const jitter = Atom(:transform, Dict{Symbol,Any}(:transform => "jitter"),
    function (args...; amount = nothing)
        amount = one_of(args, amount, "jitter", "amount")
        if amount !== nothing && (!(amount isa Real) || amount isa Bool ||
                                  !isfinite(amount) || amount < 0)
            throw(GogError(
                "gog: `jitter(amount = )` needs one non-negative number — the spread " *
                "as a multiple of the default, e.g. `jitter(0.5)` for half, up to " *
                "`jitter(1.25)`, the widest spread that keeps every point in its own " *
                "category."))
        end
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "jitter",
            :amount => amount === nothing ? nothing : Float64(amount)))
    end)

# `stack` — the measure-axis pile. `stack(share = true)` fills every pile to 1.
#
# The 100% stacked bar. A parameter here rather than a second reading of
# `proportion` because the two divide by different totals — `proportion` by the
# whole frame's, this by the slot's own — and because it composes with any
# measurement, including a `sum` that `proportion` has no column to take.
#
# `stack(baseline = )` says where each pile *hangs*, the other free choice once the
# heights are fixed: `"zero"` stands every pile on the axis (the default),
# `"center"` hangs each so its middle is at zero, `"wiggle"` chooses the foot that
# makes the bands as flat as it can — the streamgraph. Orthogonal to `share`, which
# scales the heights rather than placing them.
"""
    stack
    stack(share; baseline)
    stack(; share, baseline)

Pile the marks of each group end to end, so that the total is the height of the
pile.

$COLLISION_HELP

# Arguments
- `share`: `true` fills every pile to 1, so the plot shows each group's share of
  its pile rather than its amount: the 100% stacked bar. `false` (the default)
  piles the values themselves. It is the one argument given by position.
- `baseline`: where each pile starts. `"zero"` (the default) starts every pile
  at zero. `"center"` places each pile so that its middle is at zero.
  `"wiggle"` chooses the start that keeps the bands as flat as it can: the
  streamgraph.

Returns an [`Atom`](@ref) that `*` joins to a mark.

$(base_name_help("stack"))

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

titanic = (travel_class = ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
           survived = ["No", "No", "No", "No", "Yes", "Yes", "Yes", "Yes"],
           people = [122, 167, 528, 673, 203, 118, 178, 212])

# Piled: the height of each pile is the number of people in that class.
s = data(titanic) + bar * sum * stack + x(:travel_class) + y(:people) +
    color(:survived)

# Every pile the same height, so the plot shows the share who survived.
f = data(titanic) + bar * sum * stack(share = true) + x(:travel_class) +
    y(:people) + color(:survived)
```
"""
const stack = Atom(:transform, Dict{Symbol,Any}(:transform => "stack"),
    function (args...; share = nothing, baseline = nothing)
        share = one_of(args, share, "stack", "share")
        if share !== nothing && !(share isa Bool)
            throw(GogError(
                "gog: `stack(share = )` is true or false — true fills every pile to 1 " *
                "(the 100% stacked bar), false piles the values themselves. For shares " *
                "of the whole plot rather than of each slot, `proportion` is the " *
                "transform you want."))
        end
        if baseline !== nothing && !(baseline isa AbstractString)
            throw(GogError(
                "gog: `stack(baseline = )` is one of \"zero\", \"center\" or \"wiggle\" — " *
                "\"zero\" stands every pile on the axis, \"center\" hangs each pile so its " *
                "middle is at zero, \"wiggle\" chooses the foot that makes the bands as " *
                "flat as it can (the streamgraph)."))
        end
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "stack",
            :share => share,
            :baseline => baseline))
    end)

"""
    cumulative
    cumulative(share)

The running total of a measure, along its axis.

`cumulative` adds each position's value to the values before it, in the order
the axis draws its positions: left to right on a number, and in the categories'
own order on a category. It adds up what the layer already measures: the column
on `y`, or what a transform before it made. Every mark draws the total as it
draws any measure. The total is the column `cumulative`, while the measure's own
column keeps each row's value, so `label(:count)` and `label(:cumulative)` print
the two.

Each group made by `color` or `group` adds up its own total. On a category axis
the total follows the order declared in the table. An `order()` in the same
plot is refused, because each total would then be drawn at a category it was
not added at, so declare the order you want with [`ordered`](@ref).

# Arguments
- `share`: `true` divides each group's running total by that group's final
  total, so every curve ends at 1: the empirical distribution function of each group. `proportion`
  cannot say this, because it divides by the whole plot's total. `false` (the
  default) adds the values themselves. Given by position or by name.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

sales = (year = [2019, 2020, 2021, 2022, 2023],
         sales = [120, 135, 128, 150, 162])
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3])

# Each year's sales added to the years before it.
r = data(sales) + line * cumulative + x(:year) + y(:sales)

# The cumulative histogram, and one distribution function for each species.
h = data(flowers) + step * bin * cumulative + x(:sepal_length)
e = data(flowers) + step * bin * cumulative(share = true) + x(:sepal_length) +
    color(:species)
```
"""
const cumulative = Atom(:transform, Dict{Symbol,Any}(:transform => "cumulative"),
    function (args...; share = nothing)
        share = one_of(args, share, "cumulative", "share")
        if share !== nothing && !(share isa Bool)
            throw(GogError(
                "gog: `cumulative(share = )` is true or false — true ends every group's " *
                "total at 1, false adds the values themselves."))
        end
        Atom(:transform, Dict{Symbol,Any}(:transform => "cumulative", :share => share))
    end)

"""
    partition(levels...; cross = false)

Divide a whole among nested parts: one ring for each level of a hierarchy.

Give the hierarchy as columns, outermost first. One row of the table is one
leaf, and `partition(:group, :item, :detail)` says which columns spell the path
down to it. A missing value (`missing` or `""`) ends that branch early, so the
branches of a real hierarchy can end at different rings.

`zone * partition(...)` on its own is the icicle, and the same sentence with
`+ polar()` is the sunburst. `text * partition(...) + label(:name)` names each
part where it is drawn. The column on `x` gives each leaf its amount: with
`+ x(:amount)` each leaf counts its amount, and with no `x` each leaf counts as 1.
Join `proportion` for shares. `y(:depth, limits = [0, 4])` leaves a hole in the
middle: `depth` is the column the transform writes, each part's ring number.

Every number belongs to a leaf. A part that has children cannot have a number of
its own: gog could not tell whether its arc is the children's total or its own
number beside them, so it refuses.

# Arguments
- `levels...`: the hierarchy's columns, as symbols, outermost first. At least
  one.
- `cross`: `true` crosses the levels instead of nesting them down one axis. The
  first level divides the width, and the second divides the height within each
  of those columns. That is the mosaic. Both directions are then used for the
  hierarchy, so only the leaves are drawn. `false` (the default) nests them.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
titanic = (travel_class = ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
           survived = ["No", "No", "No", "No", "Yes", "Yes", "Yes", "Yes"],
           people = [122, 167, 528, 673, 203, 118, 178, 212])

# Rings: the people aboard the Titanic by class, and within each class by
# survival.
r = data(titanic) + zone * partition(:travel_class, :survived) + x(:people) +
    color(:travel_class) + polar()

# A mosaic: each class is as wide as its number of people, and split by survival.
m = data(titanic) + zone * partition(:travel_class, :survived, cross = true) +
    x(:people) + color(:survived) + style(border_color = "white", border_size = 2)
```
"""
function partition(levels...; cross = false)
    if isempty(levels)
        throw(GogError(
            "gog: `partition()` needs the hierarchy's columns, outermost first — " *
            "`partition(:group, :item, :detail)` puts `group` on the innermost " *
            "ring and `detail` on the rim."))
    end
    # Checked here rather than declared `::Bool`: a declared type turns a word
    # away before this body runs, with Julia's own `TypeError` and no direction,
    # where the other three bindings say what the setting means.
    cross isa Bool || throw(GogError(
        "gog: `partition(cross = )` is true or false — true crosses the levels " *
        "(the mosaic: the first divides the width, the second the height within " *
        "each column), false nests them down one axis (the icicle, and the " *
        "sunburst in `polar()`)."))
    fields = Dict{Symbol,Any}(
        :transform => "partition",
        :levels => [column_name(l, "partition") for l in levels])
    # Sent only when true, so a nested partition's wire form is byte-identical to
    # what it was before this existed.
    cross && (fields[:cross] = true)
    Atom(:transform, fields)
end

"""
    flow(stages...; shared = false, layered = false)

Show how an amount divides across a series of stages: the flow diagram.

Give the stages as columns, in reading order. One row of the table is one path
through all of them, and `flow(:class, :sex, :survived)` runs each row from its
first stage to its last. Rows that share a path add together, and any column
`flow()` does not name is ignored.

Three marks can draw this layout. `ribbon * flow(...)` draws the bands,
`zone * flow(...)` the stacked slots (one slot for each category of a stage), and
`text * flow(...) + label(:name)` names each slot where it is drawn. The column
on `y` gives each path its amount; with no `y`, each path counts as 1. Bound to
`x` instead, the amount runs the stages from top to bottom. On the band layer,
`color()` of a stage colors every band by the category its path holds there,
and `color()` of any other categorical column splits each band by its values.
The slots take their color from `style()`.

The stage axis comes from the columns given to `flow()`, so `x()` or `y()`
carries only the count, never a stage. To reorder the stages, reorder the
arguments.

# Arguments
- `stages...`: the stage columns, as symbols, in reading order. At least two,
  since a flow goes from one stage to another.
- `shared`: `true` when two columns name one set of places, the place each amount
  leaves and the place it arrives at. `flow(:exporter, :importer, shared = true)`
  gives each place one slot, whichever column names it, and lays every slot on
  one axis. On its own, that is the arc diagram; in `polar()` the amount goes
  around the circle, and it is the chord diagram. Each slot has two parts with no
  line between them. The first part, in the slot's color, holds the bands that
  leave the place; the second, a shade darker, holds the bands that arrive at
  it. The panel is white and has no frame unless `theme()` says otherwise,
  since the plot has no numbers to read. `false` (the default) runs the columns
  as stages side by side.
- `layered`: `true` when each row is a link from one place to another, and gog
  puts the places in layers computed from the links: the Sankey diagram.
  `flow(:source, :target, layered = true)` puts each place in the first layer
  after every place that sends to it. A band may skip a layer, and a flow may
  stop before the last layer. Links that form a cycle have no layers and are
  refused. Not together with `shared = true`. `false` is the default.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
titanic = (travel_class = ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
           survived = ["No", "No", "No", "No", "Yes", "Yes", "Yes", "Yes"],
           people = [122, 167, 528, 673, 203, 118, 178, 212])

# The people aboard the Titanic, from class to survival. Each band is as wide
# as its number of people.
p = data(titanic) + ribbon * flow(:travel_class, :survived) + y(:people)

# The bands, the slots, and the name of each slot.
q = data(titanic) + y(:people) +
    ribbon * flow(:travel_class, :survived) + color(:travel_class) +
    zone * flow(:travel_class, :survived) +
    text * flow(:travel_class, :survived) + label(:name)
```
"""
function flow(stages...; shared = false, layered = false)
    if length(stages) < 2
        throw(GogError(
            "gog: `flow()` needs at least two stage columns, in reading order — " *
            "`flow(:class, :sex, :survived)` runs each row from its `:class` " *
            "to its `:survived`. One column has no between."))
    end
    # Checked here rather than declared `::Bool`, for the reason `partition`'s
    # `cross` is.
    shared isa Bool || throw(GogError(
        "gog: `flow(shared = )` is true or false — true lays two columns that name " *
        "one set of places on one axis, `flow(:exporter, :importer, shared = true)`; " *
        "false runs the columns as stages side by side."))
    layered isa Bool || throw(GogError(
        "gog: `flow(layered = )` is true or false — true reads each row as a link " *
        "and stands the places in layers worked out from the links, " *
        "`flow(:source, :target, layered = true)`; false runs the columns as " *
        "stages side by side."))
    fields = Dict{Symbol,Any}(
        :transform => "flow",
        :stages => [column_name(st, "flow") for st in stages])
    # Sent only when true, as `partition`'s `cross` is, so a stage flow's wire
    # form is the bytes it always was.
    shared && (fields[:shared] = true)
    layered && (fields[:layered] = true)
    Atom(:transform, fields)
end

"""
    layout(from, to)

Place the nodes of a network, computed from a table of links.

The two columns name the two ends of each link, and one row of the table is one
link. gog makes one node for every name in either column. Each node has a `name`
and a `degree`, the number of rows that name it.
The engine computes every position, the same way in every language, so one
table gives one picture wherever it is drawn.

Three marks can draw this placement, inside [`network`](@ref).
`edge * layout(:a, :b)` draws the links, `point * layout(...)` the nodes, and
`text * layout(...) + label(:name)` names them. On the node layers,
`size(:degree)` and `color(:degree)` read each node's number of links.

The positions mean nothing as quantities: the picture could be mirrored without
changing what the graph says. So nothing binds to `x`, `y` or `z`. To draw in
three dimensions, give the space an angle: `network(turn = 30, tilt = 25)`.

# Arguments
- `from`: the column that names the first end of each link, as a symbol.
- `to`: the column that names the second end of each link, as a symbol.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
links = (sender = ["Ann", "Ann", "Bob", "Cy", "Dee"],
         receiver = ["Bob", "Cy", "Cy", "Dee", "Ann"])

# Who sends work to whom: the links, the people as points, and their names.
p = data(links) +
    edge * layout(:sender, :receiver) +
    point * layout(:sender, :receiver) +
    text * layout(:sender, :receiver) * repel + label(:name) +
    network()
```
"""
function layout(from, to)
    Atom(:transform, Dict{Symbol,Any}(
        :transform => "layout",
        :from => column_name(from, "layout"),
        :to => column_name(to, "layout")))
end

"""
    cluster(value; over)

Join the closest leaves, one pair at a time: the cluster tree.

The leaves are the categories of the position you give a column, such as
`y(:car)`, and each one is described by a profile: its `value` at every level of
`over`. The engine measures the distance between every pair of profiles, joins
the closest two, and keeps joining until one tree holds them all. Leave the
other position empty: it shows the distance at which branches join, and gog
names that axis Distance.

Two marks can draw this tree. `path * cluster(:amount, over = :nutrient) +
x(:food)` draws it: each join is drawn at the height where its two branches
merged, so a reader sees which leaves are close and where the groups separate.
`y(:food)` lays the same tree on its side. `zone * cluster(over = :nutrient)`
draws the tile plot as before, but orders the leaf axis by the tree, so similar
rows are next to each other. On a tile plot the values are already on `color`,
which is why `value` is left out there.

Placed beside other plots with `|` and `/`, a clustered panel decides the order
of any axis it shares. This is how a tree is drawn beside a tile plot in the
same order.

The statistic is fixed: the distances are Euclidean on the values as given, and
branches join at their average distance. The distances use the values' own
units; to remove that effect, rescale the column in the table first.

# Arguments
- `value`: the column of numbers the distances are measured on, as a symbol.
  Leave it out on a tile plot, where `color()` already names the values.
- `over`: the profile column, as a symbol: each leaf holds one value at every
  level of it. Leave it out only when the table has one row for each leaf.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
import Statistics

cars = (car = ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
               "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
        wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
        hp = [110, 110, 93, 110, 175, 105, 245, 62])

# Each measure on one scale, so that no unit counts for more than another.
standard(v) = (v .- Statistics.mean(v)) ./ Statistics.std(v)

# One row for each car and measure.
long = (car = repeat(cars.car, 3),
        measure = repeat(["mpg", "hp", "wt"], inner = 8),
        value = vcat(standard(cars.mpg), standard(cars.hp), standard(cars.wt)))

# The eight cars, ordered so that similar cars are next to each other.
p = data(long) + path * cluster(:value, over = :measure) + y(:car)
```
"""
function cluster(value = nothing; over = nothing)
    fields = Dict{Symbol,Any}(:transform => "cluster")
    value === nothing || (fields[:value] = column_name(value, "cluster"))
    over === nothing || (fields[:over] = column_name(over, "cluster"))
    Atom(:transform, fields)
end

# Pre-computed bounds: `lower`/`upper` bound the measure axis, `start`/`end` the
# domain. Every argument names a column.
#
# `end` is reserved syntax in Julia, so the domain's far edge is written
# `var"end" = :whatever`. That is Julia's own escape for exactly this, and using it
# keeps the argument's *name* the same in all four bindings rather than inventing a
# Julia-only synonym for it.
"""
    bounds(lower, upper; start, var"end")

Draw low and high values you computed yourself.

Use it when the table already holds the ends of each span, such as intervals
computed by a model. Every argument names a column.

`end` is a reserved word in Julia, so the argument `end` is written
`var"end" = :column`. That is Julia's own way to use a reserved word as a name,
and it keeps the argument's name the same in R, Python, Julia and JavaScript.

# Arguments
- `lower`, `upper`: the columns that hold each span's two ends on the axis that
  shows the amount. On `ribbon`, `interval`, `line` and `step`, both are needed.
  Given by position: `bounds(:lo, :hi)`.
- `start`, `var"end"`: the columns that hold the two ends on the other axis, for
  `zone` only. They give a rectangle its other two sides. Leave a pair out and a
  `zone` spans the whole panel on that axis.

Returns an [`Atom`](@ref) that `*` joins to a mark.

# Examples
```julia
# Intervals computed before the plot: the 95% confidence interval of each
# coefficient of a model fitted elsewhere.
coefs = (term = ["wt", "am", "qsec"],
         lo = [-5.37, 0.05, 0.63],
         hi = [-2.46, 5.83, 1.82])

p = data(coefs) + interval * bounds(:lo, :hi) + y(:term)

# A shaded span of years behind a line, its sides named by `start` and `end`.
sales = (year = [2019, 2020, 2021, 2022, 2023],
         sales = [120, 135, 128, 150, 162])
span = (from = [2020], to = [2021])

q = data(sales) + line + x(:year) + y(:sales) +
    data(span) + zone * bounds(start = :from, var"end" = :to)
```
"""
function bounds(args...; lower = nothing, upper = nothing, start = nothing,
                var"end" = nothing)
    finish = var"end"
    if length(args) > 2
        throw(GogError("gog: `bounds()` takes at most two positional columns, " *
                       "`lower` and `upper`."))
    end
    length(args) >= 1 && (lower === nothing ? (lower = args[1]) :
        throw(GogError("gog: `bounds()` was given `lower` twice.")))
    length(args) >= 2 && (upper === nothing ? (upper = args[2]) :
        throw(GogError("gog: `bounds()` was given `upper` twice.")))

    if lower === nothing && upper === nothing && start === nothing && finish === nothing
        throw(GogError(
            "gog: `bounds()` needs column names — `bounds(:lo, :hi)` bounds the " *
            "measure axis, and on a `zone` `bounds(start = :a, var\"end\" = :b)` " *
            "bounds the domain axis."))
    end
    Atom(:transform, Dict{Symbol,Any}(
        :transform => "bounds",
        :lower => lower === nothing ? nothing : column_name(lower, "bounds"),
        :upper => upper === nothing ? nothing : column_name(upper, "bounds"),
        :start => start === nothing ? nothing : column_name(start, "bounds"),
        :end => finish === nothing ? nothing : column_name(finish, "bounds")))
end

# ---------------------------------------------------------------------------
# Positions and coordinate spaces — always the plot's, unless a layer says so
# ---------------------------------------------------------------------------

const SCALE_NAMES = ("linear", "log", "time", "category")

function check_scale(scale)
    scale === nothing && return nothing
    scale isa AbstractString ||
        throw(GogError("gog: `scale = ` needs a single string, e.g. " *
                       "`x(:gdp, scale = \"log\")`."))
    scale in SCALE_NAMES ||
        throw(GogError("gog: `scale = \"$scale\"` is not a scale. gog has " *
                       join(["\"$n\"" for n in SCALE_NAMES], ", ") * "."))
    String(scale)
end

function check_base(base)
    base === nothing && return nothing
    (base isa Real && !(base isa Bool) && isfinite(base)) ||
        throw(GogError("gog: `base = ` needs a single number, e.g. " *
                       "`x(:bits, scale = \"log\", base = 2)`."))
    base > 1 || throw(GogError(
        "gog: `base = $base` is not a base a logarithm can have — it must be greater " *
        "than 1. Use 10 (the default), 2 for doublings, or `ℯ` for e-foldings."))
    Float64(base)
end

# The domain the channel runs over, when the data is not the authority (spec
# §10). Two numbers, either of which may be `nothing` on its own to leave that
# end to the data: `limits = (0, nothing)` pins a baseline and lets the top
# follow. Julia's `nothing` is what `json.jl` writes as JSON `null`, which is the
# engine's shape for an unstated end — so the spelling and the wire agree.
#
# `missing` is accepted for the same end, because a reader coming from R writes
# `NA` and Julia's nearest word for that is `missing`, not `nothing`.
const LIMITS_TWO_NUMBERS =
    "gog: `limits = ` needs two numbers, e.g. `x(:hour, limits = (0, 24))`. On a " *
    "date axis use dates: `(Date(2024, 1, 1), Date(2024, 12, 31))`. Use `nothing` " *
    "for an end the data should decide: `(0, nothing)`."

function check_limits(limits)
    limits === nothing && return nothing
    (limits isa Union{Tuple,AbstractVector} && length(limits) == 2) ||
        throw(GogError(LIMITS_TWO_NUMBERS))
    out = Any[]
    for e in limits
        if e === nothing || e === missing
            push!(out, nothing)
        elseif e isa Dates.DateTime
            # A domain on a temporal axis is written in dates, not epoch
            # arithmetic: `limits = (Date(2024, 1, 1), Date(2024, 12, 31))`.
            # Converted by the same function `to_wire` converts the *column* by,
            # because representation is the binding's job — and because the two
            # disagreeing silently would put the domain out by a factor of 86400.
            push!(out, Dates.datetime2unix(e))
        elseif e isa Dates.Date
            push!(out, Dates.datetime2unix(Dates.DateTime(e)))
        elseif e isa Real && !(e isa Bool) && isfinite(e)
            push!(out, Float64(e))
        else
            throw(GogError(LIMITS_TWO_NUMBERS))
        end
    end
    lo, hi = out
    if lo !== nothing && hi !== nothing && !(lo < hi)
        # `Base.min`, not `min`: inside this module the bare name is gog's
        # transform, and calling it here raises the shadowing error at the user
        # instead of the message they need. The third binding this has bitten
        # (spec §8) — Python has it too, R does not, since R's are `base::`.
        low, high = lo < hi ? (lo, hi) : (hi, lo)
        throw(GogError("gog: `limits = ($lo, $hi)` runs backwards or has no width — " *
                       "the first number is the low end. Write `($low, $high)`."))
    end
    out
end

# How many decimal places a label's numbers are printed with (spec §10): fixed
# places, so a column of labels reads as one column of figures. Checked here as a
# count; the engine says where it is drawn and refuses it on text.
# Whether a channel's numbers are written as percents (spec §10):
# `y(:proportion, percent = true)` prints 0.25 as 25%. Checked here as true or
# false; the engine says where it is drawn and refuses it beside `short`.
function check_percent(percent)
    percent === nothing && return nothing
    percent isa Bool || throw(GogError(
        "gog: `percent = ` is true or false — true writes a number as a percent, " *
        "e.g. `y(:proportion, percent = true)` prints 0.25 as 25%."))
    percent
end

# Whether a label's large numbers are written short, with K, M, B or T
# (spec §10): `label(:population, short = true)` prints 3.81B. Checked here as true
# or false; the engine says where it is drawn and refuses it on text.
function check_short(short)
    short === nothing && return nothing
    short isa Bool || throw(GogError(
        "gog: `short = ` is true or false — true writes a large number short, with " *
        "K, M, B or T, e.g. `label(:population, short = true)` prints " *
        "3811953827 as 3.81B."))
    short
end

function check_decimals(decimals)
    decimals === nothing && return nothing
    (decimals isa Real && !(decimals isa Bool) && isfinite(decimals)) ||
        throw(GogError("gog: `decimals = ` needs one number, e.g. " *
                       "`label(:life, decimals = 1)`. It is how many decimal places " *
                       "each number is printed with."))
    (isinteger(decimals) && decimals >= 0) ||
        throw(GogError("gog: `decimals = $decimals` is not a count of decimal places. " *
                       "Write a whole number from 0 up, e.g. `decimals = 1`."))
    Int(decimals)
end

# How many ticks an axis should aim for (spec §10). A *target*, not a promise:
# the count picks a step and the step is then rounded to a human number, so 8 on a
# 0..100 axis gets a step of 10 and nine ticks. Two is the floor — one tick shows a
# place but no direction — and the engine says so as well, because a binding is not
# the only way in.
function check_tick_count(tick_count)
    tick_count === nothing && return nothing
    (tick_count isa Real && !(tick_count isa Bool) && isfinite(tick_count)) ||
        throw(GogError("gog: `tick_count = ` needs one number, e.g. " *
                       "`x(:gdp, tick_count = 8)`. It is how many ticks the axis " *
                       "aims for."))
    isinteger(tick_count) ||
        throw(GogError("gog: `tick_count = $tick_count` is not a whole number — an " *
                       "axis cannot have a fraction of a tick. Try " *
                       "`tick_count = $(round(Int, tick_count))`."))
    tick_count < 2 &&
        throw(GogError("gog: `tick_count = $(Int(tick_count))` — an axis needs at " *
                       "least two ticks to show a direction as well as a place. Ask " *
                       "for 2 or more, or leave `tick_count` off for the default of 5."))
    Int(tick_count)
end

"""
`free = true` — fit this axis from each panel's own rows (spec §11).

A flag rather than a value, because the rest of the question is answered by
*where* it was written: `y(:life, free = true)` frees y, `x(...)` frees x.

Every channel takes it and forwards it, as every channel does `legend`, and the
engine decides where it means something: only a position draws an axis in each
panel, and on any other channel the engine refuses it with that direction.
Leaving it out of `color()` would answer `color(:continent, free = true)` with a
`MethodError` that names no fix. Only the shape is checked here, and its example
is a position's.
"""
function check_free(free, name::AbstractString)
    (free === nothing || free === false) && return false
    axis = name in ("x", "y", "z") ? name : "y"
    free === true || throw(GogError(
        "gog: `free = ` is true or false — it says whether this axis is fitted " *
        "per panel. Which axis is up to which binding you write it on: " *
        "`$axis(:<name>, free = true)` frees $axis."))
    true
end

"""
`legend = false` — leave this channel's legend out of the plot (spec §10).

Every channel takes it and forwards it, positions included, and the engine
decides where it means something: a legend is drawn for `color`, `size`, `shape`,
`pattern` and `opacity`, and on any other channel the engine refuses it with that
direction. Deciding here would be a rule the other three bindings get wrong, and
leaving it out of `x()` would answer `x(:gdp; legend = false)` with a
`MethodError` that names no fix. Only the shape is checked here, so the error
lands on the line that wrote it.
"""
function check_legend(legend)
    legend === nothing && return nothing
    legend isa Bool || throw(GogError(
        "gog: `legend = ` is true or false — false leaves this channel's legend out of " *
        "the plot, e.g. `color(:continent, legend = false)` when the names are " *
        "written on the plot instead. To choose the side of the plot the legends " *
        "sit on, write `theme(legend = \"bottom\")`."))
    legend
end

"""
`axis = false` — leave this position's axis out of the plot (spec §10): its tick
marks, its numbers and its name.

`legend`'s rule one guide over: every channel takes it and forwards it, and the
engine decides where it means something. Only `x`, `y` and `z` draw an axis, and on
any other channel the engine refuses it with that direction, in all four bindings at
once. Only the shape is checked here.
"""
function check_axis(axis)
    axis === nothing && return nothing
    axis isa Bool || throw(GogError(
        "gog: `axis = ` is true or false — false leaves this position's axis out of the " *
        "plot, its numbers and its name, e.g. `x(:lon, axis = false)` on a map. To " *
        "remove the gridlines as well, write `theme(grid = \"none\")`."))
    axis
end

position_atom(kind::Symbol, name::AbstractString, field, scale, base, limits,
              tick_count, free, legend = nothing, speed = nothing, axis = nothing,
              decimals = nothing, short = nothing, percent = nothing) =
    channel_atom(kind, name, field; scale, base, limits, tick_count, speed, free, legend, axis,
                 decimals, short, percent)

# Every channel takes every parameter a binding can carry, checks its shape and
# forwards it: the engine decides where one means nothing and says so in the same
# words in all four bindings, where Julia would only raise a `MethodError`.
channel_atom(kind::Symbol, name::AbstractString, field; scale = nothing, base = nothing,
             limits = nothing, tick_count = nothing, speed = nothing, free = false,
             legend = nothing, axis = nothing, decimals = nothing, short = nothing,
             percent = nothing) =
    Atom(kind, Dict{Symbol,Any}(:field => column_name(field, name),
                                :scale => check_scale(scale),
                                :base => check_base(base),
                                :limits => check_limits(limits),
                                :tick_count => check_tick_count(tick_count),
                                :speed => check_speed(speed),
                                :free => check_free(free, name),
                                :legend => check_legend(legend),
                                :axis => check_axis(axis),
                                :decimals => check_decimals(decimals),
                                :short => check_short(short),
                                :percent => check_percent(percent)))

# The argument help the channels share. Every channel takes the same keywords
# (`channel_atom`), and the engine refuses the ones that mean nothing on it, so
# each channel's help lists them all and says which it refuses. Written once
# here, so that one wording serves every channel that takes an argument.
const HELP_SCALE = """
- `scale`: how a number is placed along the scale: `"linear"` (the default) or
  `"log"`. `"time"` and `"category"` are accepted only when the column already
  holds dates or categories, and there they change nothing. To show numbers as
  categories, store them as text first, or use [`ordered`](@ref). A log scale
  keeps the data in its own units and labels them 1, 10, 100. That is not the
  same as plotting `log.(v)`, which would label them 0, 1, 2, and the reader
  would have to convert each number back."""
# `size` and `opacity` measure, so a column of categories is refused on them
# whatever its scale: "category" is not offered there.
const HELP_SCALE_MEASURE = """
- `scale`: how a number is placed along the scale: `"linear"` (the default) or
  `"log"`. `"time"` is accepted only on a column of dates, where it changes
  nothing; `"category"` is refused, since this channel needs numbers. On a log
  scale, equal ratios get equal steps (1 to 10 is the same step as 10 to 100),
  and the legend still shows the column's own values."""
# `color` takes categories as well as numbers, but has a legend, not an axis.
const HELP_SCALE_COLOR = """
- `scale`: how a number is placed along the scale: `"linear"` (the default) or
  `"log"`. `"time"` and `"category"` are accepted only when the column already
  holds dates or categories, and there they change nothing. To show numbers as
  categories, store them as text first, or use [`ordered`](@ref). On a log
  scale, equal ratios get equal steps of color (1 to 10 is the same step as 10
  to 100), and the legend still shows the column's own values."""
const HELP_BASE = """
- `base`: the base of a `"log"` scale, a number greater than 1; 10 when left
  out. Use 2 when each step doubles, and `ℯ` (or `exp(1)`) for the natural
  logarithm. The base does not move the data. It changes only where the
  gridlines fall and how the ticks are labeled."""
const HELP_BASE_LEGEND = """
- `base`: the base of a `"log"` scale, a number greater than 1; 10 when left
  out. Use 2 when each step doubles, and `ℯ` (or `exp(1)`) for the natural
  logarithm. The base does not change the picture: the scale is fitted to its
  own range, and the legend shows the same values whatever the base."""
const HELP_NO_SCALE = """
- `scale`, `base`, `limits`: not taken. This channel tells categories apart
  rather than measuring them, so there is no scale or range along it. gog
  refuses each of them when the plot is drawn."""
const HELP_NO_TICKS = """
- `tick_count`: not taken. Only a position, `x`, `y` or `z`, has an axis with
  ticks to count. gog refuses it when the plot is drawn."""
const HELP_NO_SPEED = """
- `speed`: not taken. Only `play` shows rows over time, so there is no speed to
  set here. gog refuses it when the plot is drawn, and says to put it on
  `play()`."""
const HELP_NO_FREE = """
- `free`: not taken. `free` fits an axis to each panel's own rows, and only a
  position, `x`, `y` or `z`, has an axis. gog refuses it when the plot is drawn,
  and says to write it on a position."""
const HELP_LEGEND = """
- `legend`: `false` leaves this channel's legend out of the plot. The column is
  still mapped; only its legend is not drawn. Use it when the plot already shows
  the same thing, such as names written at the ends of the lines."""
const HELP_NO_AXIS = """
- `axis`: not taken. Only a position, `x`, `y` or `z`, draws an axis. gog
  refuses it when the plot is drawn, and names the three positions."""
const HELP_LEGEND_NUMBERS = """
- `decimals`: how many decimal places each number in the legend is printed
  with, a whole number from 0 to 10.
- `short`: `true` writes each number in the legend short, with K, M, B or T, as
  `label()` does.
- `percent`: `true` writes each number in the legend as a percent: 0.25 is 25%.
  Not together with `short`."""
const HELP_NO_NUMBERS = """
- `decimals`, `short`, `percent`: not taken. This channel tells categories
  apart and prints no numbers. gog refuses each of them when the plot is
  drawn."""
const HELP_CHANNEL_RETURNS = """
Returns an [`Atom`](@ref), which `+` adds to a plot. Written before the first
mark, a channel belongs to the whole plot and reaches every layer whose mark can
take it. Written after a mark, it belongs to that mark alone."""
const HELP_POSITION_RETURNS = """
Returns an [`Atom`](@ref), which `+` adds to a plot. Written before the first
mark, a position belongs to every layer. Written after a mark, it names the
column that mark reads, and a layer that names no column of its own reads the
first one written for that axis."""

# The three positions take the same arguments and mean the same by them.
position_help(name::AbstractString) = """
# Arguments
- `field`: the column to bind, as a symbol: `$name(:wt)`. A string here is
  refused, because a string is a value and a channel takes a column. Leaving it
  out is refused too, with a message that says to name the column.
$HELP_SCALE
$HELP_BASE
- `limits`: the two ends of the axis, as a vector `[low, high]` or a tuple
  `(low, high)`. `nothing` or `missing` at either end leaves that end fitted to
  the data, as in `[0, nothing]`. On a date axis, give dates:
  `[Date(2024, 1, 1), Date(2024, 12, 31)]`.
- `tick_count`: about how many ticks the axis draws, a whole number of 2 or
  more; 5 when left out. The axis may draw a few more or fewer, because it
  rounds the tick values to readable numbers.
- `free`: when the plot is faceted, `true` lets each panel fit this axis to its
  own rows instead of sharing one scale across all of them. `false` by default.
- `legend`: not taken. A position is read off its axis rather than from a
  legend. gog refuses it when the plot is drawn, and names the five channels
  that draw a legend: `color`, `size`, `shape`, `pattern` and `opacity`.
- `axis`: `false` leaves this axis out of the plot: its tick marks, its numbers
  and its name, and the space they took. The scale is unchanged, so the
  gridlines and the frame stay; `theme(grid = "none")` and
  `theme(frame = "none")` remove those.
- `decimals`: how many decimal places each number on the axis is printed with,
  a whole number from 0 to 10. Large numbers keep the letter the axis already
  uses (K, M, B or T). If rounding would print two ticks as the same number, the
  axis draws fewer ticks.
- `short`: `true` writes each number on the axis short, with its own K, M, B or
  T. With it, an axis that reads 1000K, 1500K reads 1M, 1.5M.
- `percent`: `true` writes each number on the axis as a percent: 0.25 is 25%.
  `limits` stays in the column's own units. Not together with `short`.
$HELP_NO_SPEED

$HELP_POSITION_RETURNS"""

"""
    x(field; scale, base, limits, tick_count, free, legend, axis,
      decimals, short, percent, speed)

Bind the x axis to a column.

$(position_help("x"))

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
        hp = [110, 110, 93, 110, 175, 105, 245, 62])

p = data(cars) + point + x(:wt) + y(:mpg)

# A logarithmic axis, in powers of 10 or of 2.
q = data(cars) + point + x(:hp, scale = "log") + y(:mpg)
r = data(cars) + point + x(:hp, scale = "log", base = 2) + y(:mpg)
```
"""
x(field = nothing; scale = nothing, base = nothing, limits = nothing, tick_count = nothing,
  speed = nothing, free = false, legend = nothing, axis = nothing, decimals = nothing,
  short = nothing, percent = nothing) =
    position_atom(:coord_x, "x", field, scale, base, limits, tick_count, free, legend, speed,
                  axis, decimals, short, percent)

"""
    y(field; scale, base, limits, tick_count, free, legend, axis,
      decimals, short, percent, speed)

Bind the y axis to a column.

$(position_help("y"))

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
        hp = [110, 110, 93, 110, 175, 105, 245, 62])

p = data(cars) + point + x(:wt) + y(:mpg)

# A logarithmic axis.
q = data(cars) + point + x(:wt) + y(:hp, scale = "log")

# An axis that starts at zero.
r = data(cars) + point + x(:wt) + y(:mpg, limits = [0, 40])
```
"""
y(field = nothing; scale = nothing, base = nothing, limits = nothing, tick_count = nothing,
  speed = nothing, free = false, legend = nothing, axis = nothing, decimals = nothing,
  short = nothing, percent = nothing) =
    position_atom(:coord_y, "y", field, scale, base, limits, tick_count, free, legend, speed,
                  axis, decimals, short, percent)

"""
    z(field; scale, base, limits, tick_count, free, legend, axis,
      decimals, short, percent, speed)

Bind the z axis to a column.

A third position puts the plot in a cube: one more channel, not a chart type. On
a web page, or in a notebook, the reader can turn the cube with the pointer.
[`space`](@ref) sets the angle it is viewed from.

$(position_help("z"))

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
           sepal_width = [3.5, 3, 3.2, 3.1, 3.2, 3.2, 3.1, 2.3, 3.3, 2.7, 3, 2.9],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# Three measurements of each flower, in a cube.
p = data(flowers) + point + x(:sepal_length) + y(:sepal_width) +
    z(:petal_length) + color(:species)
```
"""
z(field = nothing; scale = nothing, base = nothing, limits = nothing, tick_count = nothing,
  speed = nothing, free = false, legend = nothing, axis = nothing, decimals = nothing,
  short = nothing, percent = nothing) =
    position_atom(:coord_z, "z", field, scale, base, limits, tick_count, free, legend, speed,
                  axis, decimals, short, percent)

# The four atoms that take an angle, each with its own call to show. JavaScript
# has carried these examples since it was written and the other three did not,
# which is four copies of one rule drifting rather than four idioms.
const DEGREE_EXAMPLES = Dict("space" => "space(45, 20)", "polar" => "polar(90)",
                             "globe" => "globe(178, -18)", "network" => "network(30, 25)")

degrees(value, atom::AbstractString, name::AbstractString) = begin
    (value isa Real && !(value isa Bool) && isfinite(value)) ||
        throw(GogError("gog: `$atom($name = )` needs a single number of degrees, " *
                       "e.g. `$(DEGREE_EXAMPLES[atom])`."))
    Float64(value)
end

"""
    space
    space(turn, tilt)

View a plot in three dimensions from an angle.

Binding [`z`](@ref) is what puts a plot in a cube. `space()` sets the angle that
cube is viewed from. The angle belongs to the space, as a start angle belongs to
[`polar`](@ref): it maps no column, so it is not a channel. Left out, a plot
with `z` takes a view from above and to one side that shows all three axes. On a web page,
or in a notebook, the reader can turn the cube with the pointer.

# Arguments
- `turn`: degrees the scene is turned around the vertical axis, which chooses
  the side you view the cube from; 30 when left out.
- `tilt`: degrees the eye is lifted above the floor, which chooses how steeply
  you look down; 25 when left out.

Both may be given by position, `space(60, 10)`, or by name. Bare `space` is the
default view.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
           sepal_width = [3.5, 3, 3.2, 3.1, 3.2, 3.2, 3.1, 2.3, 3.3, 2.7, 3, 2.9],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# Three measurements of each flower, in a cube turned 60 degrees.
p = data(flowers) + point + x(:sepal_length) + y(:sepal_width) +
    z(:petal_length) + color(:species) + space(turn = 60)
```
"""
const space = Atom(:coord_space, Dict{Symbol,Any}(:turn => 30.0, :tilt => 25.0),
    function (args...; turn = nothing, tilt = nothing)
        length(args) >= 1 && turn === nothing && (turn = args[1])
        length(args) >= 2 && tilt === nothing && (tilt = args[2])
        length(args) > 2 && throw(GogError("gog: `space()` takes `turn` and `tilt`."))
        Atom(:coord_space, Dict{Symbol,Any}(
            :turn => turn === nothing ? 30.0 : degrees(turn, "space", "turn"),
            :tilt => tilt === nothing ? 25.0 : degrees(tilt, "space", "tilt")))
    end)

"""
    polar
    polar(start)

Bend the plane into a circle: `x` becomes the angle and `y` the radius.

The axis that holds a bar chart's categories becomes the circle of a rose
chart, whose bars spread out from the center. Nothing else about the sentence
changes: `bar * count + x(:month)` draws a bar chart on its own and a rose chart
with `polar()`, from the same words.

The angle axis goes around once over the whole range of `x`, so the circle
closes with no gap. Angles run clockwise from twelve o'clock.

# Arguments
- `start`: degrees clockwise from the top where the circle begins; 0 when left
  out, which starts the axis at twelve o'clock. With eight categories, `-22.5`
  puts the middle of the first one at twelve o'clock, not its edge. Given by position or by
  name.

Bare `polar` is the same as `polar()`.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

cars = (cylinders = ["6", "6", "4", "6", "8", "6", "8", "4"],)

# The number of cars with each number of cylinders, bent into a circle.
p = data(cars) + bar * count + x(:cylinders) + polar()
```
"""
const polar = Atom(:coord_polar, Dict{Symbol,Any}(:start => 0.0),
    function (args...; start = nothing)
        start = one_of(args, start, "polar", "start")
        Atom(:coord_polar, Dict{Symbol,Any}(
            :start => start === nothing ? 0.0 : degrees(start, "polar", "start")))
    end)

"""
    nest
    nest()

Pack the panel with nested regions: the treemap.

Each row's measure becomes an area rather than a length, and the areas divide the
panel: a region's share of the picture is its share of the total. The mark, the
statistic and the channels stay the same, which is what makes this a space and
not a chart type. `bar * sum * stack + y(:revenue) + color(:region)` is a stacked
bar on its own, and a pie with `polar()`. Here the same sentence without `stack`
is a treemap: the packing gives every region its own place, so `stack` has
nothing to do and is refused.

A packed panel has no axes: its two directions carry no column, and two
neighboring regions are not near each other in the data. The color legend
decodes the regions. Bind a position to split the packing one level further:
each category gets a region of its own, and its rows are packed inside it.

`dodge`, `stack` and `jitter` are refused here. They decide what happens when
two marks land in the same place, and in a packing no two marks do.

`nest` takes no arguments, because a packing has no view to set. Bare `nest` is
the same as `nest()`.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

titanic = (travel_class = ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
           people = [122, 167, 528, 673, 203, 118, 178, 212])

# Each class aboard the Titanic as an area in proportion to its people.
p = data(titanic) + bar * sum + y(:people) + color(:travel_class) + nest()
```
"""
const nest = Atom(:coord_nest, Dict{Symbol,Any}(),
    function (args...)
        isempty(args) || throw(GogError(
            "gog: `nest()` takes no arguments — a packing has no view to set."))
        Atom(:coord_nest, Dict{Symbol,Any}())
    end)

"""
    globe
    globe(turn, tilt)

View the sphere itself: `x` is longitude and `y` is latitude, both in degrees.

Each mark is drawn at its place on the sphere's surface. The marks that draw on a
[`map`](@ref) draw here. `point` is a place, `path` is a route that bends along
great circles, and `text` is a name at a place. `rule` draws a line at one
coordinate (a meridian from pole to pole, or a parallel all the way around), and
`zone` with `group()` fills each region of a boundary. `bar` with `z()` also
draws here, as a spike at each place. The lines of longitude and latitude are
the panel grid, turned off like any other with `theme(grid = "none")`.

A globe shows one half of the earth. Rows on the far half are hidden behind the
sphere, and gog says how many rather than dropping them in silence. On a web
page, or in a notebook, the reader can turn the globe with the pointer.

Both positions are used for the place. A `bar` can still measure along the
radius with `z()`, one spike for each row; any other amount goes on `size()` or
`color()`. A binned field on the sphere is not drawn yet: its correct tiling is
hexagonal, because rectangular bins do not cover equal areas on a sphere, and
that grid is not built.

# Arguments
- `turn`: degrees of longitude the view faces; 0 when left out. Any number is
  accepted: 538 faces the same place as 178.
- `tilt`: degrees of latitude the view faces, from -90 to 90; 0 when left out.

Both may be given by position, `globe(178, -18)`, or by name. Bare `globe` faces
longitude 0 and latitude 0.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
quakes = (lat = [-20.42, -20.62, -26, -17.97, -20.42, -19.68, -11.7, -28.11],
          long = [181.62, 181.03, 184.1, 181.66, 181.96, 184.31, 166.1, 181.93])

# Earthquakes near Fiji, on a globe turned to face them.
p = data(quakes) + point + x(:long) + y(:lat) + globe(turn = 178, tilt = -18)
```
"""
const globe = Atom(:coord_globe, Dict{Symbol,Any}(:turn => 0.0, :tilt => 0.0),
    function (args...; turn = nothing, tilt = nothing)
        length(args) >= 1 && turn === nothing && (turn = args[1])
        length(args) >= 2 && tilt === nothing && (tilt = args[2])
        length(args) > 2 && throw(GogError("gog: `globe()` takes `turn` and `tilt`."))
        Atom(:coord_globe, Dict{Symbol,Any}(
            :turn => turn === nothing ? 0.0 : degrees(turn, "globe", "turn"),
            :tilt => tilt === nothing ? 0.0 : degrees(tilt, "globe", "tilt")))
    end)

"""
    network
    network(turn, tilt)

The network space, where the nodes that [`layout`](@ref) places are drawn.

A layout's positions belong to the graph, not to the data, so this space draws
no axes, no ticks and no grid: there is nothing there for them to number. It is
the one space a `layout` draws in, and a `layout` is the only thing drawn in it.

Giving a viewing angle draws the network in three dimensions. Bare `network()`
draws the graph flat. `network(turn = 30, tilt = 25)` computes the placement in
a cube and draws it from that angle, and an angle left out takes the standard
view. On a web page, or in a notebook, the reader can turn the cube with the
pointer.

# Arguments
- `turn`: degrees the scene is turned around the vertical axis. Give it only for
  a 3-D network.
- `tilt`: degrees the eye is lifted. Give it only for a 3-D network.

Both may be given by position, `network(30, 25)`, or by name. Bare `network`
is the same as `network()`.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
links = (sender = ["Ann", "Ann", "Bob", "Cy", "Dee"],
         receiver = ["Bob", "Cy", "Cy", "Dee", "Ann"])

p = data(links) + edge * layout(:sender, :receiver) +
    point * layout(:sender, :receiver) + network()

# The same network, turned and tilted to give it depth.
q = data(links) + edge * layout(:sender, :receiver) +
    point * layout(:sender, :receiver) + network(turn = 35, tilt = 20)
```
"""
const network = Atom(:coord_network, Dict{Symbol,Any}(),
    function (args...; turn = nothing, tilt = nothing)
        length(args) >= 1 && turn === nothing && (turn = args[1])
        length(args) >= 2 && tilt === nothing && (tilt = args[2])
        length(args) > 2 && throw(GogError("gog: `network()` takes `turn` and `tilt`."))
        fields = Dict{Symbol,Any}()
        turn === nothing || (fields[:turn] = degrees(turn, "network", "turn"))
        tilt === nothing || (fields[:tilt] = degrees(tilt, "network", "tilt"))
        Atom(:coord_network, fields)
    end)

"""
    map
    map(preserve)

Flatten the sphere onto the page: `x` is longitude and `y` is latitude, both in
degrees.

The projection decides where each place lands. Nothing else about the sentence
changes: `point` is a place, `path` is a route through places in the table's
order, `text` is a name at a place, and `rule` spans the axis it does not name,
which on a map is a meridian or a parallel.

Both positions are used for the place, so no axis is left for a mark that
measures an amount. Carry an amount on a channel instead: `size(:column)` gives the
proportional-symbol map, and `color(:column)` shades each place.

The panel is shaped by the projection rather than by the page, so
`theme(ratio = )` is refused here: an equal-area map stretched to fit a box is
no longer equal-area.

# Arguments
- `preserve`: what the flattening keeps, since no flat map keeps both true
  areas and true angles. `"area"` (the default) draws every region with an area
  in proportion to its true size, which is what a map read by area needs; it uses
  the Equal Earth projection, which reaches both poles. `"angle"` keeps every
  small shape's true form, but areas are no longer true; it uses Mercator. On
  Mercator the poles are infinitely far away, so the map stops at 85.05 degrees
  of latitude. A row beyond that is drawn at the edge, and gog says how many
  were. Given by position or by name. Bare `map` is the same as `map()`.

Returns an [`Atom`](@ref), which `+` adds to a plot.

$(base_name_help("map"))

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

quakes = (lat = [-20.42, -20.62, -26, -17.97, -20.42, -19.68, -11.7, -28.11],
          long = [181.62, 181.03, 184.1, 181.66, 181.96, 184.31, 166.1, 181.93],
          mag = [4.8, 4.2, 5.4, 4.1, 4, 4, 4.8, 4.4])

# Earthquakes near Fiji, at their longitude and latitude.
p = data(quakes) + point + x(:long) + y(:lat) + map()

# The same places, each sized by the earthquake's magnitude.
q = data(quakes) + point + x(:long) + y(:lat) + size(:mag) + map()
```
"""
const map = Atom(:coord_map, Dict{Symbol,Any}(:preserve => "area"),
    function (args...; preserve = nothing)
        preserve = one_of(args, preserve, "map", "preserve")
        preserve = preserve === nothing ? "area" : preserve
        # Validated at the line the caller wrote, rather than at the wire. The
        # engine checks it too — a rule implemented in one binding is a rule the
        # other three get wrong — but a reader is owed the error where they typed
        # it.
        preserve isa AbstractString && preserve in ("area", "angle") || throw(GogError(
            "gog: `map(preserve = )` takes \"area\" or \"angle\". \"area\" keeps every " *
            "region's true size, which is what a map read by area needs; \"angle\" " *
            "keeps every small shape's true form and pays for it in area. " *
            "A sphere cannot do both."))
        Atom(:coord_map, Dict{Symbol,Any}(:preserve => String(preserve)))
    end)

# ---------------------------------------------------------------------------
# Channels — they map a column, and earn a legend to decode it
# ---------------------------------------------------------------------------

scaled_channel(kind::Symbol, name::AbstractString) =
    (field = nothing; kwargs...) -> channel_atom(kind, name, field; kwargs...)

# The same parameters as a scaled channel; the name says which channels the engine
# refuses a scale on, not which keywords the function takes.
plain_channel(kind::Symbol, name::AbstractString) =
    (field = nothing; kwargs...) -> channel_atom(kind, name, field; kwargs...)

"""
    color(field; scale, base, limits, tick_count, speed, free, legend, axis,
          decimals, short, percent)

Map the fill and stroke color to a column.

A column of text picks its colors from the palette, one for each category, and
[`palette`](@ref) chooses them. A column of numbers runs along a color ramp from
low to high. Either way the plot gets a legend that decodes the colors. To give
every mark of a layer one fixed color instead, write `style(color = "tomato")`.

# Arguments
- `field`: the column to bind, as a symbol: `color(:species)`. A string is
  refused, because a string is a value and a channel takes a column.
$HELP_SCALE_COLOR
$HELP_BASE_LEGEND
- `limits`: for a column of numbers, the two ends of the color scale, as a
  vector `[low, high]` or a tuple. `nothing` or `missing` at either end leaves
  that end fitted to the data.
$HELP_NO_TICKS
$HELP_NO_SPEED
$HELP_NO_FREE
$HELP_LEGEND
$HELP_NO_AXIS
$HELP_LEGEND_NUMBERS

$HELP_CHANNEL_RETURNS

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
        hp = [110, 110, 93, 110, 175, 105, 245, 62])

# A category: one color for each species, and a legend.
p = data(flowers) + point + x(:sepal_length) + y(:petal_length) + color(:species)

# A number: colors that run from low to high.
q = data(cars) + point + x(:wt) + y(:mpg) + color(:hp)
```
"""
const color = scaled_channel(:color, "color")

# The British spelling of `color()`, refused with direction.
#
# gog writes American English throughout and accepts no second spelling, which is
# Law 2 applied to the vocabulary itself: two ways to write one word is a silent
# letter, and the reader pays for it. ggplot2 accepts both, so a reader arriving
# from there types `colour` and, unexported, would meet Julia's `UndefVarError` —
# a message that names no fix. Exported for the same reason JavaScript still
# exports `facet` (spec §13).
"""
    colour(args...; kwargs...)

The British spelling of [`color`](@ref), which gog refuses.

gog writes American English throughout and accepts no second spelling of a
word. Some other plotting libraries accept both, so a reader who is used to them
may type `colour`. The name exists so that the message can say what to write:
`color(:name)`. The same is true for the settings: `style(colour = )` and
`style(border_colour = )` are refused, and the message names `color` and
`border_color`.

# Arguments
- `args...`, `kwargs...`: whatever the call was given. They are not read, since
  the call is always refused.

Returns nothing: it always throws a [`GogError`](@ref) that names `color()`.

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# gog spells it `color`, and the message says so.
try
    colour(:species)
catch err
    println(err.msg)
end

p = data(flowers) + point + x(:sepal_length) + y(:petal_length) + color(:species)
```
"""
colour(args...; kwargs...) = throw(GogError(
    "gog: there is no `colour()` channel. gog spells it `color(:name)`: " *
    "American English is the grammar's only spelling, and unlike ggplot2 " *
    "there is no British alternative."))

"""
    size(field; scale, base, limits, tick_count, speed, free, legend, axis,
         decimals, short, percent)

Map a column of numbers to the size of a point or of a text label.

To give every mark of a layer one fixed size instead, write `style(size = 6)`.

$(base_name_help("size"))

# Arguments
- `field`: the column to bind, as a symbol: `size(:hp)`.
$HELP_SCALE_MEASURE
$HELP_BASE_LEGEND
- `limits`: the two ends of the size scale, as a vector `[low, high]` or a
  tuple. `nothing` or `missing` at either end leaves that end fitted to the
  data.
$HELP_NO_TICKS
$HELP_NO_SPEED
$HELP_NO_FREE
$HELP_LEGEND
$HELP_NO_AXIS
$HELP_LEGEND_NUMBERS

$HELP_CHANNEL_RETURNS

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
        hp = [110, 110, 93, 110, 175, 105, 245, 62])

# Cars with more horsepower are drawn larger.
p = data(cars) + point + x(:wt) + y(:mpg) + size(:hp)
```
"""
const size = scaled_channel(:size, "size")

"""
    opacity(field; scale, base, limits, tick_count, speed, free, legend, axis,
            decimals, short, percent)

Map the opacity of each mark to a column of numbers.

To give every mark of a layer one fixed opacity instead, write
`style(opacity = 0.5)`.

# Arguments
- `field`: the column to bind, as a symbol: `opacity(:hp)`.
$HELP_SCALE_MEASURE
$HELP_BASE_LEGEND
- `limits`: the two ends of the scale, as a vector `[low, high]` or a tuple.
  `nothing` or `missing` at either end leaves that end fitted to the data.
$HELP_NO_TICKS
$HELP_NO_SPEED
$HELP_NO_FREE
$HELP_LEGEND
$HELP_NO_AXIS
$HELP_LEGEND_NUMBERS

$HELP_CHANNEL_RETURNS

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
        hp = [110, 110, 93, 110, 175, 105, 245, 62])

# Cars with more horsepower are drawn more opaque.
p = data(cars) + point + x(:wt) + y(:mpg) + opacity(:hp)
```
"""
const opacity = scaled_channel(:opacity, "opacity")

"""
    group(field; scale, base, limits, tick_count, speed, free, legend, axis,
          decimals, short, percent)

Split a layer into groups without coloring them.

`group` splits the rows by a column. A `line` or a `path` then draws one stroke
for each group, `stack` or `dodge` has groups to place, and a transform such as
`mean` runs within each group. [`color`](@ref) splits the rows the same way, but
`group` draws every group alike and adds no legend.

# Arguments
- `field`: the column to group by, as a symbol: `group(:tree)`.
$HELP_NO_SCALE
$HELP_NO_TICKS
$HELP_NO_SPEED
$HELP_NO_FREE
- `legend`: not taken. `group` splits the rows without encoding anything, so it
  draws no legend. gog refuses it when the plot is drawn, and names the five
  channels that draw one.
$HELP_NO_AXIS
$HELP_NO_NUMBERS

$HELP_CHANNEL_RETURNS

# Examples
```julia
trees = (tree = ["1", "1", "1", "1", "2", "2", "2", "2", "3", "3", "3", "3"],
         age = [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
         circumference = [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108])

# One line for each tree, all drawn in one color.
p = data(trees) + line + x(:age) + y(:circumference) + group(:tree)
```
"""
const group = plain_channel(:group, "group")

"""
    shape(field; scale, base, limits, tick_count, speed, free, legend, axis,
          decimals, short, percent)

Map a categorical column to the shape of each point.

`shape` draws each category with a different symbol: a circle, a square, a
triangle and so on. A `scale` is refused, because there is no distance between a
circle and a square to measure. To give every point of a layer one fixed shape
instead, write `style(shape = "square")`.

# Arguments
- `field`: the column to bind to the shape, as a symbol: `shape(:species)`.
$HELP_NO_SCALE
$HELP_NO_TICKS
$HELP_NO_SPEED
$HELP_NO_FREE
$HELP_LEGEND
$HELP_NO_AXIS
$HELP_NO_NUMBERS

$HELP_CHANNEL_RETURNS

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# A different shape for each species.
p = data(flowers) + point + x(:sepal_length) + y(:petal_length) + shape(:species)
```
"""
const shape = plain_channel(:shape, "shape")

"""
    pattern(field; scale, base, limits, tick_count, speed, free, legend, axis,
            decimals, short, percent)

Map the texture of each mark to a categorical column, as [`shape`](@ref)
maps the shape of each point.

`pattern` tells categories apart by texture rather than by color. On a filled
region (`bar`, `box`, `area`, `ribbon`, `zone`) each category draws as a hatch,
and on a stroke (`line`, `step`, `interval`, `path`, `rule`, `edge`) as a dash.
It separates categories without color, so they stay distinct in a gray printout
or for a color-blind reader. Using `pattern` and `color` on the same column
shows each category two ways, which is the usual advice for accessible plots. A
`scale` is refused: like `shape`, it answers "which one?", not "how much?".

This channel maps a column to several textures. The setting
`style(pattern = "hatch")` fixes one texture for the whole layer.

# Arguments
- `field`: the column to bind to the texture, as a symbol: `pattern(:species)`.
$HELP_NO_SCALE
$HELP_NO_TICKS
$HELP_NO_SPEED
$HELP_NO_FREE
$HELP_LEGEND
$HELP_NO_AXIS
$HELP_NO_NUMBERS

$HELP_CHANNEL_RETURNS

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# A different hatch for each species, so the bars differ without color.
p = data(flowers) + bar * mean + x(:species) + y(:petal_length) + pattern(:species)
```
"""
const pattern = plain_channel(:pattern, "pattern")

"""
    label(field; scale, base, limits, tick_count, speed, free, legend, axis,
          decimals, short, percent)

Draw a column's values as text: the content of the `text` mark.

`label` gives the text a `text` mark draws, as `x` and `y` give its position:
`text + x(:a) + y(:b) + label(:name)`. A column of text is drawn as it is, and a
column of numbers is formatted. `text` requires it, and no other mark accepts
it, so a scatter with labels is two layers, `point + text`. A `scale` is
refused: a label is text to write, not an amount to measure.

# Arguments
- `field`: the column whose values are drawn as the text, as a symbol:
  `label(:car)`.
- `scale`, `base`, `limits`: not taken. `label` writes each value as text rather
  than measuring it, so there is no scale or range along it. gog refuses each of
  them when the plot is drawn.
$HELP_NO_TICKS
$HELP_NO_SPEED
$HELP_NO_FREE
- `legend`: not taken. A label is read where it is written, so it draws no
  legend. gog refuses it when the plot is drawn, and names the five channels
  that draw one.
$HELP_NO_AXIS
- `decimals`: how many decimal places each number is printed with, a whole
  number from 0 to 10: `label(:life, decimals = 1)` prints 70.72848 as 70.7. The
  places are fixed, so 77 prints as 77.0, and all the labels show the same
  number of decimal places. A column of text has nothing to round.
- `short`: `true` writes a large number short, with K for thousands, M for
  millions, B for billions and T for trillions:
  `label(:population, short = true)` prints 3811953827 as 3.81B. It keeps three
  significant digits unless `decimals` gives the places after the point, so
  `decimals = 1` prints 3.8B. A column of text has no number to write short.
- `percent`: `true` prints each number as a percent:
  `label(:proportion, percent = true)` prints 0.25 as 25%. With `decimals` it
  gives the places after the point. Not together with `short`.

$HELP_CHANNEL_RETURNS

# Examples
```julia
cars = (car = ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
               "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
        wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

# Each car's name, written at its weight and mileage.
p = data(cars) + text + x(:wt) + y(:mpg) + label(:car)

# The mileage itself, with one decimal place, above each point.
q = data(cars) + point + text + x(:wt) + y(:mpg) + label(:mpg, decimals = 1) +
    style(nudge = "up")
```
"""
const label = plain_channel(:label, "label")

# How fast a `play` sequence runs, as a multiple of the normal pace (spec §15).
# The narrowest of the four binding parameters: `limits` needs a domain,
# `tick_count` needs an axis, and this needs a duration — which only `play` has.
function check_speed(speed)
    speed === nothing && return nothing
    (speed isa Real && !(speed isa Bool) && isfinite(speed)) ||
        throw(GogError("gog: `speed = ` needs a single number, e.g. " *
                       "`play(:year, speed = 2)`. It is how many times faster " *
                       "than normal the frames run."))
    speed > 0 || throw(GogError(
        "gog: `speed = $speed` — a speed is a multiple of the normal pace, so it has " *
        "to be above zero. `speed = 2` is twice as fast, `speed = 0.5` half."))
    Float64(speed)
end

"""
    play(field; speed, scale, base, limits, tick_count, free, legend, axis,
         decimals, short, percent)

Cut the plot into frames and play them, one after another: the time dimension.

`play` works like `facet`, but in time. Both split the rows by the values of a
column. `| facet(:continent)` shows the pieces side by side; `play(:year)` shows
them one after another, as frames.

Every scale, the color map and every legend are fitted across the whole
sequence, never frame by frame. A scale fitted again for each frame would move
the axis under the data, and a still point would look as if it moved. A layer
that does not bind `play` is drawn in every frame, so a reference line stays
still behind the marks that move.

Unlike `facet`, `play` accepts a column of numbers. A hundred panels are too
small to read; a hundred frames only make a longer animation. Frames run in
order, so a column of text needs a declared order, made with [`ordered`](@ref);
without one, gog plays the frames in the order the values first appear and says
so.

A plot that plays moves in a web browser and in a notebook. Where an SVG
converter makes a still picture of it, the first frame is what it shows.
[`save_png`](@ref) refuses a plot that plays more than one frame;
[`save_gif`](@ref) writes every frame.

# Arguments
- `field`: the column whose distinct values become the frames, as a symbol:
  `play(:year)`.
- `speed`: how many times faster than normal the frames run, one positive
  number: `2` is twice as fast, `0.5` half.
- `scale`, `base`, `limits`: not taken. `play` shows each value as a frame of
  its own rather than measuring it, so there is no scale or range along it. gog
  refuses each of them when the plot is drawn.
$HELP_NO_TICKS
$HELP_NO_FREE
- `legend`: not taken. Each frame is named in the strip above the panel while
  it is shown, so `play` draws no legend. gog refuses it when the plot is drawn,
  and names the five channels that draw one.
$HELP_NO_AXIS
- `decimals`, `short`, `percent`: not taken yet. The strip above the panel
  writes each frame's name in its own way. gog refuses each of them when the
  plot is drawn.

$HELP_CHANNEL_RETURNS

# Examples
```julia
trees = (tree = ordered(["1", "1", "1", "1", "2", "2", "2", "2", "3", "3", "3", "3"],
                        ["1", "2", "3"]),
         age = [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
         circumference = [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108])

# One frame for each tree. On a web page, the plot shows them in turn.
p = data(trees) + point + x(:age) + y(:circumference) + play(:tree)

# Twice as fast.
q = data(trees) + point + x(:age) + y(:circumference) + play(:tree, speed = 2)
```
"""
play(field = nothing; kwargs...) = channel_atom(:play, "play", field; kwargs...)

# What `at` was given, and which of the two readings it is. One keyword rather
# than two, because the *value* answers the question the way a column answers it
# everywhere else in this grammar: numbers are a range, names are a set of slots.
function check_brush_at(at)
    at === nothing && return Dict{Symbol,Any}()
    seq = at isa AbstractString || at isa Symbol ? [at] : collect(at)
    if !isempty(seq) && all(v -> v isa AbstractString || v isa Symbol, seq)
        return Dict{Symbol,Any}(:levels => String[String(v) for v in seq])
    end
    if length(seq) != 2 || !all(v -> v isa Real && isfinite(v), seq)
        throw(GogError("gog: `at` is where the selection opens: two numbers on a " *
                       "column that measures, e.g. `brush(:gdp, at = (1200, 45000))`, " *
                       "or the names to select on a column of categories."))
    end
    Dict{Symbol,Any}(:at => Float64[Float64(seq[1]), Float64(seq[2])])
end

"""
    brush
    brush(field; at)

Let the reader select rows, and dim the rest.

`brush` selects rows by the values of one column. Selected rows keep the plot's
colors, and the other rows are dimmed, so you can compare the selected rows with
all the others. Where the page can run the engine, as on a web page or in a
notebook, the reader can drag the selection. In a still picture, such as a PNG,
it stays where the sentence put it.

Bare `brush` means the positions the plot binds: the reader can drag a region
over `x` and `y` together, and nothing is selected until they do.

A brush highlights. It never removes rows. Removing rows before the statistics
run is what `limits` does, on a position, and it counts what it dropped. Change
a range with `limits` and a histogram bins the rows that are left, while a brush
keeps every row in the picture and draws the selected ones in full color.

One column for each `brush`. Write two for a rectangle:
`brush(:gdp, at = (1200, 45000)) + brush(:life, at = (55, 78))`.

A mark can be brushed when one row is one shape: `point`, `bar`, `text`, `rule`,
`zone`, `interval * bounds` and `ribbon * flow`. A `line` draws one shape
through many rows, and a summary such as `bar * count` draws one bar for many
rows, so neither has a single row to select. A plot of such layers alone is
refused, with the reason. Beside a layer that can be brushed, such a layer is
drawn whole, without dimming.

# Arguments
- `field`: the column the selection is made on, as a symbol: `brush(:wt)`.
- `at`: where the selection starts. On a column of numbers, two numbers, as a
  tuple or a vector: `at = (20, 30)`. On a categorical column, the names to
  select, as strings or symbols: `at = ["4", "6"]`. Left out, nothing is
  selected, and the plot draws as it would with no `brush` at all until a reader
  drags.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
        cylinders = ["6", "6", "4", "6", "8", "6", "8", "4"])

# On a web page, a drag across the plot selects cars by weight.
p = data(cars) + point + x(:wt) + y(:mpg) + brush(:wt)

# A selection written in the sentence: the cars from 20 to 30 miles per gallon.
q = data(cars) + point + x(:wt) + y(:mpg) + brush(:mpg, at = (20, 30))

# On a column of categories: the cars with 4 or 6 cylinders.
r = data(cars) + point + x(:wt) + y(:mpg) + brush(:cylinders, at = ["4", "6"])
```
"""
const brush = Atom(:brush, Dict{Symbol,Any}(:field => ""),
    function (args...; at = nothing)
        # Bare `brush` and `brush(:gdp)` are the same atom in two shapes, the
        # pattern `bin` and `box` already use: `+ brush` has to add the atom
        # rather than the function.
        field = isempty(args) ? nothing : one_of(args, nothing, "brush", "field")
        Atom(:brush, merge(Dict{Symbol,Any}(
                 :field => field === nothing ? "" : column_name(field, "brush")),
                           check_brush_at(at)))
    end)

# ---------------------------------------------------------------------------
# Settings — they fix a value, map nothing, and earn no legend (spec §7)
# ---------------------------------------------------------------------------

const STYLE_STRINGS = ["color", "shape", "border_color"]
const STYLE_NUMBERS = ["opacity", "size", "border_size", "angle", "gap"]
const STYLE_FLAGS = ["caps", "center"]
const STYLE_VALUES = Dict(
    "nudge" => ["up", "down", "left", "right"],
    "pattern" => ["solid", "dashed", "dotted", "dotdash", "longdash",
                  "hatch", "crosshatch", "stripes", "grid", "dots"],
    "arrow" => ["end", "start", "both"],
    "reach" => ["panel", "edge"])
const STYLE_PROPS = vcat(STYLE_STRINGS, STYLE_NUMBERS, STYLE_FLAGS, collect(keys(STYLE_VALUES)))
# An example the setting itself would take: `style(shape = "tomato")` was the
# example a refused shape was given. The same values in all four bindings.
const STYLE_EXAMPLES = Dict("color" => "\"tomato\"", "shape" => "\"square\"",
                            "border_color" => "\"black\"", "opacity" => "0.3",
                            "size" => "6", "border_size" => "1.5", "angle" => "90",
                            "gap" => "0.2")
# What a setting does, said after the values it takes, in the same words as the
# other three bindings.
const STYLE_MEANINGS = Dict(
    "caps" => " — `caps = false` draws a bare linerange, `caps = true` (the default) " *
              "an error bar.",
    "center" => " — `center = false` hides a confidence interval's center dot, " *
                "`center = true` (the default) draws it.",
    "nudge" => " — which way a text label sits from its point.",
    "arrow" => " — which end of a `path` carries the head.")

# The British spelling of a setting, and what gog spells it instead. One entry
# per gog word that has a British form; there are three, and `colour()` the
# channel is the fourth word in the grammar with one.
const BRITISH_SETTINGS = Dict("colour" => "color",
                              "border_colour" => "border_color",
                              "centre" => "center")

"""
    style(; props...)

Set fixed visual properties on the nearest mark written before it.

A channel maps a column: `color(:species)` gives each species its own color and
draws a legend. `style()` sets one value for the whole layer instead. It maps no
column and draws no legend. A `style()` with no mark before it is refused, and
so is a column symbol given as a value.

# Arguments
- `props...`: the settings, each given by name. A name that is not a setting is
  refused, and the message lists the settings. These are the settings:
- `color`: a color name (`"steelblue"`) or a hex code (`"#4e79a7"`).
- `opacity`: a number from 0 to 1. It is set as written: unlike the `opacity`
  channel, there is no range of data to rescale from.
- `size`: in pixels, the radius of a point (4.5 when unset) or the width of a
  line.
- `shape`: one of `"circle"`, `"square"`, `"triangle"`, `"diamond"`, `"cross"`,
  `"star"` or `"wye"` (a Y shape).
- `border_color`: the outline color of a closed shape, such as a bar or a
  point. On `text`, setting `border_color` or `border_size` draws a halo around
  each label. Its color is the panel's background unless `border_color` names
  one, and its width is 3 unless `border_size` gives one.
- `border_size`: the outline width of a closed shape. On `text`, the width of
  the halo (see `border_color`).
- `caps`: on an `interval`, `true` (the default) draws the short bars at its
  ends, an error bar; `false` leaves the line bare.
- `center`: on an `interval`, `true` (the default) draws the center dot that
  `confidence` supplies; `false` hides it.
- `nudge`: which way a `text` label moves from its point: `"up"`, `"down"`,
  `"left"` or `"right"`. The distance follows from the size of the text.
- `pattern`: the dash of a stroke (`"solid"`, `"dashed"`, `"dotted"`,
  `"dotdash"`, `"longdash"`) or the texture of a fill (`"hatch"`,
  `"crosshatch"`, `"stripes"`, `"grid"`, `"dots"`).
- `arrow`: which end of a `path` or an `edge` carries an arrowhead: `"end"`,
  `"start"` or `"both"`.
- `reach`: how far a `rule` reaches: `"panel"` (the default) runs it all the way
  across, a reference line; `"edge"` draws a short tick at the start of its
  axis; one tick for each row makes a rug.
- `angle`: degrees to turn a `text` label, counterclockwise from horizontal: `90`
  reads upward. Each label turns about the point it names.
- `gap`: the share of the space each category gives a `bar` or a `box` that is
  left empty beside it, from 0 (neighbors touch) to just under 1. On a dodged
  mark it is a share of the mark's part of that space. Unset, a bar over
  categories leaves 0.2, a binned bar 0, and a box keeps its usual width.

Returns an [`Atom`](@ref), which `+` adds to a plot after a mark.

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])
trees = (tree = ["1", "1", "1", "1", "2", "2", "2", "2", "3", "3", "3", "3"],
         age = [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
         circumference = [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108])

# One color and opacity for every point: settings, not mappings.
p = data(cars) + point + x(:wt) + y(:mpg) + style(color = "tomato", opacity = 0.5)

# A thin gray line for each tree.
q = data(trees) + line + x(:age) + y(:circumference) + group(:tree) +
    style(color = "gray", size = 1)
```
"""
function style(; props...)
    isempty(props) && throw(GogError(
        "gog: `style()` sets nothing. Name at least one property, e.g. " *
        "`style(color = \"tomato\")`."))

    clean = Dict{String,Any}()
    for (key, value) in props
        name = String(key)
        # The British spelling and the ordinary typo part on the *message* and
        # not on the check: one names the word to write, the other lists what
        # exists.
        if !(name in STYLE_PROPS)
            haskey(BRITISH_SETTINGS, name) && throw(GogError(
                "gog: `style($name = )` is not a setting. gog spells it " *
                "`$(BRITISH_SETTINGS[name])`: American English is the grammar's " *
                "only spelling, and unlike ggplot2 there is no British alternative."))
            throw(GogError(
                "gog: `style($name = )` is not a setting. gog sets: " *
                join(sort(STYLE_PROPS), ", ") * "."))
        end
        # A column where a value belongs — the mirror of a string where a column
        # belongs, and the same §7 distinction seen from the other side.
        if value isa Symbol
            throw(GogError(
                name in ("color", "size", "opacity", "shape", "pattern") ?
                "gog: `style($name = )` fixes one value for the whole layer, and " *
                "`:$value` is a column. To *map* it — one value per category, with a " *
                "legend to decode it — that is a channel: `$name(:$value)`." :
                "gog: `style($name = )` fixes one value, and `:$value` is a column."))
        end
        if name in STYLE_STRINGS && !(value isa AbstractString)
            throw(GogError("gog: `style($name = )` needs a single string, e.g. " *
                           "`style($name = $(STYLE_EXAMPLES[name]))`."))
        end
        if name in STYLE_NUMBERS && (!(value isa Real) || value isa Bool || !isfinite(value))
            throw(GogError("gog: `style($name = )` needs a single number, e.g. " *
                           "`style($name = $(STYLE_EXAMPLES[name]))`."))
        end
        if name in STYLE_FLAGS && !(value isa Bool)
            throw(GogError("gog: `style($name = )` needs true or false" *
                           get(STYLE_MEANINGS, name, ".")))
        end
        if haskey(STYLE_VALUES, name) && !(value in STYLE_VALUES[name])
            # `pattern` is the one setting whose values split by geometry: five
            # are a stroke's dash and five a fill's texture, and the engine
            # refuses each on the other. A flat list of ten hides that.
            if name == "pattern"
                throw(GogError("gog: `style(pattern = )` needs a stroke's dash " *
                               "(\"solid\", \"dashed\", \"dotted\", \"dotdash\", " *
                               "\"longdash\") or a fill's texture " *
                               "(\"hatch\", \"crosshatch\", \"stripes\", \"grid\", " *
                               "\"dots\")."))
            end
            # `reach` says what each of its two values draws, which is the
            # whole of what a reader needs to choose between them.
            name == "reach" && throw(GogError(
                "gog: `style(reach = )` needs \"panel\" (the default — a `rule` all " *
                "the way across, a reference line) or \"edge\" (a short tick at the " *
                "start of that axis, a rug)."))
            throw(GogError("gog: `style($name = )` needs one of " *
                           join(["\"$v\"" for v in STYLE_VALUES[name]], ", ") *
                           get(STYLE_MEANINGS, name, ".")))
        end
        clean[name] = name in STYLE_NUMBERS ? Float64(value) : value
    end
    Atom(:style, Dict{Symbol,Any}(:props => clean))
end

# ---------------------------------------------------------------------------
# Plot-level atoms
# ---------------------------------------------------------------------------

"""
    order(field; desc = false)

Order the categories on an axis by a column.

`order()` sorts the categories on a categorical axis by the values of a column:
`order(:gold)` puts the categories in order of `gold`. An `order()` written in
the plot wins over the order declared in the table, because
it is the more specific instruction. A plot that adds up a category axis with
`cumulative` refuses `order()`.

# Arguments
- `field`: the column to order by, as a symbol. Name a column of numbers, such
  as the one on `y`, to order the categories by its values. Name the categorical
  column itself to ask for its own order: alphabetical for plain text, and the
  declared order for a column made with [`ordered`](@ref). Leaving it out is
  refused, with a message that says which column to name.
- `desc`: `true` reverses the order, so the largest value (or the last letter)
  comes first. `false` (the default) keeps the smallest first.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
cars = (car = ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
               "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

# Bars in order of their height, lowest first.
p = data(cars) + bar + x(:car) + y(:mpg) + order(:mpg)

# Highest first.
q = data(cars) + bar + x(:car) + y(:mpg) + order(:mpg, desc = true)
```
"""
function order(field; desc = false)
    check_desc(desc)
    Atom(:order, Dict{Symbol,Any}(:field => column_name(field, "order"),
                                  :descending => desc))
end

# Untyped and checked here: a `desc::Bool` keyword met `desc = "yes"` with a
# `TypeError` that names neither the setting nor what it takes.
check_desc(desc) = desc isa Bool || throw(GogError(
    "gog: `order(desc = )` is true or false — true reverses the order, so the " *
    "largest value comes first, and false keeps it, smallest first."))

# The column is required. `order(desc = true)` alone reads as a way to run a
# category axis backward, and it never was; naming the category column itself is,
# so the refusal says that rather than Julia's `MethodError`.
function order(; desc = false)
    check_desc(desc)
    written = desc ? "order(desc = true)" : "order()"
    tail = desc ? ", desc = true" : ""
    throw(GogError(
        "gog: `$written` names no column to order by. Name the category column itself " *
        "to sort the axis by its own categories, `order(:<category>$tail)`, or a column " *
        "of values to sort them by, `order(:<value>$tail)`."))
end

# A second column is a crossing, not a count: one `facet()` for each. A count
# written by position belongs to the keyword. Without these two methods either
# call was a raw `MethodError`.
facet(field, second::Union{Symbol,AbstractString}; kw...) = throw(GogError(
    "gog: `facet()` splits the plot by one column, and `$(repr(second))` is a second " *
    "one. To cross two columns, give each its own `facet()`: " *
    "`| facet($(repr(field))) / facet($(repr(second)))`."))
facet(field, n::Integer; kw...) = throw(GogError(
    "gog: `facet()` takes the number of panels before the line turns as a keyword: " *
    "`facet($(repr(field)), wrap = $n)`."))

# `wrap` is untyped and checked here, so `wrap = "4"` gets gog's sentence rather
# than a `TypeError`, and `wrap = 4.0` means 4, as it does in R and JavaScript.
"""
    facet(field; wrap = nothing)

Name the column that splits the plot into panels: small multiples.

`facet()` is joined to a plot with `|` or `/`, not with `+`:

    plot | facet(:continent)                one panel for each continent, side by side
    plot / facet(:era)                      one panel for each era, one above the other
    plot | facet(:continent) / facet(:era)  two columns crossed into a grid

The panels share one scale, which is what makes them comparable, and a
transform in the plot runs within each panel's rows. The facet column must hold
categories: a number is a position along an axis, not a name for a panel.

`wrap` breaks a long line of panels into several lines. Twelve countries side by
side make twelve narrow panels, and `wrap = 4` makes them a grid of 4 by 3. The
number is how many panels go in one line before the next line starts. The
operator chooses the direction: `| facet(:g, wrap = 4)` puts four in a row, and
`/ facet(:g, wrap = 4)` four in a column. Wrapping a crossed facet is refused,
since a crossing already fills a rectangle.

# Arguments
- `field`: the column whose categories become the panels, as a symbol:
  `facet(:species)`.
- `wrap`: how many panels go in one line before the next line starts, one
  whole number. Left out (`nothing`), the panels stay in one line.
- `second`, `n`: not taken. `facet(:a, :b)` is refused, because one `facet()`
  takes one column: cross two columns with one `facet()` each, as in
  `plot | facet(:a) / facet(:b)`. `facet(:a, 4)` is refused too, and the
  message says to write the number as `wrap = 4`.

Returns an [`Atom`](@ref), which `|` or `/` joins to a plot.

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# One panel for each species, side by side.
p = data(flowers) + point + x(:sepal_length) + y(:petal_length) | facet(:species)

# The same panels, one above the other.
q = data(flowers) + point + x(:sepal_length) + y(:petal_length) / facet(:species)

# Two panels in each row.
w = data(flowers) + point + x(:sepal_length) + y(:petal_length) |
    facet(:species, wrap = 2)
```
"""
function facet(field = nothing; wrap = nothing)
    wrap === nothing ||
        (wrap isa Real && !(wrap isa Bool) && isfinite(wrap) && isinteger(wrap)) ||
        throw(GogError(
            "gog: `facet(wrap = )` takes the number of panels to draw before the " *
            "line of them turns — one whole number, e.g. `wrap = 4`."))
    Atom(:facet, Dict{Symbol,Any}(:field => column_name(field, "facet"),
                                  :wrap => wrap === nothing ? nothing : Int(wrap)))
end

"""
    palette(pal, more...)

Set the colors that [`color`](@ref) uses.

There are three ways to write it:

    palette("okabe")                                         a palette by name
    palette(["#1b9e77", "#d95f02", "#7570b3"])               a vector of colors
    palette("Asia" => "tomato", "Europe" => "steelblue")     a color for each category

The categorical palettes are `"gog"` (the default), `"okabe"` and `"soft"`. A
column of numbers runs along a ramp instead: `"blue"`, `"viridis"`, `"magma"`,
`"inferno"`, `"plasma"`, `"cividis"` and `"gray"` run from low to high, and
`"blue_red"` and `"brown_teal"` go in two directions from a middle value.

Unnamed colors go to the categories in order: the declared order of a column
made with [`ordered`](@ref) or held in a `CategoricalArray`, or, for plain text,
the order in which the values first appear. When there are more
categories than colors, more colors are made, so that no two categories share
one. A color named for each category keeps that category's color whatever the
order of the rows. In that case, every name must be a category of the column
mapped to `color`, and every category drawn must have a name; the engine
refuses either mismatch and lists what is missing. A palette on a plot that
maps no color is refused, because it would change nothing.

# Arguments
- `pal`: the palette. A palette's name, a vector of colors (names such as
  `"tomato"` or hex codes such as `"#4e79a7"`), or the first `category => color`
  pair. A `Dict` of pairs, or a vector of pairs, also works. A tuple does not.
- `more...`: the rest of the `category => color` pairs, when they are written
  out one after another.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

# The Okabe-Ito colors, chosen to stay distinct for readers with color blindness.
p = data(flowers) + point + x(:sepal_length) + y(:petal_length) + color(:species) +
    palette("okabe")

# A color chosen for each species.
q = data(flowers) + point + x(:sepal_length) + y(:petal_length) + color(:species) +
    palette("setosa" => "tomato", "versicolor" => "steelblue",
            "virginica" => "seagreen")
```
"""
function palette(pal, more...)
    # Pairs, written out or collected: `palette("Asia" => "tomato", ...)`,
    # `palette(["Asia" => "tomato"])` or `palette(Dict("Asia" => "tomato"))`.
    pairs = if pal isa Pair
        Any[pal, more...]
    elseif isempty(more) && (pal isa AbstractDict ||
                             (pal isa AbstractVector && !isempty(pal) && all(p -> p isa Pair, pal)))
        collect(pal)
    else
        nothing
    end
    if pairs !== nothing
        # Which names are levels is the engine's question, since only the table
        # can answer it; this checks the shape of what was written. A name may be
        # a `Symbol` as well as a string, the way a column is.
        all(p -> p isa Pair && p.first isa Union{AbstractString,Symbol} &&
                 p.second isa AbstractString, pairs) ||
            throw(GogError("gog: `palette()` with names binds a color to each level, " *
                           "both written as text, e.g. " *
                           "`palette(\"Asia\" => \"tomato\", \"Europe\" => \"steelblue\")`."))
        return Atom(:palette, Dict{Symbol,Any}(:value => Dict{String,Any}(
            "levels" => LevelColors([String(p.first) => String(p.second) for p in pairs]))))
    end
    isempty(more) || throw(GogError(
        "gog: `palette()` takes one palette: a name, a vector of colors, or pairs " *
        "naming a color for each level, e.g. `palette(\"Asia\" => \"tomato\")`."))
    pal isa AbstractString &&
        return Atom(:palette, Dict{Symbol,Any}(:value => Dict{String,Any}("named" => String(pal))))
    if pal isa AbstractVector && all(c -> c isa AbstractString, pal)
        return Atom(:palette, Dict{Symbol,Any}(
            :value => Dict{String,Any}("custom" => String[String(c) for c in pal])))
    end
    throw(GogError(PALETTE_SHAPES))
end

# The same sentence in all four bindings, and only the examples spelled in each.
const PALETTE_SHAPES =
    "gog: `palette()` takes a palette name, a list of colors, or a color for each " *
    "level by name, e.g. `palette(\"okabe\")`, `palette([\"#1b9e77\", \"#d95f02\"])` " *
    "or `palette(\"Asia\" => \"tomato\")`."


const THEME_PRESETS = ("gog", "minimal", "bw")
const GRID_VALUES = ("both", "x", "y", "none")
const FRAME_VALUES = ("full", "axes", "none")
const LEGEND_SIDES = ("right", "left", "top", "bottom")

#     theme(preset = nothing; grid, ratio, tick_angle, font_size)
#
# Set the plot's furniture — the page rather than the ink.
#
# Everything here maps no column, so each is a *setting*; but none of it belongs to
# a mark either, which is why it is not `style()`. A layer has no gridlines and a
# plot has no fill, so the two property sets are disjoint, and telling them apart by
# where they were written would make a sub-expression mean different things in
# different places (Law 6). Spec §7 is the ruling.
#
# A named preset comes first and anything named adjusts it, because a preset you
# cannot adjust sends you straight back to asking for knobs.
#
# `font_size` is how many pixels a tick label is, and through it the size of every
# other piece of text the plot draws — the axis names and the title are a fixed step
# above it, so `11` (the default) gives 11, 13 and 16 while `16` gives 16, 19 and 23.
# One number rather than three. It is a measurement, not a multiplier, so
# `font_size = 1.5` is refused, and it names no typeface: the engine measures text
# with its own width table and has none to choose.
#
# `strip` is the facet strip's fill: the band above a panel that names the level it
# holds. Same colors as `background`. `theme("bw")` sets it white, because a gray band
# reproduces poorly in print, which is the one place that preset is for.
#
# `strip_text` is the ink of the strip's label. Leave it out and gog picks whichever of
# its two defaults reads on the band, so `theme(strip = "black")` already gives white
# type; name it when the ink is a real choice, such as navy with gold type.
#
# `legend` is which side of the plot its legends sit on: `"right"` (the default),
# `"left"`, `"top"` or `"bottom"`. Above or below the plot the keys are laid in a
# row, which leaves the width beside the panel to the panel. To leave one legend out,
# write `legend = false` on its channel instead, as in
# `color(:continent, legend = false)`.
#
# `width` and `height` are how many pixels the plot asks for. Alone that is the
# image; composed into an arrangement with `|` or `/` it is the plot's *cell*, and the
# plots that ask for nothing split what is left — which is how a marginal
# histogram says it is thin. One meaning in both places (Law 6), and not to be
# confused with `ratio`, which shapes the panel inside whatever room the plot was
# given.
#
# `preset` is also a keyword, so `theme(preset = "minimal")` is the same sentence.
"""
    theme(preset; grid, ratio, tick_angle, font_size, background, strip,
          strip_text, frame, axis_label, legend, width, height, gap)

Set the parts of a plot that show no data, such as the gridlines, the
background and the size of the text.

Each of these maps no column, so it is a setting. None of it belongs to a mark,
which is why it is not `style()`: `style()` sets how a mark is drawn and
`theme()` sets everything around the marks, so the two never set the same
property. A preset comes first, and anything named after it adjusts the preset.
Two `theme()` calls on one plot are combined; where both set the same property,
the later one wins.

On an [`Arrangement`](@ref), `theme()` takes only `width`, `height` and `gap`,
which say how big the whole picture is and how far apart its plots are.

# Arguments
- `preset`: `"gog"` (the default look), `"minimal"` (no gridlines) or `"bw"` (a
  white panel inside a full rectangle, with white facet strips: the journal look.
  Only the parts around the data change; the data keep their colors). Given by position,
  `theme("minimal")`, or by name, `theme(preset = "minimal")`.
- `grid`: which gridlines are drawn: `"both"`, `"x"`, `"y"` or `"none"`, named by
  the axis whose ticks they mark. That is what lets the setting work in
  `polar()`, where the gridlines of the y axis are rings.
- `ratio`: the panel's width divided by its height, one positive number. `1` is
  a square.
- `tick_angle`: degrees to turn the x tick labels through, between -90 and 90.
  Category names that do not fit side by side are turned to 90 without it, and
  some are left out when even that is not enough, so state an angle only to choose one.
- `font_size`: how many pixels a tick label is, and through it the size of every
  other piece of text the plot draws. The axis names and the title grow from it
  by a fixed ratio, 1.2 for each step, rounded: `11` (the default) gives 11, 13
  and 16, and `16` gives 16, 19 and 23. It is a measurement, not a multiplier, so it must be at least 4, and
  `font_size = 1.5` is refused. It names no typeface: the engine measures text
  with its own table of widths, so there is no typeface to choose.
- `background`: the panel's fill, any color the rest of the grammar accepts,
  including `"transparent"`.
- `strip`: the fill of a facet's strip, the band above a panel that names the
  category it holds. The same colors as `background`. `theme("bw")` sets it
  white, because a gray band prints poorly.
- `strip_text`: the color of the strip's text. Left out, gog picks whichever of
  its two text colors is easier to read on the strip's fill, so
  `theme(strip = "black")` already gives white text. Set it only to choose a
  color yourself, such as gold text on a navy strip.
- `frame`: how the panel is bounded: `"full"` (a rectangle, which `theme("bw")`
  sets), `"axes"` (the default: the bottom and the left side only) or `"none"`.
- `axis_label`: where each axis's name is written: `"beside"` (the default, centered
  along its own axis, which turns the y name through 90 degrees) or `"end"` (at
  the far end of the axis, level, so nothing is read sideways).
- `legend`: which side of the plot its legends are on: `"right"` (the default),
  `"left"`, `"top"` or `"bottom"`. Above or below the plot the legend's entries
  are placed in a row, so the panel can use the full width. To leave one
  legend out, write `legend = false` on its channel instead, as in
  `color(:species, legend = false)`; `true` or `false` here is refused.
- `width`, `height`: how many pixels the plot asks for, each at least 40. On its
  own that is the size of the image. In an arrangement made with `|` or `/`, it
  is the size of the plot's cell; plots that ask for no size share the rest, so
  a narrow histogram fits beside a scatter. `ratio` is different: it shapes the
  panel inside whatever room the plot was given.
- `gap`: the space between an arrangement's plots, in pixels, 0 or more; 20 when
  unset. It belongs to the arrangement, as its size does: write
  `(a | b) + theme(gap = 8)`. A single plot refuses it.

Returns an [`Atom`](@ref), which `+` adds to a plot or an arrangement.

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

# A square panel, with no gridlines.
p = data(cars) + point + x(:wt) + y(:mpg) + theme(ratio = 1, grid = "none")

# A smaller picture.
q = data(cars) + point + x(:wt) + y(:mpg) + theme(width = 400, height = 300)

# The journal look, with larger text.
r = data(cars) + point + x(:wt) + y(:mpg) + theme("bw", font_size = 14)
```
"""
function theme(args...; preset = nothing, grid = nothing, ratio = nothing,
               tick_angle = nothing, font_size = nothing, background = nothing,
               strip = nothing, strip_text = nothing, frame = nothing,
               axis_label = nothing, legend = nothing, width = nothing,
               height = nothing, gap = nothing)
    preset_first = "gog: `theme()` takes a preset name first — `theme(\"minimal\")` — " *
                   "and everything else by name: `theme(grid = \"none\")`."
    (length(args) > 1 || (!isempty(args) && preset !== nothing)) &&
        throw(GogError(preset_first))
    isempty(args) || (preset = args[1])

    if preset === nothing && grid === nothing && ratio === nothing &&
       tick_angle === nothing && font_size === nothing &&
       background === nothing && strip === nothing && strip_text === nothing &&
       frame === nothing && axis_label === nothing && legend === nothing &&
       width === nothing && height === nothing && gap === nothing
        throw(GogError("gog: `theme()` sets nothing. Name a preset or a property, " *
                       "e.g. `theme(\"minimal\")` or `theme(grid = \"none\", ratio = 1)`."))
    end
    preset === nothing || preset isa AbstractString || throw(GogError(preset_first))

    # Checked in the engine too (`check_theme`), which is what makes the rule the
    # grammar's rather than this binding's. Checking here as well is what puts the
    # error on the line that wrote it.
    grid === nothing || grid in GRID_VALUES ||
        throw(GogError("gog: `theme(grid = )` is one of " *
                       join(["\"$v\"" for v in GRID_VALUES], ", ", " or ") * "."))
    if ratio !== nothing && (!(ratio isa Real) || ratio isa Bool || !isfinite(ratio) || ratio <= 0)
        throw(GogError("gog: `theme(ratio = )` is the panel's width divided by its " *
                       "height, so it needs one positive number. `ratio = 1` is a square."))
    end
    if tick_angle !== nothing && (!(tick_angle isa Real) || tick_angle isa Bool ||
                                  !isfinite(tick_angle) || abs(tick_angle) > 90)
        throw(GogError("gog: `theme(tick_angle = )` turns the x tick labels between " *
                       "-90 and 90 degrees. Names that do not fit side by side are " *
                       "turned without it, so state an angle only to choose one, such " *
                       "as `tick_angle = 45`."))
    end

    if font_size !== nothing && (!(font_size isa Real) || font_size isa Bool ||
                                 !isfinite(font_size) || font_size < 4)
        throw(GogError("gog: `theme(font_size = )` is how many pixels a tick label " *
                       "is, not a multiplier, so it needs one number of at least 4. " *
                       "The default is 11, and the axis names and the title are " *
                       "derived from it."))
    end

    frame === nothing || frame in FRAME_VALUES ||
        throw(GogError("gog: `theme(frame = )` is one of \"full\" (a rectangle " *
                       "round the panel), \"axes\" (bottom and left only) or " *
                       "\"none\"."))
    background === nothing || background isa AbstractString ||
        throw(GogError("gog: `theme(background = )` needs a single color, e.g. " *
                       "`theme(background = \"white\")` or `\"transparent\"`."))
    strip === nothing || strip isa AbstractString ||
        throw(GogError("gog: `theme(strip = )` needs a single color for the band " *
                       "above each panel, e.g. `theme(strip = \"white\")`."))
    axis_label === nothing || axis_label in ("end", "beside") ||
        throw(GogError("gog: `theme(axis_label = )` is \"end\" or \"beside\" — " *
                       "where each axis's name sits: at the axis's far end, or " *
                       "centered along it."))
    # `legend = false` is how a channel leaves its own key out, so it is the
    # likeliest thing to be written here too; the refusal points at the channel.
    legend isa Bool &&
        throw(GogError("gog: `theme(legend = )` says which side of the plot the " *
                       "legends sit on: \"right\", \"left\", \"top\" or \"bottom\". " *
                       "To leave a legend out, write `legend = false` on the channel " *
                       "it decodes, as in `color(:continent, legend = false)`."))
    legend === nothing || legend in LEGEND_SIDES ||
        throw(GogError("gog: `theme(legend = )` is one of \"right\", \"left\", " *
                       "\"top\" or \"bottom\": the side of the plot its legends " *
                       "sit on. To leave a legend out, write `legend = false` on " *
                       "its channel."))
    strip_text === nothing || strip_text isa AbstractString ||
        throw(GogError("gog: `theme(strip_text = )` needs a single color for the " *
                       "strip's label. Leave it out and gog picks the one that " *
                       "reads on the band."))

    # One loop for both, because they are one property asked twice — see the
    # engine's `check_theme`, which states the same rule for every binding.
    for (name, value) in (("width", width), ("height", height))
        if value !== nothing && (!(value isa Real) || value isa Bool ||
                                 !isfinite(value) || value < 40)
            throw(GogError("gog: `theme($name = )` is how many pixels the plot asks " *
                           "for, so it needs one number of at least 40. On its own it " *
                           "sizes the image; composed with `|` or `/` it sizes the " *
                           "plot's cell in the arrangement."))
        end
    end
    # How far apart an arrangement's plots are: the arrangement's own setting,
    # as its size is.
    if gap !== nothing && (!(gap isa Real) || gap isa Bool || !isfinite(gap) || gap < 0)
        throw(GogError("gog: `theme(gap = )` is how many pixels apart an arrangement's plots " *
                       "stand, so it needs one number of 0 or more, e.g. `theme(gap = 8)`."))
    end

    Atom(:theme, Dict{Symbol,Any}(
        :preset => preset === nothing ? nothing : String(preset),
        :grid => grid === nothing ? nothing : String(grid),
        :ratio => ratio === nothing ? nothing : Float64(ratio),
        :tick_angle => tick_angle === nothing ? nothing : Float64(tick_angle),
        :font_size => font_size === nothing ? nothing : Float64(font_size),
        :background => background === nothing ? nothing : String(background),
        :strip => strip === nothing ? nothing : String(strip),
        :strip_text => strip_text === nothing ? nothing : String(strip_text),
        :frame => frame === nothing ? nothing : String(frame),
        :axis_label => axis_label === nothing ? nothing : String(axis_label),
        :legend => legend === nothing ? nothing : String(legend),
        :width => width === nothing ? nothing : Float64(width),
        :height => height === nothing ? nothing : Float64(height),
        :gap => gap === nothing ? nothing : Float64(gap)))
end

function text_value(value, atom::AbstractString)
    value isa AbstractString ||
        throw(GogError("gog: `$atom()` needs a string, e.g. `$atom(\"Life expectancy\")`."))
    String(value)
end

"""
    title(value)

Set the plot's title, the line of text above the plot.

# Arguments
- `value`: the title, as a string. Anything else is refused.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

p = data(cars) + point + x(:wt) + y(:mpg) +
    title("Heavier cars go fewer miles on a gallon")
```
"""
title(value) = Atom(:title, Dict{Symbol,Any}(:value => text_value(value, "title")))

"""
    subtitle(value)

Set a subtitle, a second line under the title.

It is smaller and lighter than the title. Use it for detail that would make the
title too long, such as which years the numbers cover.

# Arguments
- `value`: the subtitle, as a string. Anything else is refused.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

p = data(cars) + point + x(:wt) + y(:mpg) +
    title("Heavier cars go fewer miles on a gallon") +
    subtitle("Eight cars from 1973 and 1974")
```
"""
subtitle(value) = Atom(:subtitle, Dict{Symbol,Any}(:value => text_value(value, "subtitle")))

"""
    caption(value)

Set a caption, a small line at the bottom of the plot.

It lines up with the left side of the panel, and it usually gives the source of
the numbers, as a printed figure does.

# Arguments
- `value`: the caption, as a string. Anything else is refused.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

p = data(cars) + point + x(:wt) + y(:mpg) +
    title("Heavier cars go fewer miles on a gallon") +
    caption("Source: Motor Trend, 1974")
```
"""
caption(value) = Atom(:caption, Dict{Symbol,Any}(:value => text_value(value, "caption")))

"""
    x_label(value)

Set the name of the x axis.

Each axis is named after the first column written for it. Use `x_label()` to
name it yourself, in words a reader understands, such as the unit of the
numbers.

# Arguments
- `value`: the name, as a string. Anything else is refused.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

p = data(cars) + point + x(:wt) + y(:mpg) + x_label("Weight (1000 lb)")
```
"""
x_label(value) = Atom(:x_label, Dict{Symbol,Any}(:value => text_value(value, "x_label")))

"""
    y_label(value)

Set the name of the y axis.

Each axis is named after the first column written for it. Use `y_label()` to
name it yourself, in words a reader understands, such as the unit of the
numbers.

# Arguments
- `value`: the name, as a string. Anything else is refused.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

p = data(cars) + point + x(:wt) + y(:mpg) + y_label("Miles per gallon")
```
"""
y_label(value) = Atom(:y_label, Dict{Symbol,Any}(:value => text_value(value, "y_label")))

"""
    z_label(value)

Set the name of the z axis.

Sets the name of the z axis, as [`x_label`](@ref) and [`y_label`](@ref) do for
the other two. It is used only in a 3-D plot, where the axis names are written
on the edges of the cube. Without `z()`, gog warns that the label is ignored.

# Arguments
- `value`: the name, as a string. Anything else is refused.

Returns an [`Atom`](@ref), which `+` adds to a plot.

# Examples
```julia
hill = (x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4],
        y = [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4],
        height = [0.223, 0.435, 0.435, 0.223, 0.435, 0.846, 0.846, 0.435,
                  0.435, 0.846, 0.846, 0.435, 0.223, 0.435, 0.435, 0.223])

p = data(hill) + surface + x(:x) + y(:y) + z(:height) + z_label("Height of the hill")
```
"""
z_label(value) = Atom(:z_label, Dict{Symbol,Any}(:value => text_value(value, "z_label")))
