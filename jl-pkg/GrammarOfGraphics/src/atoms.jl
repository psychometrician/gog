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
# `legality.rs`, where every binding inherits it.

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
                  example::AbstractString)
    if !(value isa Real) || value isa Bool || !isfinite(value) || value <= 0
        throw(GogError("gog: `$atom($argument = )` needs one positive number, " *
                       "e.g. `$example`."))
    end
    Float64(value)
end

# ---------------------------------------------------------------------------
# Marks — the geometric forms
# ---------------------------------------------------------------------------

mark_atom(name::AbstractString) = Atom(:mark, Dict{Symbol,Any}(:mark => name))

const point    = mark_atom("point")
const line     = mark_atom("line")
const path     = mark_atom("path")
const rule     = mark_atom("rule")
const zone     = mark_atom("zone")
const area     = mark_atom("area")
const bar      = mark_atom("bar")
const step     = mark_atom("step")
const interval = mark_atom("interval")
const ribbon   = mark_atom("ribbon")
const text     = mark_atom("text")
# A sheet through the samples, and the one mark that draws in the cube alone. Its
# rows are nodes: the grid the two position columns describe is recovered rather
# than declared, so it wants one row per (x, y) crossing. Three positions, all
# required and all numeric — a face asserts every value *between* two nodes, and
# between two categories there is nothing to assert (for a mesh over categories,
# `bar * bin + space()`). One transform, `density`, which makes it the third
# geometry of one field: `zone * density` paints it as cells, `path * density`
# traces its contours, `surface * density` raises it with the estimate as height.
const surface  = mark_atom("surface")

"""`box` — the box-and-whisker mark, with its one knob."""
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

const smooth     = transform_atom("smooth")
const count      = transform_atom("count")
const sum        = transform_atom("sum")
const mean       = transform_atom("mean")
const median     = transform_atom("median")
const max        = transform_atom("max")
const min        = transform_atom("min")
const proportion = transform_atom("proportion")
const range      = transform_atom("range")
const dodge      = transform_atom("dodge")

"""`bin` — equal-width buckets. How many dimensions it cuts is the mark's answer."""
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

"""`density` — the smooth estimate; `levels` cuts a field into bands; `compare`
says what a violin's width means from one slot to the next."""
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
                      positive(reach, "density", "reach", "density(reach = 2.5)")))
    end)

"""`confidence` — the mean's interval per group, 0.95 unless told otherwise."""
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

"""`jitter` — the categorical-axis spread, a multiple of the default."""
const jitter = Atom(:transform, Dict{Symbol,Any}(:transform => "jitter"),
    function (args...; amount = nothing)
        amount = one_of(args, amount, "jitter", "amount")
        if amount !== nothing && (!(amount isa Real) || amount isa Bool ||
                                  !isfinite(amount) || amount < 0)
            throw(GogError(
                "gog: `jitter(amount = )` needs one non-negative number — the spread " *
                "as a multiple of the default, e.g. `jitter(0.5)` for half or " *
                "`jitter(2)` for double."))
        end
        Atom(:transform, Dict{Symbol,Any}(
            :transform => "jitter",
            :amount => amount === nothing ? nothing : Float64(amount)))
    end)

"""`stack` — the measure-axis pile. `stack(share = true)` fills every pile to 1.

The 100% stacked bar. A parameter here rather than a second reading of
`proportion` because the two divide by different totals — `proportion` by the
whole frame's, this by the slot's own — and because it composes with any
measurement, including a `sum` that `proportion` has no column to take.

`stack(baseline = )` says where each pile *hangs*, the other free choice once the
heights are fixed: `"zero"` stands every pile on the axis (the default),
`"center"` hangs each so its middle is at zero, `"wiggle"` chooses the foot that
makes the bands as flat as it can — the streamgraph. Orthogonal to `share`, which
scales the heights rather than placing them.
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
    partition(levels...; cross = false)

Divide a whole among nested parts — one ring per level of a hierarchy.

The hierarchy arrives as **columns**, outermost first: one row of the table is
one leaf, and `partition(:group, :item, :detail)` says which columns spell the
path down to it. A blank level ends that branch early, which is what gives a real
hierarchy its ragged rim.

`zone * partition(...)` flat is the icicle; the same sentence `+ polar()` is the
sunburst. `text * partition(...) + label(:name)` names each node where it sits.
What each branch is weighed by rides on `x`; bind nothing and every leaf weighs 1.

`cross = true` turns the levels across each other instead of down one axis: the
first divides the width, the second divides the height *within* each of those
columns. That is the **mosaic**, and because both directions are then spent on the
hierarchy there is no ring left to step and only the leaves are drawn.
"""
function partition(levels...; cross::Bool = false)
    if isempty(levels)
        throw(GogError(
            "gog: `partition()` needs the hierarchy's columns, outermost first — " *
            "`partition(:group, :item, :detail)` puts `group` on the innermost " *
            "ring and `detail` on the rim."))
    end
    fields = Dict{Symbol,Any}(
        :transform => "partition",
        :levels => [column_name(l, "partition") for l in levels])
    # Sent only when true, so a nested partition's wire form is byte-identical to
    # what it was before this existed.
    cross && (fields[:cross] = true)
    Atom(:transform, fields)
end

"""
    bounds(lower, upper; start, var"end")

Pre-computed bounds: `lower`/`upper` bound the measure axis, `start`/`end` the
domain. Every argument names a column.

`end` is reserved syntax in Julia, so the domain's far edge is written
`var"end" = :whatever`. That is Julia's own escape for exactly this, and using it
keeps the argument's *name* the same in all four bindings rather than inventing a
Julia-only synonym for it.
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
function check_limits(limits)
    limits === nothing && return nothing
    (limits isa Union{Tuple,AbstractVector} && length(limits) == 2) ||
        throw(GogError("gog: `limits = ` needs two numbers, e.g. " *
                       "`x(:hour, limits = (0, 24))`. Use `nothing` for an end the " *
                       "data should decide: `(0, nothing)`."))
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
            throw(GogError("gog: `limits = ` needs two numbers, e.g. " *
                           "`x(:hour, limits = (0, 24))`. Use `nothing` for an end the " *
                           "data should decide: `(0, nothing)`."))
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
"""
function check_free(free, name::AbstractString)
    (free === nothing || free === false) && return false
    free === true || throw(GogError(
        "gog: `free = ` is true or false — it says whether this axis is fitted " *
        "per panel. Which axis is up to which binding you write it on: " *
        "`$name(:<name>, free = true)` frees $name."))
    true
end

position_atom(kind::Symbol, name::AbstractString, field, scale, base, limits,
              tick_count, free) =
    Atom(kind, Dict{Symbol,Any}(:field => column_name(field, name),
                                :scale => check_scale(scale),
                                :base => check_base(base),
                                :limits => check_limits(limits),
                                :tick_count => check_tick_count(tick_count),
                                :free => check_free(free, name)))

"""Bind the x axis to a column."""
x(field; scale = nothing, base = nothing, limits = nothing, tick_count = nothing,
  free = false) =
    position_atom(:coord_x, "x", field, scale, base, limits, tick_count, free)

"""Bind the y axis to a column."""
y(field; scale = nothing, base = nothing, limits = nothing, tick_count = nothing,
  free = false) =
    position_atom(:coord_y, "y", field, scale, base, limits, tick_count, free)

"""Bind the z axis to a column — one more vowel, not a chart type."""
z(field; scale = nothing, base = nothing, limits = nothing, tick_count = nothing,
  free = false) =
    position_atom(:coord_z, "z", field, scale, base, limits, tick_count, free)

degrees(value, atom::AbstractString, name::AbstractString) = begin
    (value isa Real && !(value isa Bool) && isfinite(value)) ||
        throw(GogError("gog: `$atom($name = )` needs a single number of degrees."))
    Float64(value)
end

"""`space` — the angle a 3-D plot is viewed from."""
const space = Atom(:coord_space, Dict{Symbol,Any}(:turn => 30.0, :tilt => 25.0),
    function (args...; turn = nothing, tilt = nothing)
        length(args) >= 1 && turn === nothing && (turn = args[1])
        length(args) >= 2 && tilt === nothing && (tilt = args[2])
        length(args) > 2 && throw(GogError("gog: `space()` takes `turn` and `tilt`."))
        Atom(:coord_space, Dict{Symbol,Any}(
            :turn => turn === nothing ? 30.0 : degrees(turn, "space", "turn"),
            :tilt => tilt === nothing ? 25.0 : degrees(tilt, "space", "tilt")))
    end)

"""`polar` — the plane bent into a circle: x is the angle, y the radius."""
const polar = Atom(:coord_polar, Dict{Symbol,Any}(:start => 0.0),
    function (args...; start = nothing)
        start = one_of(args, start, "polar", "start")
        Atom(:coord_polar, Dict{Symbol,Any}(
            :start => start === nothing ? 0.0 : degrees(start, "polar", "start")))
    end)

"""`nest` — the panel packed with nested regions: the measure becomes an area.

Takes no argument, because it has no view to set: `space` and `polar` carry an
angle you could turn the same picture through, and a packing has nothing
underneath to turn."""
const nest = Atom(:coord_nest, Dict{Symbol,Any}(),
    function (args...)
        isempty(args) || throw(GogError(
            "gog: `nest()` takes no arguments — a packing has no view to set."))
        Atom(:coord_nest, Dict{Symbol,Any}())
    end)

# ---------------------------------------------------------------------------
# Channels — they map a column, and earn a legend to decode it
# ---------------------------------------------------------------------------

scaled_channel(kind::Symbol, name::AbstractString) =
    (field; scale = nothing, base = nothing, limits = nothing) ->
        Atom(kind, Dict{Symbol,Any}(:field => column_name(field, name),
                                    :scale => check_scale(scale),
                                    :base => check_base(base),
                                    :limits => check_limits(limits)))

plain_channel(kind::Symbol, name::AbstractString) =
    field -> Atom(kind, Dict{Symbol,Any}(:field => column_name(field, name)))

"""Map fill/stroke color to a column."""
const color = scaled_channel(:color, "color")
"""
The British spelling of `color()`, refused with direction.

gog writes American English throughout and accepts no second spelling, which is
Law 2 applied to the vocabulary itself: two ways to write one word is a silent
letter, and the reader pays for it. ggplot2 accepts both, so a reader arriving
from there types `colour` and, unexported, would meet Julia's `UndefVarError` —
a message that names no fix. Exported for the same reason JavaScript still
exports `facet` (spec §13).
"""
colour(args...; kwargs...) = throw(GogError(
    "gog: there is no `colour()` channel. gog spells it `color(:name)`: " *
    "American English is the grammar's only spelling, and unlike ggplot2 " *
    "there is no British alternative."))
"""Map size to a numeric column."""
const size = scaled_channel(:size, "size")
"""Map opacity to a numeric column."""
const opacity = scaled_channel(:opacity, "opacity")
"""Group a line/path by a column, without giving each group a color."""
const group = plain_channel(:group, "group")
"""Map glyph shape to a categorical column."""
const shape = plain_channel(:shape, "shape")
"""Map paint texture to a categorical column — `shape`'s twin."""
const pattern = plain_channel(:pattern, "pattern")
"""Draw a column's values as text — the `text` mark's content."""
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

"""Cut the plot into frames and play them — the time dimension.

`play` is `facet` read in time. Both split the rows by a column's distinct
values; `plot | facet(:continent)` lays the pieces out across the page and
`play(:year)` lays them out in sequence.

Every scale, the color map and every legend are fitted across the whole sequence
rather than per frame, so the axes hold still and only the data moves. A layer
that does not bind `play` is drawn in every frame, which is how a reference line
stands still behind the marks that move.

Unlike `facet`, a number is welcome: panels compete for page area, frames compete
for time. A static image made from the plot shows the first frame."""
play(field; speed = nothing) =
    Atom(:play, Dict{Symbol,Any}(:field => column_name(field, "play"),
                                 :speed => check_speed(speed)))

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

"""Let the reader select rows, and push back the rest.

`brush` puts a bound on one column's values. Rows inside it keep the plot's
colors; rows outside it are dimmed, so a selection is read against what it was
taken from. Where the page can run the engine, dragging moves the bound; on paper
it stays where the sentence put it.

**A brush highlights. It never removes rows.** Removing rows before the
statistics run is what `limits` does, on the binding, and it counts what it
dropped. Change a domain and a histogram re-bins the survivors; brush it and the
same bars stay, with the selected part standing out.

One column per `brush`. Write two for a rectangle:
`brush(:gdp, at = (1200, 45000)) + brush(:life, at = (55, 78))`."""
brush(field = nothing; at = nothing) =
    Atom(:brush, merge(Dict{Symbol,Any}(
             :field => field === nothing ? "" : column_name(field, "brush")),
                       check_brush_at(at)))

# ---------------------------------------------------------------------------
# Settings — they fix a value, map nothing, and earn no legend (spec §7)
# ---------------------------------------------------------------------------

const STYLE_STRINGS = ["color", "shape", "border_color"]
const STYLE_NUMBERS = ["opacity", "size", "border_size"]
const STYLE_FLAGS = ["caps", "center"]
const STYLE_VALUES = Dict(
    "nudge" => ["up", "down", "left", "right"],
    "pattern" => ["solid", "dashed", "dotted", "hatch", "crosshatch", "grid", "dots"],
    "arrow" => ["end", "start", "both"],
    "reach" => ["panel", "edge"])
const STYLE_PROPS = vcat(STYLE_STRINGS, STYLE_NUMBERS, STYLE_FLAGS, collect(keys(STYLE_VALUES)))

# The British spelling of a setting, and what gog spells it instead. One entry
# per gog word that has a British form; there are three, and `colour()` the
# channel is the fourth word in the grammar with one.
const BRITISH_SETTINGS = Dict("colour" => "color",
                              "border_colour" => "border_color",
                              "centre" => "center")

"""
    style(; props...)

Set constant visual properties on the nearest preceding mark.

Channels *map*: `color(:species)` asks the reader "which species?" and earns a
legend to answer it. `style()` *sets*: it fixes a property at one value for the
whole layer, consumes no scale, and produces no legend — there is nothing to
decode.
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
                           "`style($name = \"tomato\")`."))
        end
        if name in STYLE_NUMBERS && (!(value isa Real) || value isa Bool || !isfinite(value))
            throw(GogError("gog: `style($name = )` needs a single number, e.g. " *
                           "`style($name = 0.3)`."))
        end
        if name in STYLE_FLAGS && !(value isa Bool)
            throw(GogError("gog: `style($name = )` needs true or false."))
        end
        if haskey(STYLE_VALUES, name) && !(value in STYLE_VALUES[name])
            throw(GogError("gog: `style($name = )` needs one of " *
                           join(["\"$v\"" for v in STYLE_VALUES[name]], ", ") * "."))
        end
        clean[name] = name in STYLE_NUMBERS ? Float64(value) : value
    end
    Atom(:style, Dict{Symbol,Any}(:props => clean))
end

# ---------------------------------------------------------------------------
# Plot-level atoms
# ---------------------------------------------------------------------------

"""Order the categorical axis by a column."""
order(field; desc::Bool = false) =
    Atom(:order, Dict{Symbol,Any}(:field => column_name(field, "order"),
                                  :descending => desc))

"""
Name the column that splits the plot into panels. Joins with `|` or `/`.

`wrap` folds a long line of panels into a rectangle — the number is how many
panels before the line turns. Which *way* the line runs is the operator's to
say: `| facet(:g, wrap = 4)` puts four to a row, `/ facet(:g, wrap = 4)` four
to a column.
"""
function facet(field; wrap::Union{Integer,Nothing} = nothing)
    wrap isa Bool && throw(GogError(
        "gog: `facet(wrap = )` takes the number of panels to draw before the " *
        "line of them turns — one whole number, e.g. `wrap = 4`."))
    Atom(:facet, Dict{Symbol,Any}(:field => column_name(field, "facet"),
                                  :wrap => wrap))
end

"""Set the categorical palette — a name, or a list of hex colors."""
function palette(pal)
    pal isa AbstractString &&
        return Atom(:palette, Dict{Symbol,Any}(:value => Dict{String,Any}("named" => String(pal))))
    if pal isa AbstractVector && all(c -> c isa AbstractString, pal)
        return Atom(:palette, Dict{Symbol,Any}(
            :value => Dict{String,Any}("custom" => String[String(c) for c in pal])))
    end
    throw(GogError("gog: `palette()` takes a palette name (\"gog\", \"okabe\") or a " *
                   "vector of hex colors."))
end

const THEME_PRESETS = ("gog", "minimal", "bw")
const GRID_VALUES = ("both", "x", "y", "none")
const FRAME_VALUES = ("full", "axes", "none")

"""
    theme(preset = nothing; grid, ratio, tick_angle, font_size)

Set the plot's furniture — the page rather than the ink.

Everything here maps no column, so each is a *setting*; but none of it belongs to
a mark either, which is why it is not `style()`. A layer has no gridlines and a
plot has no fill, so the two property sets are disjoint, and telling them apart by
where they were written would make a sub-expression mean different things in
different places (Law 6). Spec §7 is the ruling.

A named preset comes first and anything named adjusts it, because a preset you
cannot adjust sends you straight back to asking for knobs.

`font_size` is how many pixels a tick label is, and through it the size of every
other piece of text the plot draws — the axis names and the title are a fixed step
above it, so `11` (the default) gives 11, 13 and 16 while `16` gives 16, 19 and 23.
One number rather than three. It is a measurement, not a multiplier, so
`font_size = 1.5` is refused, and it names no typeface: the engine measures text
with its own width table and has none to choose.

`strip` is the facet strip's fill: the band above a panel that names the level it
holds. Same colors as `background`. `theme("bw")` sets it white, because a gray band
reproduces poorly in print, which is the one place that preset is for.

`strip_text` is the ink of the strip's label. Leave it out and gog picks whichever of
its two defaults reads on the band, so `theme(strip = "black")` already gives white
type; name it when the ink is a real choice, such as navy with gold type.

`width` and `height` are how many pixels the plot asks for. Alone that is the
image; composed onto a page with `|` or `/` it is the plot's *cell*, and the
plots that ask for nothing split what is left — which is how a marginal
histogram says it is thin. One meaning in both places (Law 6), and not to be
confused with `ratio`, which shapes the panel inside whatever room the plot was
given.
"""
function theme(args...; grid = nothing, ratio = nothing, tick_angle = nothing,
               font_size = nothing, background = nothing, strip = nothing,
               strip_text = nothing, frame = nothing, width = nothing,
               height = nothing)
    preset = length(args) > 1 ?
        throw(GogError("gog: `theme()` takes a preset name first — " *
                       "`theme(\"minimal\")` — and everything else by name: " *
                       "`theme(grid = \"none\")`.")) :
        (isempty(args) ? nothing : args[1])

    if preset === nothing && grid === nothing && ratio === nothing &&
       tick_angle === nothing && font_size === nothing &&
       background === nothing && strip === nothing && strip_text === nothing &&
       frame === nothing && width === nothing && height === nothing
        throw(GogError("gog: `theme()` sets nothing. Name a preset or a property, " *
                       "e.g. `theme(\"minimal\")` or `theme(grid = \"none\", ratio = 1)`."))
    end
    preset === nothing || preset isa AbstractString ||
        throw(GogError("gog: `theme()` takes a preset name first — `theme(\"minimal\")`."))

    # Checked in the engine too (`check_theme`), which is what makes the rule the
    # grammar's rather than this binding's. Checking here as well is what puts the
    # error on the line that wrote it.
    grid === nothing || grid in GRID_VALUES ||
        throw(GogError("gog: `theme(grid = )` is one of " *
                       join(["\"$v\"" for v in GRID_VALUES], ", ") * "."))
    if ratio !== nothing && (!(ratio isa Real) || ratio isa Bool || !isfinite(ratio) || ratio <= 0)
        throw(GogError("gog: `theme(ratio = )` is the panel's width divided by its " *
                       "height, so it needs one positive number. `ratio = 1` is a square."))
    end
    if tick_angle !== nothing && (!(tick_angle isa Real) || tick_angle isa Bool ||
                                  !isfinite(tick_angle) || abs(tick_angle) > 90)
        throw(GogError("gog: `theme(tick_angle = )` turns the x tick labels between " *
                       "-90 and 90 degrees. `tick_angle = 45` is the usual answer to " *
                       "names that overlap."))
    end

    if font_size !== nothing && (!(font_size isa Real) || font_size isa Bool ||
                                 !isfinite(font_size) || font_size < 4)
        throw(GogError("gog: `theme(font_size = )` is how many pixels a tick label " *
                       "is, not a multiplier, so it needs one number of at least 4. " *
                       "The default is 11, and the axis names and the title are " *
                       "derived from it."))
    end

    frame === nothing || frame in FRAME_VALUES ||
        throw(GogError("gog: `theme(frame = )` is one of " *
                       join(["\"\$v\"" for v in FRAME_VALUES], ", ") *
                       " — \"full\" is a rectangle round the panel, \"axes\" bottom " *
                       "and left only."))
    background === nothing || background isa AbstractString ||
        throw(GogError("gog: `theme(background = )` needs a single color, e.g. " *
                       "`theme(background = \"white\")` or `\"transparent\"`."))
    strip === nothing || strip isa AbstractString ||
        throw(GogError("gog: `theme(strip = )` needs a single color for the band " *
                       "above each panel, e.g. `theme(strip = \"white\")`."))
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
                           "plot's cell on the page."))
        end
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
        :width => width === nothing ? nothing : Float64(width),
        :height => height === nothing ? nothing : Float64(height)))
end

function text_value(value, atom::AbstractString)
    value isa AbstractString ||
        throw(GogError("gog: `$atom()` needs a string, e.g. `$atom(\"Life expectancy\")`."))
    String(value)
end

"""Set the plot title."""
title(value) = Atom(:title, Dict{Symbol,Any}(:value => text_value(value, "title")))
"""Override the x-axis label."""
x_label(value) = Atom(:x_label, Dict{Symbol,Any}(:value => text_value(value, "x_label")))
"""Override the y-axis label."""
y_label(value) = Atom(:y_label, Dict{Symbol,Any}(:value => text_value(value, "y_label")))
"""Override the z-axis label."""
z_label(value) = Atom(:z_label, Dict{Symbol,Any}(:value => text_value(value, "z_label")))
