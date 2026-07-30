# render.jl — the bridge: a table to the wire, a spec to the CLI, an SVG back
#
# The mirror of the other three bridges, and deliberately the same shape: find the
# binary, turn each table into the column-oriented wire form, hand `{spec, data}`
# to `gog-cli` on stdin, read the SVG off stdout and the diagnostics off stderr.
# No policy lives here. Which plots are legal, what a missing value does to a row,
# what `GOG_STRICT` means — all of that is `gog-core`'s, because a rule
# implemented in a binding is a rule the other bindings will get wrong (spec §14).

const EXE = Sys.iswindows() ? "gog-cli.exe" : "gog-cli"

# ---------------------------------------------------------------------------
# Find the gog-cli binary
# ---------------------------------------------------------------------------

function bundled_cli()
    # The engine shipped inside this package, if this is a released copy. A
    # development checkout has no `bin/` and falls through to the build below.
    binary = joinpath(dirname(@__DIR__), "bin", EXE)
    isfile(binary) || return nothing
    if !Sys.iswindows()
        try
            chmod(binary, filemode(binary) | 0o111)
        catch
            return nothing
        end
    end
    binary
end

function on_path()
    for dir in split(get(ENV, "PATH", ""), Sys.iswindows() ? ';' : ':')
        isempty(dir) && continue
        candidate = joinpath(dir, EXE)
        isfile(candidate) && return candidate
    end
    nothing
end

"""
    find_gog_cli()

Locate the engine: an override, the shipped one, PATH, then a local build.

The same four sources in the same order as R's, Python's and JavaScript's, for
the same reason at step two — the binary that shipped with a package is the one
whose wire format matches it, so an unrelated `gog-cli` earlier on `PATH` must not
silently answer for it.
"""
function find_gog_cli()
    override = get(ENV, "GOG_CLI_PATH", "")
    isempty(override) || !isfile(override) || return override

    bundled = bundled_cli()
    bundled === nothing || return bundled

    found = on_path()
    found === nothing || return found

    # Walk up from this file as well as from the working directory, so a plot
    # drawn from anywhere inside the repo finds the build.
    roots = String[pwd(), @__DIR__]
    for start in (pwd(), @__DIR__)
        here = start
        for _ in 1:6
            parent = dirname(here)
            parent == here && break
            here = parent
            push!(roots, here)
        end
    end
    for root in roots, build in ("release", "debug")
        candidate = joinpath(root, "target", build, EXE)
        isfile(candidate) && return candidate
    end

    throw(GogError(
        "gog: cannot find the `gog-cli` binary — the engine that draws the plot.\n" *
        "An installed copy of gog carries its own; this one does not, so either\n" *
        "it was installed without one or this is a development checkout.\n" *
        "  Build it:  cargo build --release -p gog-cli\n" *
        "  Or point at one:  ENV[\"GOG_CLI_PATH\"] = \"/path/to/gog-cli\""))
end

# ---------------------------------------------------------------------------
# A Julia table → the wire
#
#   floats   {"gdp": [1.0, 2.0, null]}      numbers, and temporal values
#   strings  {"continent": ["Asia", null]}  text
#   levels   {"size": ["Low", "High"]}      a declared category order
#   dates    {"day": "day"}                 a column of floats read as time
# ---------------------------------------------------------------------------

"""
    Ordered(values, levels)

A column that remembers its category order — Julia's answer to R's `factor()`,
without a dependency on CategoricalArrays. Dropping the declared order would make
an ordered-category plot fall back to the order of the rows and say nothing, which
is the silent drop §12 forbids.
"""
struct Ordered{T} <: AbstractVector{T}
    values::Vector{T}
    levels::Vector{String}
end

# `Base.size(o.values)`, spelled out. Inside this module a bare `size` is gog's
# **channel**, because the module exports one — so the obvious `size(o.values)`
# does not call Base at all, it calls the constructor for a size encoding and dies
# somewhere unrecognizable. The nine kernel words Base also defines are a wrinkle
# for callers (see the module docstring); this is the same wrinkle biting the
# package itself, and it is why every Base call on one of those names is qualified.
Base.size(o::Ordered) = Base.size(o.values)
Base.getindex(o::Ordered, i::Int) = o.values[i]
Base.IndexStyle(::Type{<:Ordered}) = IndexLinear()

ordered(values::AbstractVector, levels::AbstractVector) =
    Ordered(collect(values), String[string(l) for l in levels])

# **A declared order must not evaporate on assignment** (2026-07-28).
#
# `Ordered` is an `AbstractVector` because `to_wire` reads a column by iterating
# it and checks `values isa AbstractVector` first — and that supertype was also
# how the order got lost. `Dict("level" => ["High", …], "count" => [30.0, …])`
# infers `Dict{String, Vector}`, so `severity["level"] = ordered(severity["level"],
# …)` calls `convert(Vector, ·)`, which collects an `Ordered` into a plain vector
# and drops the levels. Nothing was raised; the axis fell back to the order of the
# rows and said nothing about it — the silent drop §12 forbids, arriving through
# Julia's container type inference rather than through anything gog does. It is
# also the idiom that mirrors R's `severity$level <- factor(…)`, so it is what a
# reader translating from R writes first, and the one the test never used: the
# suite builds its table one-step, which is why this passed for the type's life.
#
# So the lossy conversion is **refused rather than removed**. Dropping the
# supertype would make the assignment throw too, but it would throw a `MethodError`
# about types, and it would cost `to_wire` the iteration it is written around.
# Closing only this path keeps every legitimate use and turns the one silent
# outcome into a sentence naming the table shapes that do hold an order.
function _ordered_needs_a_table_that_holds_it(T)
    throw(GogError(
        "gog: this table cannot hold a column that declares its category order — it " *
        "takes `$T`, so storing an `ordered()` column converts it back to a plain " *
        "vector and the order is lost without a word. Declare the order where the " *
        "table is built: `Dict(\"level\" => ordered(values, levels), ...)`. A " *
        "`Dict{String,Any}` holds one too, and so does a named tuple. For the values " *
        "alone, without the order, write `.values`."))
end

Base.convert(::Type{Vector}, ::Ordered) = _ordered_needs_a_table_that_holds_it(Vector)
Base.convert(::Type{Vector{T}}, ::Ordered) where {T} =
    _ordered_needs_a_table_that_holds_it(Vector{T})

# Three table shapes, each for a reason. A `NamedTuple` of vectors is Julia with
# nothing installed and is what a first plot should need; a `Dict` is what a
# program builds; and anything answering `names` and `getindex` is a DataFrame
# (duck-typed rather than imported, so the binding keeps the engine's promise of
# depending on nothing).
table_columns(t::NamedTuple) = String[String(k) for k in keys(t)]
table_columns(t::AbstractDict) = String[string(k) for k in keys(t)]
function table_columns(t)
    try
        return String[string(c) for c in names(t)]
    catch
        throw(GogError(
            "gog: `data()` takes a table — a named tuple of columns " *
            "(`(x = [1, 2], y = [3, 4])`), a Dict, or a DataFrame. " *
            "Got $(typeof(t))."))
    end
end

table_column(t::NamedTuple, name::AbstractString) = getproperty(t, Symbol(name))
table_column(t::AbstractDict, name::AbstractString) =
    haskey(t, name) ? t[name] : t[Symbol(name)]
table_column(t, name::AbstractString) = t[!, name]

is_missing(v) = v === nothing || v === missing || (v isa AbstractFloat && isnan(v))

column_levels(values) = values isa Ordered ? values.levels :
                        (hasproperty(values, :levels) ? values.levels : nothing)

"""A table in the engine's column-oriented wire form."""
function to_wire(table, name::AbstractString)
    floats = Dict{String,Any}()
    strings = Dict{String,Any}()
    levels = Dict{String,Any}()
    dates = Dict{String,Any}()

    for column in table_columns(table)
        values = table_column(table, column)
        values isa AbstractVector || throw(GogError(
            "gog: column `$column` of `$name` is not a column — a column is a vector " *
            "of values, one per row. A single value is a length-1 vector."))
        present = [v for v in values if !is_missing(v)]

        # A column is one type — the engine's table has a `Float` column and a
        # `Str` column and nothing that is both. Deciding by majority, or by the
        # first row, would be the silent drop §12 forbids one level down, so a
        # mixed column is refused here where the caller can still see which
        # column it was.
        #
        # Julia is the only binding after R that has both a `Date` and a
        # `DateTime`, so the temporal unit is read off the *type* rather than
        # inferred; JavaScript has one `Date` and must read it off the values.
        if !isempty(present) && all(v -> v isa Dates.DateTime, present)
            floats[column] = Any[is_missing(v) ? nothing : Dates.datetime2unix(v) for v in values]
            dates[column] = "second"
        elseif !isempty(present) && all(v -> v isa Dates.Date, present)
            floats[column] = Any[is_missing(v) ? nothing :
                                 Dates.datetime2unix(Dates.DateTime(v)) for v in values]
            dates[column] = "day"
        elseif all(v -> is_missing(v) || (v isa Real && !(v isa Bool)), values)
            floats[column] = Any[is_missing(v) ? nothing : Float64(v) for v in values]
        elseif all(v -> is_missing(v) || !(v isa Real) || v isa Bool, values)
            # A Bool crosses as text, deliberately: R's `is.numeric(TRUE)` is
            # FALSE and a logical column is a category in every binding — two
            # colors and a two-row legend, not an axis running 0 to 1.
            strings[column] = Any[is_missing(v) ? nothing : string(v) for v in values]
            declared = column_levels(values)
            declared === nothing || (levels[column] = String[string(l) for l in declared])
        else
            kinds = sort(unique([string(typeof(v)) for v in present]))
            throw(GogError(
                "gog: column `$column` of `$name` mixes " * join(kinds, " and ") *
                " — a column is one type, because a scale reads it as one kind of " *
                "thing. Make it numbers (a position, a magnitude) or text (a category)."))
        end
    end

    Dict{String,Any}("floats" => floats, "strings" => strings,
                     "levels" => levels, "dates" => dates)
end

# ---------------------------------------------------------------------------
# Render to an SVG string
# ---------------------------------------------------------------------------

"""
    render_svg(plot)

Draw a plot and return the SVG as a string.
"""
# A page draws through the very same function: it answers `wire` with a spec and
# its tables exactly as a plot does, and the engine tells the two shapes apart
# itself (`ir::Figure`). `Union` rather than two methods, because there is
# genuinely one implementation.
function render_svg(plot::Union{Plot,Page})
    spec, frames = wire(plot)
    data = Dict{String,Any}()
    for (name, table) in frames
        # A `query()` table is resolved here and nowhere else — one place, at
        # render, which is what leaves room for the planner to rewrite the
        # sentence before the database is ever asked (the pushdown design).
        data[name] = to_wire(resolve_query(table, name), name)
    end
    payload = to_json(Dict{String,Any}("spec" => spec, "data" => data))

    out = IOBuffer()
    errors = IOBuffer()
    process = run(pipeline(Cmd([find_gog_cli()]); stdin = IOBuffer(payload),
                           stdout = out, stderr = errors), wait = false)
    wait(process)

    messages = strip(String(take!(errors)))
    if process.exitcode != 0
        # The diagnostics *are* the error. Surfacing them as-is rather than
        # wrapping them in an exit-code message keeps the direction the engine
        # wrote (spec §12).
        throw(GogError(isempty(messages) ?
            "gog-cli exited with status $(process.exitcode)" : messages))
    end

    # Non-fatal diagnostics — an Assumption, a dropped row — belong beside the
    # plot, not inside it: stderr, exactly where the engine put them.
    isempty(messages) || println(stderr, messages)

    String(take!(out))
end

# ---------------------------------------------------------------------------
# Showing the plot
# ---------------------------------------------------------------------------

"""The SVG wrapped for an HTML host, sized to fit its column."""
svg_block(svg::AbstractString) = "<div class=\"gog-plot\" style=\"text-align:center;\">\n" *
    replace(svg, "width=\"800\" height=\"600\"" =>
                 "width=\"800\" height=\"600\" style=\"max-width:100%;height:auto;\"",
            count = 1) * "\n</div>"

"""Draw the plot and write the SVG to `path`. Returns the path."""
function save(plot::Union{Plot,Page}, path::AbstractString)
    open(path, "w") do handle
        write(handle, render_svg(plot))
    end
    path
end

# A plot in a notebook, drawn rather than described. `Plot` already has a `show`
# for the terminal; this is the one an HTML host asks for.
Base.show(io::IO, ::MIME"image/svg+xml", plot::Union{Plot,Page}) = print(io, render_svg(plot))
