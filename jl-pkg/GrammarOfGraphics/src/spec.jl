# spec.jl — the plot under construction, and the four operators that build it
#
# The mirror of `r-pkg/gog/R/spec.R` and `py-pkg/gog/gog/spec.py`, and of the four
# bindings this is the one with the least to say, because **Julia can spell all
# four assembly operators**. `+`, `*`, `|` and `/` are ordinary functions here, so
# the sentence is R's, character for character apart from the colon on a column:
#
#     data(gm) + bar * bin + x(life) | facet(era)     R
#     data(gm) + bar * bin + x(:life) | facet(:era)   here
#
# **The precedence was checked rather than trusted**, because it is not the same
# table as R's. Julia puts `|` in the *addition* tier (with `+`) where R puts it
# below, and `/` in the *multiplication* tier (with `*`) as R does. Measured with
# `Meta.parse`:
#
#     a + b + c | d   =>  (a + b + c) | d      same as R — left associativity saves it
#     a | b / c       =>  a | (b / c)          same as R — `/` binds tighter
#     a * b + c       =>  (a * b) + c          same as R
#     a | b + c       =>  (a | b) + c          **differs**: R gives a | (b + c)
#
# Only the last diverges, and it is the shape where something is written *after* a
# facet. **Zero of the 493 sentences in the manual have it**, and R refuses it
# anyway (`facet()` joins with `|` or `/`, not `+`), so every sentence the book
# teaches parses identically in both languages.
#
# One thing R gives away that Julia must do by hand, exactly as Python must:
# **`+` returns a new plot and never touches the old one.** R's copy-on-modify
# makes that free; here the spec is a mutable `Dict`, so a shared `base` plot
# would grow a layer every time a variant was built from it, and `base + color(…)`
# would silently change what `base + size(…)` meant.

# ---------------------------------------------------------------------------
# Atoms
# ---------------------------------------------------------------------------

# One word of the grammar — a mark, a transform, a channel, a setting.
#
# `configure` is what lets `bar * bin` and `bar * bin(30)` reach the same code path
# the way they do in R, where a transform used bare arrives as the function itself
# and `*` calls it with its defaults. Julia has the hook the other bindings had to
# build: an instance can be callable, so the bare atom *is* the constructor.
"""
    Atom

One word of the grammar: a mark, a transform, a position, a channel, a space or a
setting.

Every word you add to a plot is an `Atom`. Some are written bare, such as the
mark `point` and the transform `count`. Others are called with their arguments,
such as `x(:wt)` and `title("Cars")`. A word that takes arguments can be both:
bare `bin` uses its defaults, and `bin(20)` sets one. A mark or a transform that
takes no arguments, such as `point` or `count`, refuses to be called, and the
message says to write it bare.

Two atoms join with `*` when the left one is a mark and the right one a
transform: `bar * count` is one layer, bars of counts. Transforms chain from
left to right, so `step * bin * cumulative` bins the rows, counts them, and adds
up the counts. Any other pair is refused.

An atom joins a plot with `+`, and a plot starts with [`data`](@ref). An atom on
its own draws nothing. At the REPL an atom prints as one line, such as
`<gog bin>`, or `<gog color(species)>` when it names a column.

Every word of the grammar returns an `Atom`, so there is no need to call this
type yourself.

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

cars = (cylinders = ["6", "6", "4", "6", "8", "6", "8", "4"],)
geyser = (eruptions = [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95,
                       4.35, 1.833, 3.917],)

# A mark joined to a transform is one layer: bars of counts.
counts = bar * count
p = data(cars) + counts + x(:cylinders)

# Transforms chain from left to right: a cumulative histogram.
h = data(geyser) + step * bin * cumulative + x(:eruptions)
```
"""
struct Atom
    kind::Symbol
    fields::Dict{Symbol,Any}
    configure::Union{Nothing,Function}
end

Atom(kind::Symbol, fields::Dict{Symbol,Any}) = Atom(kind, fields, nothing)
Atom(kind::Symbol; fields...) = Atom(kind, Dict{Symbol,Any}(fields), nothing)

atom_name(a::Atom) = String(get(a.fields, :mark, get(a.fields, :transform, a.kind)))

function (a::Atom)(args...; kwargs...)
    if a.configure === nothing
        name = atom_name(a)
        throw(GogError("gog: `$name` takes no parameters — use it bare, " *
                       "e.g. `bar * $name`."))
    end
    a.configure(args...; kwargs...)
end

function Base.show(io::IO, a::Atom)
    field = get(a.fields, :field, nothing)
    print(io, field === nothing ? "<gog $(atom_name(a))>" : "<gog $(atom_name(a))($field)>")
end

# ---------------------------------------------------------------------------
# Plot — the sentence so far
# ---------------------------------------------------------------------------

"""
    Plot

A plot being written. gog calls the chain of atoms joined by `+` a sentence,
and a `Plot` is the sentence so far.

[`data`](@ref) and [`query`](@ref) start a plot, and each `+` returns a new plot
with one more atom. The plot on the left is never changed, so one plot can be
the start of several others.

What `+` takes on the right:

- a mark, or a mark joined to a transform with `*`, which starts a new layer;
- a position, a channel, a space, `style()`, `theme()`, `palette()`, `order()`,
  a title or an axis label;
- another `data()`, which gives its table to the mark written right after it.

Where a channel is written decides what it belongs to. Written before the first
mark, it belongs to the whole plot. Written after a mark, a channel such as
`color()` belongs to that mark alone, and a position names the column that mark
reads. A layer that names no position of its own reads the first one written
for that axis.

`|` and `/` do two different things, chosen by what is on their right:

- With a [`facet`](@ref) on the right, they split the plot into panels: `|` puts
  the panels side by side, `/` one above the other, and
  `p | facet(:a) / facet(:b)` crosses two columns into a grid.
- With another plot on the right, they place the two plots together in an
  [`Arrangement`](@ref): `p | q` side by side, `p / q` one above the other.

Julia groups `/` before `+`, and `|` together with `+`, from left to right. So
when you place two plots together, put each plot in parentheses:
`(data(a) + point + x(:u) + y(:v)) | (data(b) + bar + x(:w))`.

A plot is drawn when it is shown. In a notebook, such as Jupyter or Pluto, it is
drawn as a picture under the cell. At the REPL it prints as one line, such as
`<gog plot: point on data>`. To see the picture, write it to a file with
[`save_svg`](@ref) and open the file in a web browser.

Every sentence returns a `Plot`, so there is no need to call this type yourself.

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
        hp = [110, 110, 93, 110, 175, 105, 245, 62],
        cylinders = ["6", "6", "4", "6", "8", "6", "8", "4"])

# A plot is built one atom at a time.
p = data(cars) + point
p = p + x(:wt) + y(:mpg)

# `+` leaves the plot on its left as it was, so both of these start from `p`.
by_cylinders = p + color(:cylinders)
by_power = p + size(:hp)

# One panel for each number of cylinders.
panels = p | facet(:cylinders)
```
"""
mutable struct Plot
    spec::Dict{String,Any}
    frames::Dict{String,Any}
    names::IdDict{Any,String}     # table object → the name it was given
    current_layer::Union{Nothing,Dict{String,Any}}
    pending_data::Union{Nothing,String}
    anonymous::Int
    # Which of these names the binding invented rather than being handed. Only a
    # name the *author* wrote can clash: a generated one means nothing to them,
    # so it can be renamed to make room.
    generated::Set{String}
end

function Base.show(io::IO, p::Plot)
    spec = layers_closed(p)
    marks = join([l["mark"] for l in spec["layers"]], " + ")
    print(io, "<gog plot: $(isempty(marks) ? "no mark" : marks) on $(spec["data"])>")
end

function copy_plot(p::Plot)
    Plot(deepcopy(p.spec), copy(p.frames), copy(p.names),
         p.current_layer === nothing ? nothing : deepcopy(p.current_layer),
         p.pending_data, p.anonymous, copy(p.generated))
end

"""The sealed spec and its tables, ready for the bridge.

A table still waiting for its mark is refused rather than dropped: a later `data()`
gives its table to the mark written after it, and at the end of the sentence there
is none, so the plot drew as though the table had never been written, and said
nothing.
"""
function wire(p::Plot)
    p.pending_data === nothing || throw(GogError(trailing_data(
        p.pending_data in p.generated ? nothing : p.pending_data)))
    (layers_closed(p), p.frames)
end

# Named as the author wrote it, or not at all: a name the binding invented for an
# unnamed table is one they never wrote and would not recognize.
function trailing_data(name)
    said = name === nothing ? "the last `data()`" : "`data($name)`"
    shown = name === nothing ? "..." : name
    "gog: $said ends the sentence, so no mark comes after it to read its table. " *
    "A later `data()` gives its table to the mark written directly after it: write " *
    "that mark next, as in `+ data($shown) + point`, or remove " *
    (name === nothing ? "that `data()`" : said) * "."
end

# The spec with its last open layer closed, and nothing refused: what a one-line
# description of the plot reads, which must not stop to object.
function layers_closed(p::Plot)
    spec = deepcopy(p.spec)
    if p.current_layer !== nothing
        push!(spec["layers"], deepcopy(p.current_layer))
    end
    spec
end

# ---------------------------------------------------------------------------
# data() — the table, and its name
# ---------------------------------------------------------------------------

# Start a plot with a table.
#
# Law 4 resolves nearest-table-wins **by name**. R reads the name off the
# expression with `substitute()` and Python off the caller's frame; Julia can do
# neither, so it takes JavaScript's answer, which spec §8 records with its reason:
# what a name has to *do* downstream is distinguish, and a counter distinguishes.
# An unnamed table gets a unique generated one, the same table handed over twice
# keeps the one it has, and `name =` is there for when a diagnostic should say
# `notes` rather than `data2`.
"""
    data(table; name = nothing)

Start a plot with a table.

Every plot begins by naming its table. The atoms that follow name its columns
as symbols, as in `x(:wt)`. When a plot holds more than one table, each mark
reads the nearest one written before it.

A table is a named tuple of columns, such as `(wt = [2.62, 2.875], mpg = [21.0,
21.0])`, a `Dict` of columns, or any table that answers `names(table)` and
`table[!, column]`, such as a DataFrame. The package does not depend on
DataFrames. Each column holds one kind of value: numbers, text, `true` and
`false`, `Date` or `DateTime`. A missing value is `missing`. A column made with
[`ordered`](@ref), or a `CategoricalArray`, keeps the order of its categories.

A `data()` written later in a sentence gives its table to the mark written
right after it: `data(cars) + point + x(:wt) + y(:mpg) + data(notes) + text + ...`.

# Arguments
- `table`: the table, in one of the forms above.
- `name`: the name messages use for the table, a string. Julia cannot see the
  name of the variable you passed, so a table without one is called `data`,
  then `data2`, and so on. Give one when you want a message to name your table.
  Two different tables with the same name are refused.

Returns a [`Plot`](@ref), the start of a sentence, which `+` extends.

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

p = data(cars) + point + x(:wt) + y(:mpg)

# Naming the table, which matters when a plot has more than one.
q = data(cars, name = "cars") + point + x(:wt) + y(:mpg)
```
"""
function data(table; name = nothing)
    check_table_name(name, "data(df, name = )", "data(df, name = \"notes\")")
    if table isa Atom
        throw(GogError("gog: `data()` takes a table, not an atom — " *
                       "`data(df) + point + x(:a) + y(:b)`."))
    end
    if table isa Symbol
        throw(GogError("gog: `data()` takes the table itself, not a column — " *
                       "`data(df)`, then the columns are symbols inside the plot: " *
                       "`+ x(:gdp)`."))
    end

    plot = new_plot()
    resolved = name_for!(plot, table, name)
    plot.spec["data"] = resolved
    plot.frames[resolved] = table
    plot
end

# Untyped and checked here: a `name::Union{Nothing,AbstractString}` keyword met
# `name = 5` with a `TypeError` that names neither the argument nor what it takes.
check_table_name(name, written, example) =
    name === nothing || name isa AbstractString ||
    throw(GogError("gog: `$written` takes a string — `$example`."))

# The empty sentence. One skeleton, shared by every atom that can open a plot —
# `data()` and `query()`. Two copies of this Dict is how a field gets added to
# one data source and not the other.
function new_plot()
    Plot(
        Dict{String,Any}(
            "data" => nothing,
            "layers" => Any[],
            "coord" => "flat",
            "title" => nothing,
            # `AxisSpec` is the axis's furniture, which is only its name:
            # `tick_count` moved to the channel binding 2026-07-26, beside `scale`
            # and `limits`, because how many ticks an axis gets is a property of
            # the scale (spec §10).
            "x_axis" => Dict{String,Any}("label" => nothing),
            "y_axis" => Dict{String,Any}("label" => nothing),
            "z_axis" => Dict{String,Any}("label" => nothing),
            "x" => nothing, "y" => nothing, "z" => nothing,
            "channels" => Dict{String,Any}(),
        ),
        Dict{String,Any}(), IdDict{Any,String}(), nothing, nothing, 0, Set{String}())
end

"""
A table named by a SQL query instead of held in memory.

Deliberately **not** executed when it is written: a query that ran at that moment
would foreclose pushing the transform down to the database, because the planner
has to see the whole sentence first. `resolve_query` runs it once, at render.
"""
struct Query
    connection::Any
    sql::String
end

"""
    query(connection, sql; name = nothing)

Start a plot with a table that is stored in a database.

`query()` takes the place of [`data`](@ref), and nothing after it changes: the
same operators, the same column symbols and the same transforms.

    data(orders)                         + bar + x(:status)
    query(con, "SELECT * FROM orders")   + bar + x(:status)

The SQL stays in this one argument, and the rest of the sentence is the same
grammar: `x(:status)` is still a column of the table the query returns.

The connection is your own, and gog opens none. It is a DBInterface.jl
connection, Julia's database standard, so SQLite.jl, LibPQ.jl, MySQL.jl and
DuckDB.jl all work. The package does not depend on DBInterface. It uses the one
your database driver loads, so a user who never writes SQL installs no database
package. When none is loaded, drawing the plot is refused with a message that
says what to load.

The query is not run when it is written. It runs once, when the plot is drawn.
A query that returns no rows is refused.

# Arguments
- `connection`: an open DBInterface.jl connection, such as `SQLite.DB()`. A
  string here is refused: the connection comes first, and the SQL second.
- `sql`: one `SELECT`, as a string.
- `name`: the table's name, a string. It is `"query"` when left out. A second
  query in one sentence needs a name of its own, since each mark finds its
  table by name.

Returns a [`Plot`](@ref), the start of a sentence, which `+` extends.

# Examples
This example needs the SQLite.jl and DBInterface.jl packages.

```julia
using SQLite, DBInterface

cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

con = SQLite.DB()                  # a database in memory
SQLite.load!(cars, con, "cars")    # copy the table into it
p = query(con, "SELECT wt, mpg FROM cars") + point + x(:wt) + y(:mpg)
```
"""
function query(connection, sql = nothing; name = nothing)
    # `sql` defaults so that `query("SELECT ...")` — the mistake `data()` invites,
    # that atom taking one argument — reaches this refusal rather than Julia's
    # own `MethodError: no method matching query(::String)`, which names the
    # dispatch and not the fix. The same default is in the other three bindings.
    if sql === nothing
        if connection isa AbstractString
            throw(GogError(
                "gog: `query()` takes the connection first, then the SELECT — " *
                "`query(con, \"SELECT ...\")`. A query on its own cannot say which " *
                "database it runs against, which is why the connection is written " *
                "out loud. If the rows are already in hand, that is `data(df)`."))
        end
        throw(GogError(
            "gog: `query()` takes a connection and a SELECT — " *
            "`query(con, \"SELECT ...\")`. Got $(typeof(connection)) and no query."))
    end
    if connection isa AbstractString
        throw(GogError(
            "gog: `query()` takes the connection first, then the SELECT — " *
            "`query(con, \"SELECT ...\")`. A query on its own cannot say which " *
            "database it runs against, which is why the connection is written out " *
            "loud. If the rows are already in hand, that is `data(df)`."))
    end
    if !(sql isa AbstractString)
        throw(GogError(
            "gog: `query()` takes a SELECT as text — `query(con, \"SELECT ...\")`. " *
            "Got $(typeof(sql)) for the query."))
    end
    check_table_name(name, "query(con, sql, name = )", "query(con, sql, name = \"orders\")")

    plot = new_plot()
    resolved = name === nothing ? "query" : String(name)
    plot.spec["data"] = resolved
    plot.frames[resolved] = Query(connection, String(sql))
    plot
end

# Run the query, as a table of columns. `DBInterface` is resolved from the
# session rather than imported: this package declares one dependency, `Dates`,
# and a user who never writes SQL should not gain a database stack to draw a
# plot. Rows come back as NamedTuples, which is enough to name the columns
# without `Tables.jl` either.
function resolve_query(q::Query, table::AbstractString)
    dbi = nothing
    for (pkg, mod) in Base.loaded_modules
        if pkg.name == "DBInterface"
            dbi = mod
            break
        end
    end
    dbi === nothing && throw(GogError(
        "gog: `query()` needs DBInterface.jl, which is not loaded — " *
        "`using DBInterface` (and your driver: SQLite, LibPQ, MySQL, DuckDB). " *
        "It is looked up rather than depended on, so drawing a plot from a table " *
        "in memory never asks for it."))

    result = try
        Base.invokelatest(getfield(dbi, :execute), q.connection, q.sql)
    catch err
        throw(GogError("gog: the query for `$table` failed: $(err)"))
    end

    # **The values are read during the one pass, not after it.** A database
    # cursor is commonly a *forward-only* iterator whose rows are valid only
    # while being iterated — SQLite.jl says so outright, and collecting the row
    # handles to read later fails with "row 1 is no longer valid". So each row is
    # materialized into a NamedTuple as it goes by. Using nothing but
    # `propertynames`/`getproperty` keeps this working for every DBInterface
    # driver without `Tables.jl` being a dependency.
    cols = Dict{String,Vector{Any}}()
    names = Symbol[]
    for row in result
        if isempty(names)
            names = collect(propertynames(row))
            for n in names
                cols[String(n)] = Any[]
            end
        end
        for n in names
            push!(cols[String(n)], getproperty(row, n))
        end
    end

    isempty(names) && throw(GogError(
        "gog: the query for `$table` returned no rows, so there is nothing to " *
        "draw and no columns to name."))

    Dict{String,Any}(k => v for (k, v) in cols)
end

resolve_query(table, ::AbstractString) = table

function name_for!(p::Plot, table, given::Union{Nothing,AbstractString})
    if given !== nothing
        existing = get(p.frames, given, nothing)
        if existing !== nothing && existing !== table
            throw(GogError(
                "gog: two different tables are both called `$given` — a layer resolves " *
                "its columns against the nearest table by name, so one of these can " *
                "never be reached. Give them distinct names: `data(df, name = \"…\")`."))
        end
        p.names[table] = given
        return given
    end
    already = get(p.names, table, nothing)
    already !== nothing && return already
    p.anonymous += 1
    generated = p.anonymous == 1 ? "data" : "data$(p.anonymous)"
    p.names[table] = generated
    push!(p.generated, generated)
    generated
end

# ---------------------------------------------------------------------------
# `*` — derive a layer from a mark and a transform
# ---------------------------------------------------------------------------

# `bin`'s count, `density`'s bandwidth, `confidence`'s level, `jitter`'s amount,
# `stack`'s share flag and baseline, `cumulative`'s share flag,
# and `bounds`' column names ride the *layer* on the wire (`layer.bin`, …), not
# the transform list — the transform list is names only. Absent parameters attach
# nothing, so a bare `bar * bin` stays on Sturges' rule.
const CARRIED = Set([:bin, :density, :range, :confidence, :smooth_band, :straight_band, :deviation, :quantile, :jitter, :stack, :cumulative, :bounds, :partition, :flow, :layout, :cluster])

function carry!(layer::Atom, transform::Atom)
    name = transform.fields[:transform]
    Symbol(name) in CARRIED || return
    params = Dict{Symbol,Any}()
    for (key, value) in transform.fields
        key === :transform && continue
        value === nothing && continue
        params[key] = value
    end
    isempty(params) || (layer.fields[Symbol(name)] = params)
end

function Base.:*(left::Atom, right::Atom)
    if left.kind === :mark && right.kind === :transform
        layer = Atom(:layer, Dict{Symbol,Any}(
            :mark => left.fields[:mark],
            :transforms => Any[right.fields[:transform]],
            :encodings => Dict{String,Any}()))
        haskey(left.fields, :box) && (layer.fields[:box] = left.fields[:box])
        carry!(layer, right)
        return layer
    end
    if left.kind === :layer && right.kind === :transform
        layer = Atom(:layer, deepcopy(left.fields))
        push!(layer.fields[:transforms], right.fields[:transform])
        carry!(layer, right)
        return layer
    end
    throw(GogError("gog: `*` is not defined for $(left.kind) * $(right.kind). " *
                   "Use `*` to combine a mark with a transform, e.g. `bar * bin`."))
end

Base.:*(left::Atom, right) = throw(GogError(
    "gog: `*` combines a mark with a transform — `bar * bin`, `line * smooth`. " *
    "Got $(typeof(right)) on the right."))

# ---------------------------------------------------------------------------
# `+` — the sentence accumulates left to right
# ---------------------------------------------------------------------------

Base.:+(a::Atom, b::Atom) = throw(GogError(
    "gog: these atoms have no plot to join — the sentence starts with the data: " *
    "`data(df) + point + x(:a) + y(:b)`."))

function Base.:+(left::Plot, right::Plot)
    # A second table joins mid-sentence: `… + data(notes) + text + …`
    #
    # This path keeps the table and returns, so anything else the right operand is
    # carrying would go no further. That is fine for a bare `data(df)`, which
    # carries nothing, and silent loss for a parenthesized group, whose marks,
    # positions and titles simply stop existing. Refuse instead: a dropped binding
    # is never acceptable (§12), and a sub-expression that means one thing alone
    # and nothing at all in context breaks Compositional Invariance (Law 6).
    skeleton = new_plot().spec
    skeleton["data"] = right.spec["data"]
    if right.spec != skeleton || right.current_layer !== nothing ||
       right.pending_data !== nothing
        # An unnamed table is called `data`, as `data()` names it everywhere else.
        nm = something(right.spec["data"], "data")
        # The example is built from the marks inside the parentheses, so it reads
        # as the reader's own sentence rather than one about `point` and `area`.
        marks = [layer["mark"] for layer in right.spec["layers"]]
        right.current_layer === nothing || push!(marks, right.current_layer["mark"])
        isempty(marks) && throw(GogError(
            "gog: parentheses do not group the parts of a plot, so everything " *
            "inside these would be dropped. Write them in sequence instead, each " *
            "after its own `+`, beginning with `+ data($nm)`. " *
            "Parentheses compose whole plots, with `|` and `/`."))
        example = join(["+ data($nm) + $m" for m in marks], " ")
        throw(GogError(
            "gog: parentheses do not group marks, so everything inside these would " *
            "be dropped. Write the marks in sequence instead, and repeat `data()` " *
            "before each one that reads that table: `$example`. " *
            "Parentheses compose whole plots, with `|` and `/`."))
    end

    plot = copy_plot(left)
    for (_, table) in right.frames
        name = name_for!(plot, table, get(right.names, table, nothing) == "data" ? nothing :
                                      get(right.names, table, nothing))
        plot.frames[name] = table
        plot.pending_data = name
    end
    plot
end

function Base.:+(left::Plot, right::Atom)
    plot = copy_plot(left)
    kind = right.kind

    if kind === :mark
        layer = Dict{String,Any}("mark" => right.fields[:mark],
                                 "encodings" => Dict{String,Any}(),
                                 "transforms" => Any[],
                                 "data" => plot.pending_data)
        haskey(right.fields, :box) && (layer["box"] = deepcopy(right.fields[:box]))
        open_layer!(plot, layer)

    elseif kind === :layer
        layer = Dict{String,Any}(
            "mark" => right.fields[:mark],
            "encodings" => deepcopy(right.fields[:encodings]),
            "transforms" => copy(right.fields[:transforms]),
            "data" => plot.pending_data)
        for param in (:bin, :density, :range, :confidence, :smooth_band, :straight_band, :deviation, :quantile, :jitter, :stack, :cumulative, :bounds, :partition, :flow, :layout, :cluster, :box)
            haskey(right.fields, param) &&
                (layer[String(param)] = deepcopy(right.fields[param]))
        end
        open_layer!(plot, layer)

    elseif kind === :coord_x || kind === :coord_y || kind === :coord_z
        set_position!(plot, String(kind)[end:end], right)

    elseif kind === :coord_space
        space_free(plot, "space")
        plot.spec["coord"] = Dict{String,Any}("space" => Dict{String,Any}(
            "turn" => right.fields[:turn], "tilt" => right.fields[:tilt]))

    elseif kind === :coord_polar
        space_free(plot, "polar")
        plot.spec["coord"] = Dict{String,Any}("polar" =>
            Dict{String,Any}("start" => right.fields[:start]))

    # Nest carries no view parameter, so it crosses as the bare string "nest" —
    # the one unit variant left in `CoordSpace`.
    elseif kind === :coord_nest
        space_free(plot, "nest")
        plot.spec["coord"] = "nest"

    # A globe carries the place its view faces, in space's own two words:
    # {"globe":{"turn":0,"tilt":0}} matches `CoordSpace::Globe(GlobeView)`, and
    # a bare "globe" is not a legal form.
    elseif kind === :coord_network
        space_free(plot, "network")
        view = Dict{String,Any}()
        haskey(right.fields, :turn) && (view["turn"] = right.fields[:turn])
        haskey(right.fields, :tilt) && (view["tilt"] = right.fields[:tilt])
        plot.spec["coord"] = Dict{String,Any}("network" => view)

    elseif kind === :coord_globe
        space_free(plot, "globe")
        plot.spec["coord"] = Dict{String,Any}("globe" => Dict{String,Any}(
            "turn" => right.fields[:turn], "tilt" => right.fields[:tilt]))

    # A map carries what the flattening must preserve, the same way space and
    # polar carry theirs: {"map":{"preserve":"area"}} matches
    # `CoordSpace::Map(MapView)`, and a bare "map" is not a legal form.
    elseif kind === :coord_map
        space_free(plot, "map")
        plot.spec["coord"] = Dict{String,Any}("map" =>
            Dict{String,Any}("preserve" => right.fields[:preserve]))

    elseif kind in (:color, :group, :size, :shape, :opacity, :label, :pattern, :play)
        set_channel!(plot, String(kind), right)

    # Plot-scoped, like `palette`: a predicate over rows is a fact about the
    # data, so every layer reading that column answers to it.
    elseif kind === :brush
        entry = Dict{String,Any}("field" => right.fields[:field])
        for key in (:at, :levels)
            haskey(right.fields, key) && (entry[String(key)] = right.fields[key])
        end
        push!(get!(plot.spec, "brush", Any[]), entry)

    elseif kind === :style
        set_style!(plot, right.fields[:props])

    elseif kind === :palette
        plot.spec["palette"] = right.fields[:value]

    elseif kind === :theme
        # Merged rather than replaced, so two `theme()` calls accumulate the way
        # two `style()` calls on one mark do. Only the properties actually named
        # are written, keeping "said nothing" apart from "asked for the default"
        # (spec §7).
        haskey(plot.spec, "theme") || (plot.spec["theme"] = Dict{String,Any}())
        for key in (:preset, :grid, :ratio, :tick_angle, :font_size, :background, :strip, :strip_text, :frame, :axis_label,
                    :legend, :width, :height, :gap)
            value = right.fields[key]
            value === nothing || (plot.spec["theme"][String(key)] = value)
        end

    elseif kind === :title
        plot.spec["title"] = right.fields[:value]
    elseif kind === :subtitle
        plot.spec["subtitle"] = right.fields[:value]
    elseif kind === :caption
        plot.spec["caption"] = right.fields[:value]

    elseif kind in (:x_label, :y_label, :z_label)
        plot.spec[string(String(kind)[1], "_axis")]["label"] = right.fields[:value]

    elseif kind === :order
        plot.spec["order"] = Dict{String,Any}("field" => right.fields[:field],
                                              "descending" => right.fields[:descending])

    elseif kind === :facet
        throw(GogError(
            "gog: `facet()` joins with `|` (panels side by side) or `/` (panels " *
            "stacked), not `+`. Write `plot | facet(:$(right.fields[:field]))` or " *
            "`plot / facet(:$(right.fields[:field]))`."))

    elseif kind === :atom_then_facet
        # `… + y(:b) / facet(:g)`: `/` binds tighter than `+`, so it took the atom
        # written just before it. Apply the atom, then the facet — left to right,
        # as written. Julia's precedence table puts `/` above `+` exactly as R's
        # does, so this arrives here for the same reason it does in R.
        plot = plot + right.fields[:atom]
        haskey(plot.spec, "facet") ||
            (plot.spec["facet"] = Dict{String,Any}("col" => nothing, "row" => nothing))
        facet_slot_free(plot, right.fields[:slot])
        plot.spec["facet"][right.fields[:slot]] = right.fields[:facet]
        let w = get(right.fields, :wrap, nothing)
            w === nothing || (plot.spec["facet"]["wrap"] = w)
        end

    else
        throw(GogError("gog: unknown atom `$kind`."))
    end

    plot
end

Base.:+(left::Plot, right) = throw(GogError(
    "gog: `+` joins gog atoms to a plot — a mark, a channel, a setting. " *
    "Got $(typeof(right)). A table joins through `data()`: `+ data(notes)`."))

Base.:+(left, right::Atom) = throw(GogError(
    "gog: a plot starts with `data()`, which names the table — a channel names one " *
    "of its columns, and the nearest named table wins, so the name matters. " *
    "Write `data(df) + $(atom_name(right)) + …`."))

function open_layer!(p::Plot, layer::Dict{String,Any})
    p.current_layer === nothing || push!(p.spec["layers"], p.current_layer)
    p.current_layer = layer
    p.pending_data = nothing
end

# `limits` is the domain the channel runs over when the data is not the
# authority (spec §10) — two numbers with `nothing` for an end the data should
# decide, which `json.jl` writes as the engine's `[0, null]`.
channel_def(a::Atom) = Dict{String,Any}(
    "field" => a.fields[:field],
    "scale" => get(a.fields, :scale, nothing),
    "base" => get(a.fields, :base, nothing),
    "limits" => get(a.fields, :limits, nothing),
    "tick_count" => get(a.fields, :tick_count, nothing),
    "speed" => get(a.fields, :speed, nothing),
    "free" => get(a.fields, :free, false),
    # Whether the channel's legend is drawn; `nothing` is the default, which draws it.
    "legend" => get(a.fields, :legend, nothing),
    # Whether a position's axis is drawn; `nothing` is the default, which draws it.
    "axis" => get(a.fields, :axis, nothing),
    # How many decimal places the channel's numbers are printed with.
    "decimals" => get(a.fields, :decimals, nothing),
    # Whether the channel's large numbers are written short (3.81B).
    "short" => get(a.fields, :short, nothing),
    # Whether the numbers are written as percents (25%).
    "percent" => get(a.fields, :percent, nothing))

# A position is scoped by position, like every other channel. Written before any
# mark it is the plot's; written after one it is that layer's, which is what lets
# a second `data()` say where its own rows go. One axis with two column names,
# never two axes — the scale, the ticks and the space stay the plot's.
function set_position!(p::Plot, channel::AbstractString, a::Atom)
    if p.current_layer === nothing
        p.spec[channel] = channel_def(a)
    else
        p.current_layer["encodings"][channel] = channel_def(a)
    end
end

function set_channel!(p::Plot, channel::AbstractString, a::Atom)
    if p.current_layer === nothing
        p.spec["channels"][channel] = channel_def(a)
    else
        p.current_layer["encodings"][channel] = channel_def(a)
    end
end

function set_style!(p::Plot, props::Dict{String,Any})
    if p.current_layer === nothing
        throw(GogError("gog: `style()` has no mark to style. Put it after a mark, " *
                       "e.g. `point + style(color = \"tomato\")`."))
    end
    haskey(p.current_layer, "style") || (p.current_layer["style"] = Dict{String,Any}())
    merge!(p.current_layer["style"], props)
end

# ---------------------------------------------------------------------------
# Arrangement — separate plots in one picture
#
#     plot_a | plot_b        side by side
#     plot_a / plot_b        one above the other
#     top / (main | right)   nested: the marginal plot
#
# Faceting is one plot split by a variable and sharing everything; composition
# is several plots in one picture, each keeping its own coordinate space (spec
# §11). The two wear the same operators and are told apart by the operand types,
# which in Julia is literally what multiple dispatch is for: `|(::Plot, ::Atom)`
# is a facet split, `|(::Plot, ::Plot)` is an arrangement.
#
# What relates the composed plots is one rule, and the engine owns it: the same
# column on the same axis in two of them, read the same way, is one axis — one
# scale, one panel extent, drawn once (`render::page`).
#
# **`/` binds tighter than `|` in Julia too**, so `a | b / c` reads as
# `a | (b / c)`. Parenthesize when the reading matters; the marginal plot does.
# ---------------------------------------------------------------------------

# An arrangement of plots. It carries a `spec` and its `frames` exactly as a `Plot`
# does, because every host — `render_svg`, `save`, the notebook's `show` — asks a
# figure for those two and nothing else.
"""
    Arrangement

Several plots placed together in one picture, made with `|` and `/`.

`p | q` places two plots side by side, and `p / q` places one above the other. A
row of three is `a | b | c`, and `top / (main | right)` is one plot above a row
of two. Each plot keeps its own space, its own marks and its own table.

When two of the plots put the same column on the same axis, with the same
scale, they share that axis: one scale, one range, drawn once.

An arrangement accepts one atom of its own, [`theme`](@ref), and only for its
size and spacing: `theme(width = )`, `theme(height = )` and `theme(gap = )`. Every other
atom belongs to one of the plots, so write it into that plot before placing it.
A facet splits one plot, so facet a plot before you place it, too. Put the
arrangement in parentheses before its `theme()`: `(p | q) + theme(height = 300)`.

Draw, show and save an arrangement as you would a plot: [`render_svg`](@ref),
[`save_svg`](@ref), [`save_png`](@ref) and [`save_json`](@ref) take either.

Placing plots with `|` and `/` returns an `Arrangement`, so there is no need to
call this type yourself.

# Examples
```julia
using GrammarOfGraphics: count, sum, min, max, range, size, step, stack, map

flowers = (species = ["setosa", "setosa", "setosa", "setosa",
                      "versicolor", "versicolor", "versicolor", "versicolor",
                      "virginica", "virginica", "virginica", "virginica"],
           sepal_length = [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
           petal_length = [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6])

p = data(flowers) + point + x(:sepal_length) + y(:petal_length)
q = data(flowers) + bar * count + x(:species)

# Two plots side by side, and one above the other.
side = p | q
stacked = p / q

# The size of the whole picture, and the space between its plots.
smaller = (p | q) + theme(height = 300, gap = 8)
```
"""
mutable struct Arrangement
    spec::Dict{String,Any}
    frames::Dict{String,Any}
    names::IdDict{Any,String}
    generated::Set{String}
end

function Base.show(io::IO, p::Arrangement)
    print(io, "<gog arrangement: $(length(p.spec["cells"])) cells, $(p.spec["arrange"])>")
end

"""The sealed arrangement and its tables — the same pair `wire(::Plot)` hands over."""
wire(p::Arrangement) = (deepcopy(p.spec), p.frames)

# The cells this figure contributes to an arrangement running `arrange`.
#
# An arrangement already running that way is *flattened* into it, so `a | b | c`
# is one row of three rather than a row of a row — the reading the eye gives it.
# An arrangement running the other way stays a cell of its own, which is what
# makes `top / (main | right)` two rows, the second holding two plots.
#
# An arrangement that has stated its own size does not flatten either:
# flattening keeps the cells and drops the node, and the node is where the size
# was written.
figure_cells(p::Arrangement, arrange::AbstractString) =
    p.spec["arrange"] == arrange && !haskey(p.spec, "theme") ?
        copy(p.spec["cells"]) : Any[wire(p)[1]]
figure_cells(p::Plot, ::AbstractString) = Any[wire(p)[1]]

# The next generated table name nothing is using: `data`, `data2`, …
function free_name(taken)
    "data" in taken || return "data"
    n = 2
    while "data$n" in taken
        n += 1
    end
    "data$n"
end

# Rewrite every reference to a table, through nested arrangements. A name
# reaches the wire in exactly two places — the plot's own table, and a layer
# that reads a different one — so this is the whole rewrite.
function rename_table!(cells, old::AbstractString, new::AbstractString)
    for cell in cells
        get(cell, "data", nothing) == old && (cell["data"] = new)
        for layer in get(cell, "layers", Any[])
            get(layer, "data", nothing) == old && (layer["data"] = new)
        end
        haskey(cell, "cells") && rename_table!(cell["cells"], old, new)
    end
    cells
end

# Two figures' tables, under Law 4's rule: one name, one table.
#
# A name the author wrote is theirs and cannot be moved, so two different tables
# under one of those is still refused. A generated name is the binding's own and
# means nothing to them, so it gives way instead — which is what keeps an
# arrangement of two anonymous tables legal, the way a plot of two already is.
function merge_frames!(left, right, left_cells, right_cells)
    frames = copy(left.frames)
    names = copy(left.names)
    generated = copy(left.generated)
    for (name, table) in right.frames
        if haskey(frames, name) && frames[name] !== table
            taken = union(keys(frames), keys(right.frames))
            if name in right.generated
                fresh = free_name(taken)
                rename_table!(right_cells, name, fresh)
                frames[fresh] = table
                push!(generated, fresh)
                continue
            elseif name in generated
                # The author wrote the incoming one; the binding invented the one
                # already here, so that is the one that moves.
                fresh = free_name(taken)
                rename_table!(left_cells, name, fresh)
                frames[fresh] = frames[name]
                delete!(generated, name)
                push!(generated, fresh)
                frames[name] = table
                continue
            end
            throw(GogError(
                "gog: two different tables in one arrangement are both called `$name` — a " *
                "layer resolves its columns against the nearest table by name, so one " *
                "of these can never be reached. Give them distinct names: " *
                "`data(df, name = \"...\")`."))
        end
        frames[name] = table
        name in right.generated && push!(generated, name)
    end
    merge!(names, right.names)
    (frames, names, generated)
end

function compose(left, right, arrange::AbstractString)
    left_cells = figure_cells(left, arrange)
    right_cells = figure_cells(right, arrange)
    frames, names, generated = merge_frames!(left, right, left_cells, right_cells)
    Arrangement(Dict{String,Any}("arrange" => arrange,
                          "cells" => vcat(left_cells, right_cells)),
         frames, names, generated)
end

# ---------------------------------------------------------------------------
# `|` and `/` — facet a plot, or compose two of them
# ---------------------------------------------------------------------------

Base.:|(left::Plot, right::Atom) = facet_join(left, right, "col", "|")
Base.:/(left::Plot, right::Atom) = facet_join(left, right, "row", "/")
Base.:|(left::Atom, right::Atom) = facet_join(left, right, "col", "|")
Base.:/(left::Atom, right::Atom) = facet_join(left, right, "row", "/")

# A plot on the right of an atom means two plots were being placed, and `/`, which
# Julia evaluates before `+`, caught the atom written just before it. Without these
# methods it was a raw `MethodError`.
Base.:|(left::Atom, right::Union{Plot,Arrangement}) = unparenthesized_page(left, "|")
Base.:/(left::Atom, right::Union{Plot,Arrangement}) = unparenthesized_page(left, "/")

# Only `/` groups before `+`, so only `/` says so; an atom before `|` and a plot
# after it can only have been written that way.
unparenthesized_page(left::Atom, operator::AbstractString) = throw(GogError(
    "gog: `$operator` places one plot $(operator == "/" ? "below" : "beside") another, " *
    "and $(operator == "/" ? "it groups before `+`, so " : "")here it joined " *
    "`$(atom_shown(left))` to the plot after it. Put each plot in parentheses: " *
    "`(data(df) + point + ...) $operator (data(df) + bar + ...)`."))

# Composition — dispatch on the *pair*, which is the whole design: a facet split
# takes a plot and an atom, an arrangement takes two figures.
Base.:|(left::Union{Plot,Arrangement}, right::Union{Plot,Arrangement}) =
    compose(left, table_alone(right, "|"), "beside")
Base.:/(left::Union{Plot,Arrangement}, right::Union{Plot,Arrangement}) =
    compose(left, table_alone(right, "/"), "below")

# A plot on the right that is only its table: the operator joined it before `+`
# could add the rest of that plot. `/` groups before `+`, so `(a) / data(t) +
# bar` arrives here, and Julia groups `|` with `+`, from left to right, so an
# arrangement with no parentheses at all does, `a … + y(:b) | data(t) + bar`.
# Each atom after it then reached the arrangement and was refused there, as if
# it belonged to a plot on the left. Said here instead, where the parentheses
# are missing. R and Python group `+` before `|`, so they check `/` alone.
function table_alone(right, operator::AbstractString)
    right isa Plot || return right
    skeleton = new_plot().spec
    skeleton["data"] = right.spec["data"]
    (right.spec == skeleton && right.current_layer === nothing &&
     right.pending_data === nothing) || return right
    where, which, why = operator == "/" ?
        ("below", "below", "`/` groups before `+`, so it") :
        ("beside", "beside it", "Julia groups `|` with `+`, from left to right, so `|`")
    throw(GogError(
        "gog: `$operator` places one plot $where another, and the plot $which holds " *
        "nothing but its table. $why joined that table before `+` could add the rest " *
        "of its plot. Put each plot in parentheses: " *
        "`(data(df) + point + ...) $operator (data(df) + bar + ...)`."))
end

# An arrangement can only be composed further: a facet splits *one* plot by a
# column.
Base.:|(left::Arrangement, right::Atom) = page_facet_refusal("|")
Base.:/(left::Arrangement, right::Atom) = page_facet_refusal("/")

page_facet_refusal(operator::AbstractString) = throw(GogError(
    "gog: `$operator` faceted an arrangement of plots, and a facet splits *one* plot by a " *
    "column. Facet the plots before composing them: " *
    "`(plot $operator facet(:g)) $operator other_plot`."))

# The theme properties that describe a *panel*, and so cannot be said about a
# arrangement. The engine holds the same list in `check_page_theme`; this copy
# is what puts the refusal on the line that wrote it.
const PANEL_THEME = (:preset, :grid, :ratio, :tick_angle, :font_size,
                     :background, :strip, :strip_text, :frame, :axis_label,
                     :legend)

# How an atom is named in a message: a mark and a transform are bare words
# (`point`, `mean`), and every other atom is a call (`color()`). Every atom was
# shown as `title("...")` in the example below, and a mark as `point()`. A mark
# with its transforms is held as a `layer`, a word only JavaScript has, so it is
# named the way it was written: `bar * count`.
atom_shown(a::Atom) = a.kind in (:mark, :transform) ? atom_name(a) :
                      a.kind === :layer ? compound_shown(a) : "$(written_kind(a))()"

compound_shown(a::Atom) = join([String(a.fields[:mark]); String.(a.fields[:transforms])], " * ")

# A position or a space is held as `coord_x` or `coord_polar`, and written `x`.
written_kind(a::Atom) = replace(String(a.kind), r"^coord_" => "")

# The atom written into a sentence, for an example: a transform joins a mark, a
# label holds text, and every other atom takes something.
function atom_example(a::Atom)
    a.kind === :mark && return atom_name(a)
    a.kind === :layer && return compound_shown(a)
    a.kind === :transform && return "<mark> * $(atom_name(a))"
    a.kind in (:title, :subtitle, :caption, :x_label, :y_label, :z_label) && return "$(a.kind)(\"...\")"
    return "$(written_kind(a))(...)"
end

# A plot on the right is placed on the arrangement, not added to it. Without
# this method `arrangement + data(t)` was a raw `MethodError`.
Base.:+(left::Arrangement, right::Plot) = throw(GogError(
    "gog: `+` adds atoms to one plot, and the left side is an arrangement of them. To " *
    "place another plot in the arrangement, write `arrangement | other_plot` or " *
    "`arrangement / other_plot`."))

# An atom belongs to a plot, not to the arrangement — with the one exception
# whose subject is the figure rather than a panel. `theme(height = 310)` says
# how big this arrangement is, which is the same sentence a plot writes about
# itself. When the plots on an arrangement ask for no size, the arrangement is
# the one place the figure's size can be written. A title for the arrangement as
# a whole is real and not built — designed, and deliberately not implemented
# yet.
function Base.:+(left::Arrangement, right::Atom)
    right.kind === :theme || throw(GogError(
        "gog: `$(atom_shown(right))` belongs to a plot, and the left side is an " *
        "arrangement of them. Write it into the plot it describes, before composing: " *
        "`(plot + $(atom_example(right))) | other_plot`."))

    named = filter(k -> right.fields[k] !== nothing, collect(PANEL_THEME))
    if !isempty(named)
        written = :preset in named ? "theme(\"$(right.fields[:preset])\")" :
                                     "theme($(first(named)) = )"
        throw(GogError(
            "gog: `$written` describes a panel, and an arrangement is plots placed " *
            "together rather than a panel of its own. On an arrangement, `theme()` " *
            "states how big the figure " *
            "is and how far apart its plots are — `theme(width = )`, `theme(height = )` " *
            "and `theme(gap = )` — and nothing else. Write " *
            "this into the plot it describes, before composing: " *
            "`(plot + $written) | other_plot`."))
    end

    arrangement = Arrangement(deepcopy(left.spec), left.frames, left.names,
                              copy(left.generated))
    theme = get!(arrangement.spec, "theme", Dict{String,Any}())
    for key in (:width, :height, :gap)
        value = right.fields[key]
        value === nothing || (theme[String(key)] = value)
    end
    arrangement
end

# A plot is drawn in one coordinate space. Each space atom wrote its own and
# replaced whatever was there, so the engine only ever saw the last one written:
# `space() + polar()` drew polar and dropped the cube, and `map() + polar()` drew
# polar, in silence. Restating the same space replaces its view, which is how an
# angle is changed.
function space_free(plot, kind::AbstractString)
    coord = get(plot.spec, "coord", nothing)
    was = coord isa AbstractString ? String(coord) :
          coord isa AbstractDict && !isempty(coord) ? String(first(keys(coord))) : "flat"
    (was == "flat" || was == kind) && return nothing
    throw(GogError("gog: this plot is already drawn in `$was()`, and `$kind()` asks for " *
                   "a second space. A plot is drawn in one coordinate space: keep one, or " *
                   "draw the plot twice side by side, `(p + $was()) | (p + $kind())`."))
end

# A plot splits once across and once down, so a second facet in a direction
# already split would replace the first. It did, in silence:
# `plot | facet(:continent) | facet(:era)` kept only `era`. Refused, with the
# crossing that keeps both.
function facet_slot_free(plot, slot::AbstractString)
    field = get(plot.spec["facet"], slot, nothing)
    field === nothing && return nothing
    way, op = slot == "col" ? ("panel columns", "|") : ("panel rows", "/")
    throw(GogError("gog: this plot is already split into $way by `$field`, and a " *
                   "second `$op facet()` would replace it. A plot splits once across " *
                   "and once down: cross two columns with `plot | facet(:a) / facet(:b)`, " *
                   "or split by one."))
end

function facet_join(left, right, slot::AbstractString, operator::AbstractString)
    other = slot == "col" ? "row" : "col"

    if left isa Atom
        # The operator reached an atom instead of the plot. Two legitimate ways
        # in: an inner pair (`facet(:a) / facet(:b)`), whose slots the *outer*
        # operator assigns; and `y(:b) / facet(:g)`, where `/` bound tighter than
        # `+` and took the atom written just before it.
        if right isa Atom && right.kind === :facet
            if left.kind === :facet
                return Atom(:facet_pair, Dict{Symbol,Any}(
                    :first => left.fields[:field], :second => right.fields[:field],
                    :wrap => let l = get(left.fields, :wrap, nothing)
                        l === nothing ? get(right.fields, :wrap, nothing) : l
                    end))
            end
            if left.kind === :facet_pair
                throw(GogError("gog: a plot crosses at most two facet columns — one " *
                               "for the panel rows, one for the columns."))
            end
            return Atom(:atom_then_facet, Dict{Symbol,Any}(
                :atom => left, :facet => right.fields[:field], :slot => slot,
                :wrap => get(right.fields, :wrap, nothing)))
        end
        throw(GogError(
            "gog: `$operator` facets a *plot* — build the plot first, then facet it: " *
            "`data(df) + point + x(:a) + y(:b) $operator facet(:g)`."))
    end

    if !(right isa Atom) || !(right.kind in (:facet, :facet_pair))
        throw(GogError("gog: the right side of `$operator` must be `facet(:<name>)`."))
    end

    plot = copy_plot(left)
    haskey(plot.spec, "facet") ||
        (plot.spec["facet"] = Dict{String,Any}("col" => nothing, "row" => nothing))
    for taken in (right.kind === :facet ? (slot,) : (slot, other))
        facet_slot_free(plot, taken)
    end
    if right.kind === :facet
        plot.spec["facet"][slot] = right.fields[:field]
    else
        # `plot | facet(:a) / facet(:b)`: the operator's own slot takes the first
        # column written, the other slot the second — left to right, as read.
        plot.spec["facet"][slot] = right.fields[:first]
        plot.spec["facet"][other] = right.fields[:second]
    end
    # The count rides with the column it was written on; which way the line runs
    # is the operator's, already settled. Carried even onto a crossing, where the
    # engine refuses it with the reason — dropping a binding in silence is what
    # spec §12 forbids.
    w = get(right.fields, :wrap, nothing)
    w === nothing || (plot.spec["facet"]["wrap"] = w)
    plot
end
