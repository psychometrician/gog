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

"""
The engine a development checkout built, when this package is sitting in one.

Walked up from this file for a directory holding both a `Cargo.toml` and
`target/{release,debug}/gog-cli`, R's `walk_up_for_engine` rule: the
`Cargo.toml` is what tells a checkout from an installed copy, since a registered
package has no Cargo workspace above it. So this can outrank the artifact
safely; it only ever answers inside a checkout.

**It has to outrank the artifact.** A checkout carries `Artifacts.toml` as well
(it is how the release ships the engine), and the artifact is the *published*
engine, so a test run or a parity run in the checkout drew with the last release
and never with the code beside it: a test of a change measured the old engine,
and a number recorded that way described the release rather than the tree.
"""
function workspace_cli()
    here = abspath(@__DIR__)
    for _ in 1:7
        if isfile(joinpath(here, "Cargo.toml"))
            for build in ("release", "debug")
                candidate = joinpath(here, "target", build, EXE)
                isfile(candidate) && return candidate
            end
        end
        parent = dirname(here)
        parent == here && break
        here = parent
    end
    nothing
end

"""
The engine Julia's own artifact system fetched at install time.

`Artifacts.toml` is **not lazy**, so `Pkg.add` downloads the right binary for
the platform before a reader ever calls this — there is nothing to install on
demand and nothing to ask them to do. A platform the release does not build for
falls through to the routes below rather than to an error. A development
checkout carries `Artifacts.toml` too, which is why `workspace_cli` is asked
first.
"""
function artifact_cli()
    toml = joinpath(dirname(@__DIR__), "Artifacts.toml")
    isfile(toml) || return nothing
    try
        hash = artifact_hash("gog_cli", toml)
        hash === nothing && return nothing
        candidate = joinpath(artifact_path(hash), EXE)
        isfile(candidate) || return nothing
        Sys.iswindows() || chmod(candidate, filemode(candidate) | 0o111)
        candidate
    catch
        # An artifact store that cannot be read is a reason to keep looking,
        # never a reason to fail: the routes below may still find an engine.
        nothing
    end
end

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

Locate the engine: an override, a checkout's own build, the shipped artifact, a
copy staged in `bin/`, PATH, then a build found from the working directory.

The same order R uses. The checkout's build comes second, ahead of the
artifact, because inside a checkout the artifact is the last release's engine
(`workspace_cli`). The shipped one then keeps the other bindings' reason: the
binary that shipped with a package is the one whose wire format matches it, so
an unrelated `gog-cli` earlier on `PATH` must not silently answer for it. A
released version fetches the engine as an artifact at install time, and a copy
staged by hand in `bin/` is found after it.
"""
function find_gog_cli()
    # A set path that names no file is refused rather than passed over: falling
    # through would draw with an engine nobody chose. The same sentence in all four
    # bindings, each with its own spelling of the fix.
    override = get(ENV, "GOG_CLI_PATH", "")
    if !isempty(override)
        isfile(override) && return override
        what = isdir(override) ?
            "which is a folder, not the engine itself. Point it at the `gog-cli` file inside it," :
            "and no file is there. Point it at the `gog-cli` file,"
        throw(GogError("gog: `GOG_CLI_PATH` is set to `" * override * "`, " * what *
            " or unset it with `delete!(ENV, \"GOG_CLI_PATH\")` to use the engine gog finds by itself."))
    end

    workspace = workspace_cli()
    workspace === nothing || return workspace

    # The artifact comes before the bundled copy because it is what a released
    # install actually has; `bin/` predates it and stays for anyone staging one
    # by hand.
    artifact = artifact_cli()
    artifact === nothing || return artifact

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

    # A released install fetches the engine as an artifact, so reaching here
    # means either a development checkout with nothing built, or a platform the
    # release does not build for. Both are real and the message names both,
    # rather than telling a reader with a working install that their install is
    # broken.
    throw(GogError(
        "gog: cannot find the `gog-cli` binary — the engine that draws the plot.\n" *
        "An installed copy fetches one for your platform, so this is either a\n" *
        "checkout with nothing built yet, or a platform the release does not\n" *
        "cover.\n" *
        "  Build it once:  cargo build --release -p gog-cli  (in a gog checkout)\n" *
        "  Then point at it:  ENV[\"GOG_CLI_PATH\"] = \"/path/to/gog-cli\""))
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
        #
        # A **declared order** is asked first, because it says what the column is
        # whatever its values look like: categories, in that order. Asked after
        # the numbers, it was lost whenever the categories were numbers —
        # `ordered([2019, 2020, 2021], ...)` crossed as a column of years to
        # measure. R sends a factor's labels plus its levels, so each value is
        # written with the same `string` the levels were.
        declared = column_levels(values)
        if declared !== nothing
            strings[column] = Any[is_missing(v) ? nothing : string(v) for v in values]
            levels[column] = String[string(l) for l in declared]
        elseif !isempty(present) && all(v -> v isa Dates.DateTime, present)
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
"""The engine's input for a plot or a page, as JSON — the one place either is
turned into what `gog-cli` reads.

Split out of `render_svg` when `save_gif` became a second caller. Two functions
serializing the same object is two chances to disagree about a number's
precision or about what a missing value crosses as, and that disagreement would
surface as a GIF that does not match the plot beside it."""
function wire_payload(plot::Union{Plot,Page}; salt = nothing)
    spec, frames = wire(plot)
    data = Dict{String,Any}()
    for (name, table) in frames
        # A `query()` table is resolved here and nowhere else — one place, at
        # render, which is what leaves room for the planner to rewrite the
        # sentence before the database is ever asked (the pushdown design).
        data[name] = to_wire(resolve_query(table, name), name)
    end
    request = Dict{String,Any}("spec" => spec, "data" => data)
    # A drawing embedded in a page names its ids after the block it sits in
    # (`html_of`); a drawing written to a file sends no salt and stays byte for
    # byte the same.
    salt === nothing || (request["salt"] = salt)
    to_json(request)
end

"""The request holding only the columns its spec names, for a page to carry.

A plot that needs the engine in the browser carries its request into the page, and
a column the sentence never maps has no business being published there. Which
columns the plot reads is the engine's to say (`gog-cli --prune`), so the list lives
in one place rather than in four bindings. An engine too old to answer writes back
something that is not a request, and then the whole request is carried, as it
always was, rather than a page that cannot draw.
"""
function pruned(request::AbstractString)
    out = IOBuffer()
    process = try
        run(pipeline(Cmd([find_gog_cli(), "--prune"]); stdin = IOBuffer(request),
                     stdout = out, stderr = devnull), wait = false)
    catch
        return String(request)
    end
    wait(process)
    text = strip(String(take!(out)))
    process.exitcode == 0 && startswith(text, "{") ? String(text) : String(request)
end

# What to say when something that is not a plot is asked to draw. A facet with no
# plot is the likely one, reached by `facet(:g) | facet(:g)`, and it failed with a
# raw `MethodError`. The direction is the sentence that facets a plot.
function not_a_plot(atom::Atom)
    if atom.kind in (:facet, :facet_pair, :atom_then_facet)
        return "gog: this is `facet()` with no plot to split. A facet splits a plot by " *
               "a column, so build the plot first and facet it: " *
               "`data(df) + point + x(:a) + y(:b) | facet(:g)`."
    end
    "gog: this is not a plot, so there is nothing to draw. A plot starts with " *
    "`data()`: `data(df) + point + x(:a) + y(:b)`."
end

render_svg(atom::Atom) = throw(GogError(not_a_plot(atom)))
save_gif(atom::Atom, path; scale = 1) = throw(GogError(not_a_plot(atom)))
save_svg(atom::Atom, path) = throw(GogError(not_a_plot(atom)))

render_svg(plot::Union{Plot,Page}) = draw_svg(plot)

# `render_svg`, with the salt a page block passes (`html_of`). `quiet` leaves the
# engine's notes unprinted, for a form a host asks for beside another one.
function draw_svg(plot::Union{Plot,Page}; salt = nothing, quiet = false)
    payload = wire_payload(plot; salt = salt)

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
    isempty(messages) || quiet || println(stderr, messages)

    String(take!(out))
end

# Whether IJulia is the host. It asks an object for every form it can show,
# SVG first and then HTML (its `ijulia_mime_types`), so a plot was drawn twice
# and every note the engine wrote printed twice under one cell. Looked up among
# the loaded packages rather than declared, as `query()` finds DBInterface, so
# this package depends on nothing it does not use.
const IJULIA = Base.PkgId(Base.UUID("7073ff75-c697-5162-941a-fcdaad2a7d2a"), "IJulia")
function in_ijulia()
    ijulia = get(Base.loaded_modules, IJULIA, nothing)
    ijulia !== nothing && isdefined(ijulia, :inited) && getfield(ijulia, :inited) === true
end

# ---------------------------------------------------------------------------
# Showing the plot
# ---------------------------------------------------------------------------

"""Where the browser assets live, when a host wants them beside the page rather
than carried inside it. A book sets these; a notebook leaves them empty and gets
the bytes inline, which is the only form that survives being emailed."""
const WASM_URL = Ref{String}("")
const JS_URL = Ref{String}("")

"""The engine and its runtime, or `nothing` — in which case plots stay static.

Searched the way `find_gog_cli` searches for the binary, and *walked up to*
rather than counted, since the distance to the repository root differs between a
script, a notebook and a test."""
function find_wasm_assets()
    bundled = (joinpath(@__DIR__, "..", "assets", "gog.wasm"),
               joinpath(@__DIR__, "..", "assets", "interactive.js"))
    all(isfile, bundled) && return (abspath(bundled[1]), abspath(bundled[2]))

    for start in unique([pwd(), @__DIR__])
        root = abspath(start)
        for _ in 1:7
            pair = (joinpath(root, "gog-wasm", "target", "wasm32-unknown-unknown",
                             "release", "gog_wasm.wasm"),
                    joinpath(root, "js-pkg", "gog", "src", "interactive.js"))
            all(isfile, pair) && return (pair[1], pair[2])
            parent = dirname(root)
            parent == root && break
            root = parent
        end
    end

    artifact_www()
end

"""
The browser engine and its modules, from the artifact `Pkg.add` fetched.

They ride in the engine's own artifact, beside `gog-cli`, so a released copy
turns, brushes and plays with the same build it draws with. A package installed
from General tracks no `assets/` and sits in no checkout, so until this route
existed such a copy drew every plot and turned none, with no message: the two
routes above answer for a staged copy and for a checkout, and every check ran in
one of those. After them for the reason `artifact_cli` comes after
`workspace_cli`: inside a checkout the artifact is the last release's build.
"""
function artifact_www()
    toml = joinpath(dirname(@__DIR__), "Artifacts.toml")
    isfile(toml) || return nothing
    try
        hash = artifact_hash("gog_cli", toml)
        hash === nothing && return nothing
        dir = artifact_path(hash)
        pair = (joinpath(dir, "gog.wasm"), joinpath(dir, "interactive.js"))
        all(isfile, pair) ? pair : nothing
    catch
        # An artifact store that cannot be read leaves the plot static, which is
        # the same honest picture a host without JavaScript shows.
        nothing
    end
end

"""The view module alone, or `nothing`: zoom out, zoom in, fit, grab and save as
PNG. Every plot carries it and it asks the engine nothing, so it is looked for
apart from the engine. Found only beside the engine, as it once was, it went
missing with the engine and took every plot's buttons with it. The artifact is
not asked: it carries the view module beside the engine, so wherever it has one
it has both, and `find_wasm_assets` has already answered."""
function find_view_module()
    staged = joinpath(@__DIR__, "..", "assets", "view.js")
    isfile(staged) && return abspath(staged)

    for start in unique([pwd(), @__DIR__])
        root = abspath(start)
        for _ in 1:7
            view = joinpath(root, "js-pkg", "gog", "src", "view.js")
            isfile(view) && return view
            parent = dirname(root)
            parent == root && break
            root = parent
        end
    end
    nothing
end

"""The modules' own source, ready to sit inside `<script type="module">`.

**A `data:` URL cannot be imported where a page has a content-security policy**,
and every host that shows a plot outside a plain browser has one: JupyterLab,
VS Code notebooks, and the Positron and RStudio viewer panes. `script-src` there
does not list `data:`, so importing the module from one is refused, silently,
because a blocked module import throws nothing the page can catch. The plot still
drew, since the SVG is markup, and every control was missing. Inlining the source
survives that policy: an inline module runs under `script-src 'unsafe-inline'`.
"""
function inline_modules(paths::Vector{String})
    src = join([read(p, String) for p in paths], "\n")
    # `interactive.js` takes its view helpers from the sibling file. Inlined,
    # that specifier has nothing to resolve against, and both files are already
    # in this one scope, so the two statements naming it go.
    replace(src, r"(?:import|export)\s*\{[^}]*\}\s*from\s*\"\./view\.js\";?" => "")
end

"""The engine as a JavaScript expression evaluating to its bytes. `loadEngine()`
takes a URL *or* a BufferSource, so this is the second of the two: no fetch, no
scheme, nothing the policy can refuse."""
function wasm_expression(path::AbstractString)
    isempty(WASM_URL[]) || return "\"" * WASM_URL[] * "\""
    "Uint8Array.from(atob(\"" * base64encode(read(path)) * "\"), c => c.charCodeAt(0))"
end

"""An `import` needs a module specifier, which is stricter than a URL a `fetch`
would take. A bare word like `"gog.js"` is reserved for import maps, so a browser
refuses it outright: the script never runs, nothing is fetched, and the page
shows the static plot with nothing in the console to say why. That silence is why
this normalizes rather than documents."""
module_specifier(url::AbstractString) =
    occursin(r"^(data:|https?:|file:|/|\./|\.\./)", url) ? url : "./" * url

"""Does this spec draw in the cube? The twin of `isSpatial` in the browser module
and of `space_of` in the engine — a bound `z` projects a plot even when the
coordinate still reads "flat", so naming `space()` is sufficient, not necessary."""
# Two reasons to carry the engine, not one. A plot in the cube has an angle worth dragging; a plot that names a brush has a bound worth moving. A flat plot with neither stays a still image and pays nothing.
needs_engine(spec) = is_spatial(spec) || !isempty(get(spec, "brush", []))

function is_spatial(spec)
    # The cube's view, or the globe's: both carry an angle worth dragging.
    coord = get(spec, "coord", nothing)
    coord isa AbstractDict &&
        (get(coord, "space", nothing) !== nothing ||
         get(coord, "globe", nothing) !== nothing) && return true
    # A network with a *stated* angle is the cube form and turns; bare
    # `network()` is flat and has nothing to drag.
    net = coord isa AbstractDict ? get(coord, "network", nothing) : nothing
    net isa AbstractDict && (haskey(net, "turn") || haskey(net, "tilt")) && return true
    get(spec, "z", nothing) !== nothing && return true
    for layer in get(spec, "layers", [])
        enc = get(layer, "encodings", Dict())
        get(enc, "z", nothing) !== nothing && return true
    end
    # **Both spellings are read**, and this binding emits the first: a `Page`
    # writes its list as `cells`, so looking only for `plots` answered false for
    # every composed cube and shipped no engine. The page drew perfectly and
    # would not turn, which is the failure that hides.
    cells = get(spec, "cells", nothing)
    cells === nothing && (cells = get(spec, "plots", []))
    any(needs_engine(cell) for cell in cells)
end

"""The script that upgrades a static cube into a turnable one, or `""`."""
function interactive_block(plot::Union{Plot,Page}, id::AbstractString;
                           assets = find_wasm_assets())
    spec, frames = wire(plot)
    # Two questions, not one. The *engine* has two reasons — an angle worth
    # dragging, a bound worth moving — and both redraw. The *module* has a third,
    # and it is every plot: looking closer. A zoom scales the viewBox and
    # recomputes nothing, so it needs this file and not the WebAssembly beside it.
    engine = needs_engine(spec)

    # The buttons and the engine are found apart. Beside the engine, the view
    # module is the one next to it, since the two are inlined together and the
    # engine's module imports from its sibling. With no engine, it is looked for
    # on its own, so a plot that needs the engine keeps its buttons and stays the
    # still picture it already is.
    view_path = assets === nothing ? find_view_module() :
                joinpath(dirname(assets[2]), "view.js")
    view_path === nothing && return ""

    # A flat plot names the smaller module and sends no data.
    if !engine || assets === nothing
        head = isempty(JS_URL[]) ?
            inline_modules([view_path]) * "\n" :
            "import { mountView } from \"" *
            module_specifier(replace(JS_URL[], "interactive.js" => "view.js")) * "\";\n"
        return "\n<script type=\"module\">\n" * head *
               "mountView(\"" * id * "\");\n</script>\n"
    end
    wasm_path, js_path = assets

    # The module arrives one of two ways, and the engine likewise. A book names
    # files it serves; everything else carries them, because a notebook cell has
    # no server behind it and a temp page in a viewer pane has no directory.
    head = isempty(JS_URL[]) ?
        inline_modules([view_path, js_path]) * "\n" :
        "import { mount } from \"" * module_specifier(JS_URL[]) * "\";\n"

    data = Dict{String,Any}()
    for (name, table) in frames
        data[name] = to_wire(resolve_query(table, name), name)
    end
    request = pruned(to_json(Dict{String,Any}("spec" => spec, "data" => data)))

    "\n<script type=\"module\">\n" * head *
    "mount(\"" * id * "\", " * request *
    ", { wasm: " * wasm_expression(wasm_path) * " });\n</script>\n"
end

"""The SVG wrapped for an HTML host, sized to fit its column.

A plot in the cube also gets the script that makes it turnable. The static SVG is
still what is written, and it is what a reader sees in a PDF, in a viewer that
strips JavaScript, and before the engine loads — the script only upgrades a
picture that is already there.

**The script goes inside the container**, which is a layout rule rather than a
style choice. Quarto's `layout-ncol` divides a chunk's output into cells by
counting top-level blocks, so a `<div>` with a sibling `<script>` is two cells
and two plots become four — wrapping into two rows, each plot alone at full width
beside an empty cell holding only its script. One element is one cell."""
function svg_block(svg::AbstractString, plot = nothing; id = block_id())
    # Untyped and checked here, as every keyword in this package is, so a value
    # of the wrong kind gets gog's sentence rather than a `TypeError`.
    id isa AbstractString || throw(GogError(
        "gog: `svg_block(svg, plot, id = )` takes a string — " *
        "`svg_block(svg, plot, id = \"plot-1\")`."))
    # **Whatever size the canvas is.** This matched the literal 800x600 for as
    # long as that was the only canvas, so `size()` on a plot quietly opted it
    # out of fitting. Anchored inside the opening `<svg` tag, because `[^>]`
    # cannot cross the tag's own `>` — which keeps it off the background `<rect>`
    # carrying the same two numbers a few characters later.
    sized = replace(svg, r"(<svg[^>]*) width=\"(\d+)\" height=\"(\d+)\"" =>
                         s"\1 width=\"\2\" height=\"\3\" style=\"max-width:100%;height:auto;\"",
                    count = 1)
    block = plot === nothing ? "" : interactive_block(plot, id)
    isempty(block) &&
        return "<div class=\"gog-plot\" style=\"text-align:center;\">\n" * sized * "\n</div>"
    "<div class=\"gog-plot\" id=\"" * id * "\" style=\"text-align:center;\">\n" *
        sized * "\n" * block * "</div>"
end

"""A block's id, unique on the page it lands on."""
block_id() = "gog-" * randstring(['a':'z'; '0':'9'], 10)

"""The plot as a block for a web page, its ids its own.

The id is chosen first and the drawing is asked for with it as the salt, so every
clip and texture the drawing minted is named after this block. Ids come from the
drawing, and a page resolves an id against the whole document, so the same plot
twice on one page, or in two notebooks JupyterLab holds in one document, shared
its definitions, and the second copy drew with the first's. `render_svg` sends no
salt, so a saved file stays byte for byte the same."""
function html_of(plot::Union{Plot,Page})
    id = block_id()
    svg_block(draw_svg(plot; salt = id), plot; id = id)
end

"""
Retired for `save_svg(plot, path)` (ruled 2026-09-28).

Kept as a name so a call already written is answered with the new one rather than
with a `MethodError`, and refused before anything is written, so a file already at
`path` is left as it was: a refusal must cost nothing that was already on disk.
"""
function save(plot, path::AbstractString)
    shown = endswith(lowercase(path), ".svg") ? path : "plot.svg"
    throw(GogError("gog: `save()` is retired: `save_svg(p, \"$shown\")` writes the same " *
                   "drawing, and its name says the file is an SVG, as `save_gif()`'s says a GIF."))
end

"""    save_svg(plot, path)

Write the plot's SVG to `path`, which ends in `.svg`. Returns the path.

The drawing `render_svg()` returns, written byte for byte, so the file is the same
in every binding and on every platform. The plot is drawn first and written
second, so a plot gog refuses leaves a file already at `path` as it was.
"""
# `path` and `scale` are untyped and checked in the body, so a value of the wrong
# kind gets gog's sentence rather than a `MethodError` or a `TypeError` that names
# neither the argument nor what it takes.
function save_svg(plot::Union{Plot,Page}, path)
    (path isa AbstractString && !isempty(path)) ||
        throw(GogError("gog: `save_svg()` needs one path — `save_svg(p, \"plot.svg\")`."))
    # The name says what the file is, as `save_gif()`'s does: a path that says
    # otherwise is refused, and echoed whole with the extension corrected.
    if !endswith(lowercase(path), ".svg")
        stem = first(splitext(path))
        throw(GogError("gog: `save_svg()` writes an SVG, so the path ends in " *
                       "`.svg` — `save_svg(p, \"$(stem).svg\")`."))
    end
    svg = render_svg(plot)
    write(expanduser(path), svg)
    path
end

"""    save_gif(plot, path; scale = 1)

Write a played plot to an animated GIF. Returns the path.

A plot that binds `play()` moves in a browser, because the SVG carries its own
timing. Most other places do not read that: a message to a friend, a slide, a
post. This writes the same sequence as a GIF, which they do read.

The frames come out of the one renderer, so the file cannot disagree with the
plot. Every scale, the color map and each legend are fitted across the whole
sequence at once, and the frames are cut from that single drawing rather than
drawn again one at a time. Nothing needs to be installed.

`scale` multiplies the plot's canvas, which is 800 by 600 unless its theme says
otherwise — small for a post, so `scale = 2` doubles it.
"""
function save_gif(plot::Union{Plot,Page}, path; scale = 1)
    (path isa AbstractString && !isempty(path)) ||
        throw(GogError("gog: `save_gif()` needs one path — `save_gif(p, \"wave.gif\")`."))
    # The name says what the file is, so a path that says otherwise is refused
    # rather than quietly corrected. Writing GIF bytes into `wave.png` is the
    # kind of small lie that is discovered much later, by someone else.
    if !endswith(lowercase(path), ".gif")
        stem = first(splitext(path))
        throw(GogError("gog: `save_gif()` writes a GIF, so the path ends in " *
                       "`.gif` — `save_gif(p, \"$(stem).gif\")`."))
    end
    (scale isa Real && !(scale isa Bool) && isfinite(scale) && scale > 0) ||
        throw(GogError("gog: `save_gif(scale = )` needs one positive number, " *
                       "e.g. `save_gif(p, \"wave.gif\", scale = 2)`."))

    payload = wire_payload(plot)
    errors = IOBuffer()
    cmd = Cmd([find_gog_cli(), "--gif", expanduser(path), "--scale", string(scale)])
    process = run(pipeline(cmd; stdin = IOBuffer(payload),
                           stdout = devnull, stderr = errors), wait = false)
    wait(process)

    messages = strip(String(take!(errors)))
    if process.exitcode != 0
        throw(GogError(isempty(messages) ?
            "gog-cli exited with status $(process.exitcode)" : messages))
    end
    isempty(messages) || println(stderr, messages)
    path
end

"""A refusal, shown in a cell as the sentence the engine wrote.

The engine takes trouble over these: every one names what it would not do and
what to write instead. A display hook that lets the exception through buries
that sentence under twenty-odd frames of `limitstringmime`, `display_dict` and
`eventloop`, and not one of those lines is anywhere the author can act.

Only the *display* path does this. `render_svg()`, `save()` and `save_gif()`
still throw, so a script still stops on a refusal.
"""
function refusal_block(message::AbstractString)
    escaped = replace(message, "&" => "&amp;", "<" => "&lt;", ">" => "&gt;")
    string("<pre style=\"white-space:pre-wrap;word-break:break-word;",
           "border-left:3px solid #c2410c;padding:0.6em 0.9em;margin:0;",
           "font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:0.9em\">",
           escaped, "</pre>")
end

# A plot in a notebook, drawn rather than described. `Plot` already has a `show`
# for the terminal; this is the one an HTML host asks for.
#
# A refusal is caught here rather than thrown at the frontend. The SVG form gets
# the message as a picture, because a host that asked for an image and is handed
# markup renders neither — an SVG saying why there is no plot is the honest
# answer to the question that was asked.
function Base.show(io::IO, ::MIME"image/svg+xml", plot::Union{Plot,Page})
    try
        # Salted like a block: a host showing the bare SVG puts it in the page too.
        # Quiet under IJulia, which asks for the HTML form next and prints the
        # notes from there, once; the SVG stays in the notebook for an export.
        print(io, draw_svg(plot; salt = block_id(), quiet = in_ijulia()))
    catch error
        error isa GogError || rethrow()
        text = replace(sprint(showerror, error),
                       "&" => "&amp;", "<" => "&lt;", ">" => "&gt;")
        print(io, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"760\" height=\"96\">",
              "<foreignObject x=\"0\" y=\"0\" width=\"760\" height=\"96\">",
              "<div xmlns=\"http://www.w3.org/1999/xhtml\" style=\"white-space:pre-wrap;",
              "border-left:3px solid #c2410c;padding:0.6em 0.9em;",
              "font-family:ui-monospace,Menlo,monospace;font-size:0.85em\">",
              text, "</div></foreignObject></svg>")
    end
end

# And the HTML form, which is what carries a *turnable* cube.
#
# `image/svg+xml` alone cannot: a bare SVG has nowhere to put the script that
# drives the engine, so a Julia notebook could only ever show a still picture.
# A host that prefers SVG still gets the method above and the same drawing —
# this adds a richer form beside it rather than replacing anything, and for a
# flat plot the two differ only by the wrapping `<div>`.
function Base.show(io::IO, ::MIME"text/html", plot::Union{Plot,Page})
    try
        print(io, html_of(plot))
    catch error
        error isa GogError || rethrow()
        print(io, refusal_block(sprint(showerror, error)))
    end
end
