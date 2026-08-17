"""
    GrammarOfGraphics

One graphics engine written in Rust, spoken here in Julia. A plot is a
*specification*, not drawing code.

```julia
using GrammarOfGraphics
using GrammarOfGraphics: bin, count, sum, min, max, range, size, step, stack, map

gm = (gdp = [1000.0, 2000.0, 3000.0], life = [60.0, 70.0, 80.0])
render_svg(data(gm) + point + x(:gdp) + y(:life))
```

**Julia writes R's sentence.** It is the only binding besides R that can spell all
four assembly operators — `+`, `*`, `|` and `/` are ordinary overloadable
functions — and its symbols give the column/value distinction for free, the way R's
bare names do. So a sentence differs from the manual's R by one colon:

```
data(gm) + bar * bin + x(life) | facet(era)     R
data(gm) + bar * bin + x(:life) | facet(:era)   here
```

The precedence is not R's table, and it was checked rather than trusted: Julia
puts `|` in the *addition* tier where R puts it below. Left associativity makes
every sentence in the manual parse identically anyway, and the one shape that
would differ (`a | b + c`) appears in none of them and is refused by R regardless.

**The second line of that example is Julia's one wrinkle.** Ten of the kernel
words are also Base words — `bin`, `count`, `sum`, `min`, `max`, `range`,
`size`, `step`, `stack`, `map` — and Julia will not silently pick a winner between two
modules exporting one name. Importing them explicitly says which you meant, and
`Base.sum` stays reachable. It is the third spelling of a problem R has against
base R and Python has against its builtins; a grammar keeps its own vocabulary,
and each language has its own way of saying so.
"""
module GrammarOfGraphics

using Dates
using Base64: base64encode
using Random: randstring

include("errors.jl")
include("json.jl")
include("columns.jl")
include("spec.jl")
include("atoms.jl")
include("render.jl")
include("tables.jl")

# The environment is read at load time, never at the top level of a file: a
# top-level `get(ENV, …)` runs during *precompilation*, and the value the
# precompiling session saw is serialized into the cache — so a user setting
# `ENV["GOG_JS_URL"]` in their own script would be silently ignored until the
# package happened to recompile. `__init__` runs in the session that is
# actually using the package, which is the one whose environment counts.
function __init__()
    WASM_URL[] = get(ENV, "GOG_WASM_URL", "")
    JS_URL[] = get(ENV, "GOG_JS_URL", "")
end

export GogError, Plot, Page, Atom, Ordered, ordered
export data, query, render_svg, save, save_gif, svg_block, find_gog_cli, to_wire
export gog_table

# marks — the "consonants"
export point, line, path, rule, zone, area, bar, step, interval, box, ribbon, text,
       surface
# transforms
export bin, smooth, count, density, sum, mean, median, max, min, proportion,
       range, confidence, deviation, quantile, bounds, partition, dodge, stack, jitter,
       repel
# positions and spaces
export x, y, z, space, polar, nest, globe, map
# channels — the "vowels"
export color, group, size, shape, opacity, label, pattern, play
export brush
# exported only to be refused: the British spelling names its fix
export colour
# settings and plot-level atoms
export style, theme, order, facet, palette, title, x_label, y_label, z_label

end # module
