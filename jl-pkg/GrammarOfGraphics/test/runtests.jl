# Basic sanity test for the Julia binding.
#
#     julia --project=jl-pkg/GrammarOfGraphics -e 'using Pkg; Pkg.test()'
#
# The mirror of the other three suites: does a sentence reach the engine, does the
# engine draw it, and do the refusals refuse.
#
# The checks that are *this binding's own* are the ones Julia's own shape decides:
# that all four assembly operators really are spellable (they are, which no other
# binding but R can say), that the precedence table produces R's tree despite not
# being R's table, and that a symbol keeps the column/value distinction the two
# accessor languages had to build.

using Test
using Dates
using GrammarOfGraphics
using GrammarOfGraphics: bin, count, sum, min, max, range, size, step, stack, map

const df = Dict("x" => [1.0, 2.0, 3.0, 4.0, 5.0],
                "y" => [2.5, 3.1, 1.8, 4.0, 3.5],
                "group" => ["A", "B", "A", "B", "A"])
const bars = (category = ["A", "B", "C"], value = [10.0, 25.0, 15.0])
const gaps = (a = [1.0, missing, 3.0, 4.0], b = [2.0, 2.5, missing, 4.5])

"""A sentence the grammar must refuse, whose message must say what to do."""
macro refuses(expr, fragment)
    quote
        thrown = nothing
        try
            $(esc(expr))
        catch error
            thrown = error
        end
        @test thrown isa GogError
        if thrown isa GogError
            @test occursin("gog:", thrown.msg)
            @test occursin($(esc(fragment)), thrown.msg)
        end
    end
end

@testset "the four operators, which Julia can actually spell" begin
    p = data(df) + point + x(:x) + y(:y)
    @test length(p.spec["layers"]) == 0        # still open until sealed
    spec, _ = GrammarOfGraphics.wire(p)
    @test length(spec["layers"]) == 1
    @test spec["layers"][1]["mark"] == "point"
    # Written after the mark, so they are that layer's — position decides scope.
    @test spec["layers"][1]["encodings"]["x"]["field"] == "x"
    @test spec["x"] === nothing

    before = data(df) + x(:x) + y(:y) + point
    @test before.spec["x"]["field"] == "x"

    layered = data(df) + bar * bin + x(:x)
    spec, _ = GrammarOfGraphics.wire(layered)
    @test spec["layers"][1]["transforms"] == ["bin"]

    faceted = data(df) + point + x(:x) + y(:y) | facet(:group)
    @test faceted.spec["facet"] == Dict("col" => "group", "row" => nothing)

    stacked = data(df) + point + x(:x) + y(:y) / facet(:group)
    @test stacked.spec["facet"]["row"] == "group"

    # The cube takes a facet too, one projected box per panel. Refused as "not
    # drawn yet" until 2026-07-28, when it turned out the renderer had always
    # built its scene from the panel's own rectangle and only the check said so.
    cubes = Dict("x" => df["x"], "y" => df["y"], "group" => df["group"],
                 "z" => [1.0, 5.0, 2.0, 6.0, 3.0])
    cube_svg = render_svg(data(cubes) + point + x(:x) + y(:y) + z(:z) | facet(:group))
    @test Base.count("fill=\"#f5f5f8\"", cube_svg) == 2
    @test Base.count("stroke=\"#d8d8de\"", cube_svg) == 2

    # `wrap` folds the line of panels into a rectangle. The count rides with the
    # column; which way the line runs is the operator's, so nothing says it twice.
    wrapped = data(df) + point + x(:x) + y(:y) | facet(:group, wrap = 4)
    @test wrapped.spec["facet"] == Dict("col" => "group", "row" => nothing, "wrap" => 4)
    @test (data(df) + point + x(:x) + y(:y) / facet(:group, wrap = 4)).spec["facet"]["wrap"] == 4
    # No `wrap` written, nothing on the wire — an unwrapped facet is unmoved.
    @test !haskey((data(df) + point + x(:x) + y(:y) | facet(:group)).spec["facet"], "wrap")
end

@testset "wrap folds the ribbon, and the operator says which way it runs" begin
    levels = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"]
    wide = Dict("x" => repeat([0.0, 1.0], 10),
                "y" => collect(1.0:20.0),
                "g" => repeat(levels, inner = 2),
                "h" => repeat(["u", "v"], 10))

    svg = render_svg(data(wide) + point + x(:x) + y(:y) | facet(:g, wrap = 4))
    # Ten levels are ten panels, not the 4 x 3 rectangle's twelve cells: the
    # slack the fold left over is not a combination, so nothing is drawn there.
    @test length(collect(eachmatch(r"fill=\"#f5f5f8\"", svg))) == 10
    # A folded ribbon has a different level in every cell, so each panel is named.
    @test all(occursin(">$level</text>", svg) for level in levels)

    down = render_svg(data(wide) + point + x(:x) + y(:y) / facet(:g, wrap = 4))
    @test svg != down

    @refuses render_svg(data(wide) + point + x(:x) + y(:y) |
                        facet(:g, wrap = 2) / facet(:h)) "Drop `wrap`"
    @refuses facet(:g, wrap = true) "whole number"
end

@testset "a free scale is fitted per panel, and only the axis that asked" begin
    free = Dict("x" => [1.0, 2.0, 1.0, 2.0, 1.0, 2.0],
                "y" => [1.0, 2.0, 100.0, 200.0, 10.0, 20.0],
                "g" => ["a", "a", "b", "b", "c", "c"])
    shared = render_svg(data(free) + point + x(:x) + y(:y) | facet(:g))
    freed  = render_svg(data(free) + point + x(:x) + y(:y, free = true) | facet(:g))
    # Shared, the axis spans 1..200 and never ticks 20; freed, each panel does.
    @test !occursin(">20</text>", shared)
    @test occursin(">200</text>", freed)
    @test occursin(">20</text>", freed)

    @refuses render_svg(data(free) + point + x(:x) + y(:y, free = true)) "one panel"
    @refuses render_svg(data(free) + point + x(:x) +
                        y(:y, limits = (0, 300), free = true) | facet(:g)) "one scale per panel"
    @refuses y(:y, free = "yes") "true or false"
end

@testset "Julia's precedence gives R's tree, though the table differs" begin
    # `|` sits in Julia's *addition* tier, not below it as in R. Left
    # associativity makes the sentence group as written anyway, and `/` binds
    # tighter than `+` in both languages, which is what `y(:b) / facet(:g)`
    # depends on. Asserted on the parse, not on a rendered picture.
    @test Meta.parse("a + b + c | d") == Meta.parse("(a + b + c) | d")
    @test Meta.parse("a | b / c") == Meta.parse("a | (b / c)")
    @test Meta.parse("a * b + c") == Meta.parse("(a * b) + c")
    @test Meta.parse("a + b / c") == Meta.parse("a + (b / c)")
    # The one divergence from R, recorded so a future Julia release changing it
    # would fail here rather than quietly in the manual.
    @test Meta.parse("a | b + c") == Meta.parse("(a | b) + c")

    # And the shape that depends on `/` binding tighter than `+` really works.
    crossed = data(df) + point + x(:x) + y(:y) | facet(:group) / facet(:group)
    @test crossed.spec["facet"] == Dict("col" => "group", "row" => "group")
end

@testset "a symbol is a column, a string is a value" begin
    @refuses x("gdp") "x(:gdp)"
    @refuses color("red") "style(color = \"red\")"
    @refuses color([1, 2, 3]) "values"
    @refuses x(42) "takes a column"
    @refuses style(color = :group) "that is a channel: `color(:group)`"
    @refuses style(nonsense = 1) "is not a setting"
    @refuses style() "sets nothing"
    # One spelling of English, and the refusal has to say which. A reader
    # arriving from ggplot2 types `colour` because there it works.
    @refuses style(colour = "red") "gog spells it `color`"
    @refuses style(colour = "red") "ggplot2"
    @refuses style(border_colour = "red") "gog spells it `border_color`"
    @refuses style(centre = true) "gog spells it `center`"
    @refuses colour(:species) "gog spells it `color(:name)`"
    # A name Julia cannot write as a bare symbol still has a spelling.
    @test x(Symbol("life exp")).fields[:field] == "life exp"
end

@testset "arguments are R's, because Julia has real keywords" begin
    @test x(:x, scale = "log").fields[:scale] == "log"
    @test x(:x, scale = "log", base = 2).fields[:base] == 2.0
    @test bin(30).fields[:bins] == 30
    @test bin(bins = 30).fields[:bins] == 30
    @test bin(width = 5).fields[:width] == 5.0
    @test space(45, 20).fields[:tilt] == 20.0
    @test polar(90).fields[:start] == 90.0
    @test box("range").fields[:box]["whiskers"] == "range"
    @refuses bin(bins = 10, width = 5) "either `bins` or `width`"
    @refuses x(:x, scale = "logarithmic") "is not a scale"
    @refuses x(:x, scale = "log", base = 0.5) "greater than 1"
    @refuses mean() "takes no parameters"
end

# `category` is the third scale chosen from the column's *type*, and since
# 2026-07-28 the third that may be said out loud for nothing — the allowance
# `linear` has on a number and `time` has on a date (spec §10). It may not
# contradict the column, though: a scale says how a measured column is placed,
# and whether an axis measures at all is the column's type (§18).
@testset "`category` may be said on a text column, and refused on a number" begin
    t = Dict("place" => ["a", "b", "c"], "life" => [4.0, 5.0, 6.0],
             "gdp" => [1.0, 2.0, 3.0])
    plain = render_svg(data(t) + bar * mean + x(:place) + y(:life))
    said = render_svg(data(t) + bar * mean + x(:place, scale = "category") + y(:life))
    @test said == plain

    @refuses render_svg(data(t) + point + x(:gdp, scale = "category") + y(:life)) "factor(gdp)"
end

@testset "`text * repel` separates a label crowd and keeps every label" begin
    # The fourth offset, and the one that moves ink (spec §5). `dodge`, `stack`
    # and `jitter` resolve marks that share a *position*; a label is as wide as
    # the word it draws, so two labels overlap where their points never did.
    crowd = Dict("px" => fill(5.0, 6), "py" => fill(5.0, 6),
                 "who" => ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"])
    label_at(svg) = [(parse(Float64, m[1]), parse(Float64, m[2])) for m in
        eachmatch(r"<text x=\"([0-9.-]+)\" y=\"([0-9.-]+)\" fill=\"[^\"]*\" fill-opacity=", svg)]

    plain = label_at(render_svg(data(crowd) + text + x(:px) + y(:py) + label(:who)))
    @test length(unique(plain)) == 1

    spec = data(crowd) + text * repel + x(:px) + y(:py) + label(:who)
    svg = render_svg(spec)
    moved = label_at(svg)
    # Nothing is dropped, however impossible the packing (spec §12).
    @test length(moved) == 6
    # `Base.max`, because this file imports the grammar's `max` at the top — the
    # collision the R and Julia chapters both document, met by its own test suite.
    for i in 1:6, j in (i + 1):6
        @test Base.max(abs(moved[i][1] - moved[j][1]), abs(moved[i][2] - moved[j][2])) > 7
    end
    # One specification is one picture, however the placement anneals.
    @test svg == render_svg(spec)
    # A label pushed clear of its dot keeps a line back to it.
    @test occursin("stroke-width=\"0.7\"", svg)
    # It is `text`-only, and each refusal names the offset that fits.
    @refuses render_svg(data(crowd) + point * repel + x(:px) + y(:py)) "jitter"
    @refuses render_svg(data(crowd) + bar * repel + x(:who) + y(:py)) "dodge"
end

@testset "bounds names columns everywhere, and `end` needs Julia's escape" begin
    atom = bounds(:lo, :hi)
    @test atom.fields[:lower] == "lo"
    @test atom.fields[:upper] == "hi"
    # `end` is reserved syntax, so the domain's far edge takes `var"end"`.
    pair = bounds(start = :a, var"end" = :b)
    @test pair.fields[:start] == "a"
    @test pair.fields[:end] == "b"
    @refuses bounds() "needs column names"
end

@testset "the table, and the name Julia cannot read off a variable" begin
    p = data(df) + point + x(:x) + y(:y)
    @test p.spec["data"] == "data"
    @test collect(keys(p.frames)) == ["data"]

    notes = Dict("at" => [2.0], "value" => [3.0])
    two = data(df) + point + x(:x) + y(:y) + data(notes) + text + x(:at) + y(:value)
    @test Set(keys(two.frames)) == Set(["data", "data2"])
    spec, _ = GrammarOfGraphics.wire(two)
    @test spec["layers"][2]["data"] == "data2"

    # The same table twice is a restatement, not a clash.
    again = data(df) + point + x(:x) + y(:y) + data(df) + line
    @test collect(keys(again.frames)) == ["data"]

    named = data(df, name = "series") + point + x(:x) + y(:y)
    @test named.spec["data"] == "series"
end

@testset "a plot starts with its table" begin
    @refuses point + x(:x) "no plot to join"
    @refuses data(point) "not an atom"
    @refuses data(df) + point + x(:x) + y(:y) + facet(:group) "joins with `|`"
    @refuses data(df) + style(color = "tomato") "has no mark to style"
end

@testset "a sentence reaches the engine and comes back an SVG" begin
    svg = render_svg(data(df) + point + x(:x) + y(:y) + color(:group))
    @test startswith(svg, "<svg xmlns=\"http://www.w3.org/2000/svg\"")
    @test occursin("<circle", svg)
end

# `play` is `facet` read in time — the same split, laid out in sequence rather
# than across the page. `speed` is a real keyword argument here, which is this
# binding's idiom for what R writes the same way and JavaScript puts in a
# trailing object.
@testset "play cuts one frame per value and names each" begin
    played = Dict("x" => [1, 2, 3, 10, 20, 30],
                  "y" => [1, 2, 3, 10, 20, 30],
                  "year" => [1957, 1957, 1957, 1962, 1962, 1962])
    frames(s) = length(collect(eachmatch(r"<animate attributeName=\"display\"", s)))

    svg = render_svg(data(played) + point + x(:x) + y(:y) + play(:year))
    # Two moments, once for the marks and once for the strip that names them.
    @test frames(svg) == 4
    @test occursin(">1957</text>", svg)
    @test occursin(">1962</text>", svg)
    @test !occursin(">1957.0<", svg)   # a year is named, not measured

    # The invariant the feature rests on: no play, no timing, no bytes.
    @test !occursin("<animate", render_svg(data(played) + point + x(:x) + y(:y)))

    fast = render_svg(data(played) + point + x(:x) + y(:y) + play(:year, speed = 2))
    @test frames(fast) == 4          # the pace changes, the frames do not
    @test occursin("dur=\"0.800s\"", fast)

    @refuses play(:year, speed = 0) "above zero"
end

# The same sequence written where SVG animation is not read. Checked as a file,
# because everything this adds happens after the SVG above: the header proves it
# is a GIF, the trailer proves it was finished rather than left half-written, and
# NETSCAPE2.0 is what makes it loop instead of freezing on the last moment.
@testset "save_gif writes a played plot where SVG motion is not read" begin
    played = Dict("x" => [1, 2, 3, 10, 20, 30],
                  "y" => [1, 2, 3, 10, 20, 30],
                  "year" => [1957, 1957, 1957, 1962, 1962, 1962])
    moving = data(played) + point + x(:x) + y(:y) + play(:year)

    mktempdir() do folder
        path = save_gif(moving, joinpath(folder, "wave.gif"))
        raw = read(path)
        @test raw[1:6] == Vector{UInt8}("GIF89a")
        @test raw[end] == 0x3b
        @test occursin("NETSCAPE2.0", String(copy(raw[1:200])))

        # A plot with no moments cannot become a sequence, and the refusal says
        # what to write instead rather than leaving a file nobody asked for.
        still = data(played) + point + x(:x) + y(:y)
        err = try
            save_gif(still, joinpath(folder, "still.gif"))
            nothing
        catch e
            sprint(showerror, e)
        end
        @test err !== nothing
        @test occursin("does not play", err)
        @test occursin("play(year)", err)

        # The name says what the file is, so a path that says otherwise is
        # refused rather than quietly corrected.
        wrong = try
            save_gif(moving, joinpath(folder, "wave.png"))
            nothing
        catch e
            sprint(showerror, e)
        end
        @test wrong !== nothing
        @test occursin("ends in `.gif`", wrong)
    end
end

@testset "every mark the kernel has draws" begin
    sentences = [
        data(bars) + bar + x(:category) + y(:value),
        data(df) + line + x(:x) + y(:y),
        data(df) + path + x(:x) + y(:y),
        data(df) + area + x(:x) + y(:y),
        data(df) + box + x(:group) + y(:y),
        data(df) + box("range") + x(:group) + y(:y),
        data(df) + bar * count + x(:group),
        data(df) + point + x(:x) + y(:y) + title("A title") + x_label("An axis"),
        # `color` bound because a palette with nothing to color is now its own
        # refusal, and this list is asking whether a palette *draws*.
        data(df) + point + x(:x) + y(:y) + color(:group) + palette("okabe"),
        data(bars) + bar + x(:category) + y(:value) + polar(),
        data(df) + point + x(:x) + y(:y) + z(:y) + space(),
        data(df) + zone * bin + x(:x) + y(:y),
    ]
    for sentence in sentences
        @test startswith(render_svg(sentence), "<svg ")
    end
end

@testset "the wire carries what the engine needs" begin
    # A missing value is dropped and reported, never dropped in silence.
    @test startswith(render_svg(data(gaps) + point + x(:a) + y(:b)), "<svg ")

    # A mixed column is refused where the caller can still see which one.
    @refuses render_svg(data(Dict("a" => [1, "two", 3])) + point + x(:a) + y(:a)) "one type"

    # A declared category order survives the trip.
    table = Dict("size" => ordered(["Low", "High", "Mid"], ["Low", "Mid", "High"]),
                 "value" => [1.0, 3.0, 2.0])
    @test startswith(render_svg(data(table) + bar + x(Symbol("size")) + y(:value)), "<svg ")

    # …and the *order* survives, not merely the render. The assertion above is a
    # `startswith`, so it would have passed with the levels thrown away — which is
    # how the bug below lived: every test built its table one-step.
    axis(t) = [m.captures[1] for m in
               eachmatch(r">(Low|Mid|High)</text>", render_svg(data(t) + bar + x(Symbol("size")) + y(:value)))]
    @test axis(table) == ["Low", "Mid", "High"]

    # **The two-step form is refused rather than silently flattened.** A `Dict`
    # built from plain vectors infers `Dict{String, Vector}`, so assigning an
    # `ordered()` column into it used to `convert` the declared order away without
    # a word — the idiom that mirrors R's `severity$level <- factor(…)`, and so the
    # first thing a reader translating from R writes.
    inferred = Dict("size" => ["Low", "High", "Mid"], "value" => [1.0, 3.0, 2.0])
    @refuses (inferred["size"] = ordered(inferred["size"], ["Low", "Mid", "High"])) "declares its category order"

    # A stricter container is the same refusal, not a hole beside it.
    strict = Dict{String,Vector{String}}("size" => ["Low", "High", "Mid"])
    @refuses (strict["size"] = ordered(strict["size"], ["Low", "Mid", "High"])) "lost without a word"

    # The two shapes the refusal names must actually hold one — a direction that
    # does not lead anywhere fails §12 as squarely as no direction.
    loose = Dict{String,Any}("size" => ["Low", "High", "Mid"], "value" => [1.0, 3.0, 2.0])
    loose["size"] = ordered(loose["size"], ["Low", "Mid", "High"])
    @test axis(loose) == ["Low", "Mid", "High"]
    @test axis((size = ordered(["Low", "High", "Mid"], ["Low", "Mid", "High"]),
                value = [1.0, 3.0, 2.0])) == ["Low", "Mid", "High"]

    # Julia has both Date and DateTime, so the unit comes off the type.
    days = (when = [Date(2020, 1, 1), Date(2020, 2, 1), Date(2020, 3, 1)],
            value = [1.0, 2.0, 3.0])
    secs = (when = [DateTime(2020, 1, 1, 9, 30), DateTime(2020, 1, 1, 10, 45)],
            value = [1.0, 2.0])
    @test to_wire(days, "days")["dates"]["when"] == "day"
    @test to_wire(secs, "secs")["dates"]["when"] == "second"
    @test startswith(render_svg(data(days) + line + x(:when) + y(:value)), "<svg ")
end

@testset "theme() is the page, style() is the ink" begin
    theme_df = Dict("g" => ["Alpha", "Beta", "Gamma"], "v" => [3.0, 7.0, 5.0],
                    "side" => ["Left", "Right", "Left"])
    lines(atom) = Base.count("<line",
        render_svg(data(theme_df) + bar + x(:g) + y(:v) + atom))

    @test lines(theme(grid = "none")) < lines(style(opacity = 1))
    # The preset resolves in the engine, not here — four bindings expanding one
    # preset is four chances for them to disagree about what "minimal" means.
    @test lines(theme("minimal")) == lines(theme(grid = "none"))

    # A preset a caller cannot adjust sends them back to knobs.
    @test render_svg(data(theme_df) + bar + x(:g) + y(:v) + theme("minimal", ratio = 1)) !=
          render_svg(data(theme_df) + bar + x(:g) + y(:v) + theme("minimal"))

    @test occursin("rotate",
        render_svg(data(theme_df) + bar + x(:g) + y(:v) + theme(tick_angle = 45)))

    # One number, three sizes: the ticks take the number and the axis names and
    # the title are a fixed step above it, so a plot's text is one decision.
    font_sizes(svg) = sort(unique(parse.(Float64,
        [m.captures[1] for m in eachmatch(r"font-size=\"([0-9.]+)\"", svg)])), rev = true)
    typed(atoms...) = render_svg(foldl(+, atoms;
        init = data(theme_df) + bar + x(:g) + y(:v) + title("T")))

    @test font_sizes(typed()) == [16.0, 13.0, 11.0]
    @test font_sizes(typed(theme(font_size = 16))) == [23.0, 19.0, 16.0]
    # Asking for the size you already have must draw the plot you already had, or
    # the default is an approximation of the scale rather than a point on it.
    @test typed(theme(font_size = 11)) == typed()

    @refuses render_svg(data(theme_df) + bar + x(:g) + y(:v) + theme("dark")) "is not a theme"
    @refuses theme(grid = "diag") "is one of"
    @refuses theme(ratio = -1) "positive number"
    @refuses theme(tick_angle = 120) "-90 and 90"
    @refuses theme() "sets nothing"
    @refuses theme(frame = "box") "is one of"
    # The mistake the pixel unit invites: reading the number as a multiplier.
    @refuses theme(font_size = 1.5) "not a"

    # The preset rule, faceted on purpose: it passed for the whole life of
    # `theme("bw")` while the preset left gray strips over its white panels,
    # because an unfaceted plot draws no strip to miss.
    bw_named = render_svg((data(theme_df) + bar + x(:g) + y(:v) + theme("bw")) | facet(:side))
    @test bw_named == render_svg((data(theme_df) + bar + x(:g) + y(:v) +
        theme(background = "white", frame = "full", strip = "white")) | facet(:side))
    @test !occursin("#e4e4ec", bw_named)
    @test occursin("#e4e4ec", render_svg((data(theme_df) + bar + x(:g) + y(:v)) | facet(:side)))
    @test occursin("seagreen",
        render_svg((data(theme_df) + bar + x(:g) + y(:v) + theme(strip = "seagreen")) | facet(:side)))
    @refuses render_svg((data(theme_df) + bar + x(:g) + y(:v) +
        theme(strip = "whte")) | facet(:side)) "is not a color"

    # The ink derives from the band, so `strip = "black"` is a whole instruction:
    # without it the near-black label would sit on the near-black band.
    @test occursin("fill=\"#ffffff\" text-anchor=\"middle\"",
        render_svg((data(theme_df) + bar + x(:g) + y(:v) + theme(strip = "black")) | facet(:side)))
    @test occursin("fill=\"#3c3c46\" text-anchor=\"middle\"",
        render_svg((data(theme_df) + bar + x(:g) + y(:v)) | facet(:side)))
    @test occursin("gold", render_svg((data(theme_df) + bar + x(:g) + y(:v) +
        theme(strip = "navy", strip_text = "gold")) | facet(:side)))
    @refuses render_svg((data(theme_df) + bar + x(:g) + y(:v) +
        theme(strip_text = "gld")) | facet(:side)) "is not a color"

    # A preset is only a bundle of properties a caller could set themselves.
    @test render_svg(data(theme_df) + bar + x(:g) + y(:v) + theme("bw")) ==
          render_svg(data(theme_df) + bar + x(:g) + y(:v) +
                     theme(background = "white", frame = "full"))
    # The furniture goes black and white; the data does not.
    @test occursin("#",
        render_svg(data(theme_df) + bar + x(:g) + y(:v) + color(:g) + theme("bw")))
end

@testset "the engine's refusals arrive with the engine's own words" begin
    # A legality refusal belongs to `legality.rs`, not to this binding: every
    # binding must get the same one, which is what makes the rule the engine's.
    @refuses render_svg(data(df) + point + x(:x)) "gog:"
    @refuses render_svg(data(df) + point + x(:nope) + y(:y)) "nope"
end

@testset "limits state the domain when the data is not the authority" begin
    hrs = Dict("hour" => [1.0, 4, 7, 10, 13, 16, 19, 22],
               "n" => [2.0, 5, 9, 14, 20, 15, 8, 3])

    # The forcing case: a periodic axis cannot tell that a variable is periodic,
    # so the period is stated — and a stated end is flush, or the circle would
    # not close on it.
    @test occursin(">0</text>",
        render_svg(data(hrs) + line + x(:hour, limits = (0, 24)) + y(:n) + polar()))

    # Restricting is the instruction, so it draws and reports rather than
    # refusing — the one place this parts from `scale = "log"` at zero.
    @test occursin("<circle",
        render_svg(data(hrs) + point + x(:hour, limits = (0, 10)) + y(:n)))

    # A domain that keeps no row is the empty panel, and that is fatal.
    @refuses render_svg(data(hrs) + point + x(:hour, limits = (100, 200)) + y(:n)) "leaves no rows at all"

    # `limits` reaches every channel that measures, not only the axes (Law 1).
    @test render_svg(data(hrs) + point + x(:hour) + y(:n) + color(:n, limits = (0, 100))) !=
          render_svg(data(hrs) + point + x(:hour) + y(:n) + color(:n, limits = (0, 200)))

    # A category has no range to lie inside; the refusal points at `order`.
    @refuses render_svg(data(Dict("g" => ["a", "b"], "v" => [1.0, 2.0])) +
                        bar + x(:g, limits = (0, 5)) + y(:v)) "order(g)"

    # Caught in the binding, at the line that wrote it. `nothing` is Julia's
    # spelling of an end the data should decide, and `missing` is accepted too,
    # for the reader arriving from R's `NA`.
    @refuses x(:hour, limits = (20, 5)) "runs backwards"
    @refuses x(:hour, limits = (5,)) "needs two numbers"
    @test x(:hour, limits = (0, missing)).fields[:limits] == [0.0, nothing]

    # `shape` measures nothing, so it offers no domain either.
    @test_throws MethodError shape(:g, limits = (0, 1))

    # A domain on a temporal axis is written in dates, and the binding converts
    # them the way it converts the column — otherwise the two disagree and every
    # row falls outside.
    days = [Date(2024, 3, 1) + Day(i) for i in 0:19]
    dts = Dict("day" => days, "orders" => Float64.(20:39))
    year = render_svg(data(dts) + line + y(:orders) +
                      x(:day, limits = (Date(2024, 1, 1), Date(2024, 12, 31))))
    @test occursin(">Jan 2024</text>", year)
    @test occursin(">Nov 2024</text>", year)
end

@testset "the named ramps render as themselves, and limits center a diverging one" begin
    # The ruling: a diverging ramp has no midpoint parameter, because the middle
    # of a stated domain already is one. The data is one-sided (0..40), which is
    # what makes the two readings differ.
    signed = Dict("a" => [1.0, 2, 3, 4, 5], "b" => [1.0, 2, 3, 4, 5],
                  "d" => [0.0, 10, 20, 30, 40])
    fills(svg) = Set(m.captures[1] for m in eachmatch(r"<circle[^>]*fill=\"([^\"]*)\"", svg))

    for (name, dark) in [("magma", "#000004"), ("inferno", "#000004"),
                         ("plasma", "#0d0887"), ("cividis", "#00204d"),
                         ("gray", "#a9a9a9")]
        drawn = fills(render_svg(data(signed) + point + x(:a) + y(:b) +
                                 color(:d) + palette(name)))
        @test dark in drawn
        @test !("#8faed5" in drawn)   # never a silent fall back to the default
    end

    for name in ("blue_red", "brown_teal")
        drawn = fills(render_svg(data(signed) + point + x(:a) + y(:b) +
                                 color(:d, limits = (-40, 40)) + palette(name)))
        @test "#a9a9a9" in drawn      # zero sits on the neutral
        @test !("#004383" in drawn) && !("#6b3d10" in drawn)
    end

    # Unstated, the ramp fits the data instead, low end included.
    @test "#004383" in fills(render_svg(data(signed) + point + x(:a) + y(:b) +
                                        color(:d) + palette("blue_red")))

    # `gray` is in the vocabulary and `grey` is not — the American-English rule
    # enforced at the door rather than merely obeyed inside it.
    @refuses render_svg(data(signed) + point + x(:a) + y(:b) +
                        color(:d) + palette("grey")) "`gray`"

    # `soft` is the muted categorical set, and it reaches a *fill* — which is the
    # geometry it exists for, so testing it on a point would miss the point.
    cats = Dict("g" => ["a", "b", "a", "c"], "v" => [1.0, 2.0, 3.0, 4.0])
    bars = render_svg(data(cats) + bar * count + x(:g) + color(:g) + palette("soft"))
    @test occursin("#66c2a5", bars)
    @test !occursin("#4e79a7", bars)   # never a silent fall back to the default
end

# ---------------------------------------------------------------------------
# tick_count — how many ticks an axis aims for (spec §10)
#
# The last property that was real in the IR, read by the renderer, and reachable
# from no binding. It rides the binding beside `scale` and `limits` because it
# describes the **scale**; `theme()` declined it on that ground (§7).
# ---------------------------------------------------------------------------

@testset "tick_count states how many ticks an axis aims for" begin
    g5 = Dict("a" => [0.0, 25, 50, 75, 100], "b" => [1.0, 2, 3, 4, 5])
    ticks(p) = collect(m.match for m in eachmatch(r">[-0-9.]+</text>", render_svg(p)))

    # A target rather than a promise: the count picks a step and the step is then
    # rounded to a human number, so the claim is monotone rather than exact.
    few = ticks(data(g5) + point + x(:a, tick_count = 3) + y(:b))
    many = ticks(data(g5) + point + x(:a, tick_count = 11) + y(:b))
    @test length(many) > length(few)

    # Thinning the labels is not coarsening the step: a sparse axis's ticks are a
    # subset of a dense one's, so a value read off either is on the same scale.
    @test issubset(Set(few), Set(many))

    # A legend is not a short axis: `limits` reaches all six magnitude channels,
    # `tick_count` only the three that draw an axis.
    @test_throws MethodError color(:a, tick_count = 4)

    # Caught in the binding, at the line that wrote it.
    @refuses x(:a, tick_count = 1) "at least two ticks"
    @refuses x(:a, tick_count = 2.5) "not a whole number"
    @refuses x(:a, tick_count = "8") "needs one number"

    # A category axis has one tick per level, so the count is the data's.
    cats = Dict("g" => ["a", "b"], "v" => [1.0, 2.0])
    @refuses render_svg(data(cats) + bar + x(:g, tick_count = 5) + y(:v)) "order(g)"

    # One axis, one count — a layer stating its own is the plot-scoped-scale rule.
    @refuses render_svg(data(g5) + x(:a, tick_count = 4) + y(:b) +
                        point + x(:a, tick_count = 9)) "its own tick count"
end

# ---------------------------------------------------------------------------
# surface — the sheet through the samples (spec §15)
#
# The engine tests pin the mesh against the lattice; these pin the *binding*: that
# `surface` is exported, that a grid table reaches the engine as a grid, and that
# the refusals a reader will actually hit arrive with direction.
# ---------------------------------------------------------------------------

@testset "surface" begin
    # One row per (x, y) crossing is the mark's whole contract with the caller.
    side = [-3.0 + 6.0 * i / 14 for i in 0:14]
    gx = repeat(side, inner = length(side))
    gy = repeat(side, outer = length(side))
    r = [sqrt(a^2 + b^2) + 1e-9 for (a, b) in zip(gx, gy)]
    surf = Dict("gx" => gx, "gy" => gy, "h" => sin.(r) ./ r)
    faces(svg) = Base.count("<path d=\"M", svg)

    sheet = render_svg(data(surf) + surface + x(:gx) + y(:gy) + z(:h))
    @test faces(sheet) == 196   # a 15x15 grid of nodes is 14x14 faces

    # Binding `z` is what puts a plot in the cube, so a surface needs no `space()`
    # — and `space()` still sets the angle, which must change the picture.
    @test sheet != render_svg(data(surf) + surface + x(:gx) + y(:gy) + z(:h) +
                              space(110, 40))

    # The mesh lines: the seam hairline each face already carried, handed over.
    @test occursin("stroke=\"white\"",
                   render_svg(data(surf) + surface + x(:gx) + y(:gy) + z(:h) +
                              style(border_color = "white", border_size = 0.6)))

    # A flat surface is one failure, not two, and the direction names both routes
    # in plus the mark that draws the same field in the plane.
    @refuses render_svg(data(surf) + surface + x(:gx) + y(:gy)) "needs the cube"

    # A scatter is the empty panel this refusal exists to prevent.
    scat = Dict("sx" => [((i * 37) % 101) / 101 for i in 0:59],
                "sy" => [((i * 53) % 97) / 97 for i in 0:59],
                "sh" => [((i * 29) % 89) / 89 for i in 0:59])
    @refuses render_svg(data(scat) + surface + x(:sx) + y(:sy) + z(:sh)) "scatter rather than a grid"

    # And the sentence that refusal advises must draw: the field raised, no `z()`.
    est = render_svg(data(scat) + surface * density + x(:sx) + y(:sy) + space())
    @test faces(est) > 100

    # `bin` cuts the floor into adjacent cells and the sheet lays a flat lid on each —
    # the terraced surface, for a design that measures one value per cell. A 3x3 grid
    # read as *nodes* is 2x2 blocks of four corners, so four faces; read as cells it
    # is nine lids plus the twelve risers that connect them.
    tside = [-2.0, 0.0, 2.0]
    terr = Dict("ta" => [a for a in tside for _ in tside],
                "tb" => [b for _ in tside for b in tside])
    terr["tv"] = [a * a + b * b for (a, b) in zip(terr["ta"], terr["tb"])]
    @test faces(render_svg(data(terr) + surface + x(:ta) + y(:tb) + z(:tv))) == 4
    @test faces(render_svg(data(terr) + surface * bin(3) * mean +
                           x(:ta) + y(:tb) + z(:tv))) == 21

    # What is still refused is a floor of *slots*: categories leave air between them,
    # and tiles that float apart are not a sheet.
    @refuses render_svg(data(scat) + surface * count + x(:sx) + y(:sy) + space()) "surface * bin"

    # A face spans the gap between two samples; two categories have no gap to span.
    cats = merge(surf, Dict("band" => [isodd(i) ? "low" : "high" for i in 1:length(gx)]))
    @refuses render_svg(data(cats) + surface + x(:band) + y(:gy) + z(:h)) "bar * count"
end

# ---------------------------------------------------------------------------
# Polar — every mark that draws flat draws bent (spec §15)
#
# Five marks were refused in this space until 2026-07-26 on one recorded ground,
# *their straight edges would have to become arcs*. Three never needed one. What
# each test pins is the property the refusal was really about: a segment that
# **holds** a value across a span must follow the ring, since a chord falls
# inside the circle and puts the mark where the data is not.
# ---------------------------------------------------------------------------

@testset "polar: every mark that draws flat draws bent" begin
    wind = Dict("dir"    => ["N", "N", "N", "E", "E", "E", "S", "S", "S", "W", "W", "W"],
                "spd"    => [4.0, 5, 6, 8, 9, 11, 6, 7, 5, 3, 4, 2],
                "season" => ["Summer", "Winter", "Summer", "Winter", "Summer", "Winter",
                             "Summer", "Winter", "Summer", "Winter", "Summer", "Winter"])
    band = Dict("dir" => ["N", "E", "S", "W"],
                "lo"  => [2.0, 6, 4, 1], "hi" => [6.0, 11, 8, 5])
    arcs(svg) = length(collect(eachmatch(r" A ", svg)))

    for (name, p) in [
        ("step",     data(wind) + step * mean + x(:dir) + y(:spd) + polar()),
        ("interval", data(wind) + interval * range + x(:dir) + y(:spd) + polar()),
        ("box",      data(wind) + box + x(:dir) + y(:spd) + polar()),
        ("ribbon",   data(band) + ribbon * bounds(:lo, :hi) + x(:dir) + polar()),
        ("zone",     data(wind) + zone * count + x(:dir) + y(:season) + polar()),
    ]
        svg = render_svg(p)
        @test occursin("<svg", svg)
        @test !occursin("NaN", svg)
    end

    # The one genuinely new segment: a tread holds its value across a span of
    # angle, so it follows the ring. Flat, the same mark draws no arc at all.
    @test arcs(render_svg(data(wind) + step * mean + x(:dir) + y(:spd) + polar())) > 0
    @test arcs(render_svg(data(wind) + step * mean + x(:dir) + y(:spd))) == 0

    # A band's two boundaries run through the data's own vertices, which is
    # `line`'s geometry — the correction this made to the recorded refusal.
    @test arcs(render_svg(data(band) + ribbon * bounds(:lo, :hi) + x(:dir) + polar())) == 0

    # `bin(tiling = )`'s third refusal: a plane is what a tiling partitions, and
    # a bent plane has no distance for a hexagon to be regular against.
    mesh = Dict("a" => [Float64(i % 6) for i in 0:35],
                "b" => [Float64(i ÷ 6) for i in 0:35])
    @refuses render_svg(data(mesh) + zone * bin(tiling = "hex") + x(:a) + y(:b) + polar()) "rect"
    @test arcs(render_svg(data(mesh) + zone * bin(tiling = "rect") + x(:a) + y(:b) + polar())) > 0
end

# ---------------------------------------------------------------------------
# Space — the three slot marks stand on the cube's floor (spec §15)
#
# `interval` and `box` joined `bar` in the cube on 2026-07-26 and needed no
# ruling of their own: `is_slot_mark` had grouped the three since orientation
# was decided. The cube's remaining blanks are the other half — four *decided*
# refusals and two blocked on occlusion, and until this change every one of them
# said "not drawn yet".
# ---------------------------------------------------------------------------

@testset "space: the slot marks stand on the cube's floor" begin
    plots = Dict(
        "site"   => vcat(fill("North", 20), fill("Center", 20), fill("South", 20)),
        "season" => [isodd(i) ? "Wet" : "Dry" for i in 1:60],
        "yield"  => [50.0 + (i % 11) + (i <= 20 ? 0 : i <= 40 ? 8 : -4) for i in 1:60])

    for (name, p) in [
        ("interval", data(plots) + interval * range + x(:site) + y(:season) + z(:yield) + space()),
        ("conf",     data(plots) + interval * confidence + x(:site) + y(:season) + z(:yield) + space()),
        ("box",      data(plots) + box + x(:site) + y(:season) + z(:yield) + space()),
    ]
        svg = render_svg(p)
        @test occursin("<svg", svg)
        @test !occursin("NaN", svg)
    end

    # One per **cell**, not one per row: six cells, each a span plus a crossed cap
    # at either end — 6 x 5 = 30 strokes carrying a linecap.
    svg = render_svg(data(plots) + interval * range + x(:site) + y(:season) + z(:yield) + space())
    @test length(collect(eachmatch(r"stroke-linecap", svg))) == 30

    # A decided refusal gives its ruling and does not promise a renderer.
    @refuses render_svg(data(plots) + line + x(:yield) + y(:yield) + z(:yield) + space()) "no left to right"
    err = try
        render_svg(data(plots) + line + x(:yield) + y(:yield) + z(:yield) + space()); ""
    catch e
        sprint(showerror, e)
    end
    @test !occursin("not drawn yet", err)
    @test occursin("path", err)

    # The two blocked on occlusion say *that*, which is a different sentence.
    @refuses render_svg(data(plots) + rule + x(:yield) + z(:yield) + space()) "footprint"
end

@testset "the composed cut — bin supplies the cells, a statistic measures them" begin
    # Which transform owns the measurement when two are composed (spec §5).
    # `bin` says where the cells are *and* what is in them, and only the first is
    # what makes it a `bin`, so composed with a statistic it keeps the cut and
    # gives the tally up: the binned mean profile, and the summary heatmap one
    # dimension up.
    cut = render_svg(data(df) + bar * bin * mean + x(:x) + y(:y))
    @test occursin("<svg", cut)

    # Order cannot decide anything here — a cell has to exist before anything can
    # be measured in it, so the cut is prior rather than merely earlier.
    @test cut == render_svg(data(df) + bar * mean * bin + x(:x) + y(:y))

    # And the statistic has to reach the plot. Until 2026-07-26 it did not: `bin`
    # overwrote the named column with its own tally, the reduction handed that
    # straight back, and only the axis *title* changed. Geometry settles it, not
    # text — that is exactly what the old bug left untouched.
    strip_text(s) = replace(s, r"<text[^<]*</text>" => "")
    @test strip_text(cut) != strip_text(render_svg(data(df) + bar * bin + x(:x)))

    # The other two measure without cutting, so nothing is left of them to
    # compose — and each is refused for its own reason rather than a shared one.
    @refuses render_svg(data(df) + bar * count * mean + x(:group) + y(:y)) "measures each cell twice"
    @refuses render_svg(data(df) + bar * density * mean + x(:x) + y(:y)) "not a bucket holding rows"
    @refuses render_svg(data(df) + bar * bin * smooth + x(:x) + y(:y)) "asks one question twice"

    # Two synthesizing transforms: neither was handed a column, so neither gives way.
    # `proportion` left this class on 2026-07-26 — it rescales a measurement rather
    # than inventing one — so the pair here is `bin * count`.
    @refuses render_svg(data(df) + bar * bin * count + x(:x)) "neither was handed a column"
end

@testset "proportion normalizes, and stack(share = ) fills" begin
    # Read the drawn heights back as data values through the axis's own two ticks.
    # Comparing the bars *with each other* is the point: the defect behind this
    # session was twelve equal bars at 1/12, and the check that missed it read only
    # the axis range. A range is not a shape.
    function bar_values(svg)
        ticks = [(m[1], parse(Float64, m[2]), parse(Float64, m[3]))
                 for m in eachmatch(r"<text x=\"([0-9.]+)\" y=\"([0-9.]+)\">([0-9.]+)</text>", svg)]
        # The y ticks share an x; the x ticks share a y. Take the commonest x rather
        # than a pixel threshold, which a short x label slips under.
        tally = Dict{String,Int}()
        for t in ticks; tally[t[1]] = get(tally, t[1], 0) + 1; end
        axis = argmax(tally)
        on = filter(t -> t[1] == axis, ticks)
        per_px = (on[2][3] - on[1][3]) / (on[1][2] - on[2][2])
        hs = [parse(Float64, m[1]) for m in
              eachmatch(r"<rect[^>]*height=\"([0-9.]+)\"[^>]*fill-opacity", svg)]
        [h * per_px for h in hs if h != 12.0]      # drop legend swatches
    end

    share = Dict(
        :dir => vcat(fill("N", 6), fill("E", 10), fill("S", 4), fill("W", 20)),
        # Uneven inside each slot as well as between them: an alternating split makes
        # every slot 50/50, which a fill that ignored the values would also draw.
        :season => vcat(fill("Su", 4), fill("Wi", 2), fill("Su", 3), fill("Wi", 7),
                        fill("Su", 1), fill("Wi", 3), fill("Su", 15), fill("Wi", 5)),
        :v => Float64.(1:40))
    # Skewed on purpose: a uniform column binned evenly gives near-equal bars, the
    # one shape this test must be able to tell apart from the 1/12 defect.
    skew = Dict(:v => [round(exp(i * 4.6 / 199)) for i in 0:199])

    # 1. Unchanged: a bare `proportion` sums to 1.
    @test abs(Base.sum(bar_values(render_svg(data(share) + bar * proportion + x(:dir)))) - 1) < 0.01

    # 2. The fix. A `color` split used to give each group its own denominator, so
    #    the plot summed to 2 — two conditional distributions, where §5 had always
    #    said the word means a share of the whole frame (Law 6).
    split = Base.sum(bar_values(render_svg(
        data(share) + bar * proportion + x(:dir) + color(:season))))
    @test abs(split - 1) < 0.01

    # 3. The relative-frequency histogram, refused for one day as two synthesizing
    #    transforms. The bars must *differ* — all-equal is the 1/12 defect itself.
    h = bar_values(render_svg(data(skew) + bar * bin(12) * proportion + x(:v)))
    @test length(h) == 12
    @test abs(Base.sum(h) - 1) < 0.01
    @test length(unique(round.(h, digits = 3))) > 1
    n = bar_values(render_svg(data(skew) + bar * bin(12) + x(:v)))
    @test Base.maximum(abs.(n ./ Base.sum(n) .- h)) < 0.01

    # 4. `stack(share = true)` fills every pile to exactly 1, whatever measured it.
    tops = bar_values(render_svg(data(share) + bar * count * stack(share = true) +
                                 x(:dir) + color(:season)))
    half = length(tops) ÷ 2
    for i in 1:half
        @test abs(tops[i] + tops[i + half] - 1) < 0.01
    end
    @test length(unique(round.(tops, digits = 3))) > 1
    # It composes with any measurement, which is why it is a `stack` parameter and
    # not a second reading of `proportion`: there is no column for `proportion` to sum.
    render_svg(data(share) + bar * sum * stack(share = true) + x(:dir) + y(:v) + color(:season))
    @refuses stack(share = 1) "is true or false"

    # `stack(baseline = )` says where the pile hangs — the streamgraph. A displaced
    # pile draws no numbers on the measure axis, because no value on it corresponds
    # to a measurement once the foot has moved.
    flows = (t = repeat(1.0:6.0, 3),
             g = vcat(fill("a", 6), fill("b", 6), fill("c", 6)),
             v = [4.0, 9, 3, 8, 2, 7, 5, 5, 5, 5, 5, 5, 2, 3, 9, 2, 8, 3])
    draw(tr) = render_svg(data(flows) + area * tr + x(:t) + y(:v) + color(:g))
    ticks(s) = filter(t -> occursin(r"^-?[0-9.]+$", t),
                      [m.captures[1] for m in eachmatch(r">([^<>]+)</text>", s)])
    plain, strm = draw(stack), draw(stack(baseline = "wiggle"))
    @test length(ticks(plain)) > length(ticks(strm))
    @test !isempty(ticks(strm))
    # Displacing moves the pile; it never changes a thickness, so the band count holds.
    # `Base.count`, because `count` here is the transform atom — the masking the
    # package's own attach message warns about, met in its own test suite.
    @test Base.count("<polygon", plain) == Base.count("<polygon", strm)
    @refuses stack(baseline = 1) "is one of"
    @refuses draw(stack(baseline = "sym")) "is not a baseline"
    @refuses render_svg(data(flows) + area * stack(baseline = "center") + x(:t) + y(:v) +
                        color(:g) + polar()) "no origin to spare"

    # A *composed* `proportion` synthesizes nothing, so its `y` names an input
    # column and a misspelling of it must still be caught. Found by a reader
    # looking at a plot: `bar * sum * proportion + y(pop)` — `pop` renamed
    # `population` in the book's own data — drew an empty panel on 0..1 axes.
    @refuses render_svg(data(share) + bar * sum * proportion + x(:dir) + y(:nosuchcolumn)) "not in the data"
    # …while a bare `proportion` still names the column it writes.
    render_svg(data(share) + bar * proportion + x(:dir) + y(:whatever))
end

# --- the violin: the slot reading of `density` (spec §5) ---------------------
#
# Not a new mark, and the test says so by drawing it with the two that already
# exist: `ribbon` closes on its own reflection, `area` on the slot's center line.
@testset "the violin — a density per slot, drawn as a width" begin
    viol = Dict("grp" => vcat(fill("wide", 40), fill("narrow", 10)),
                "v"   => repeat(0.0:9.0, 5))
    # `findall` rather than `count`, which this package deliberately shadows with
    # the transform of that name.
    npolys(spec) = length(findall("<polygon", render_svg(spec)))

    @test npolys(data(viol) + ribbon * density + x(:grp) + y(:v)) == 2
    @test npolys(data(viol) + area * density + x(:grp) + y(:v)) == 2
    # Lying down, the orientation read off the bindings — the form with room for
    # long category names, exactly as `box + x(pay) + y(dept)` is.
    @test npolys(data(viol) + ribbon * density + x(:v) + y(:grp)) == 2

    # `compare` chooses what the widths mean between slots, and must change the plot.
    counted = render_svg(data(viol) + ribbon * density + x(:grp) + y(:v))
    shaped  = render_svg(data(viol) + ribbon * density(compare = "shape") + x(:grp) + y(:v))
    @test counted != shaped
    @refuses render_svg(data(viol) + line * density(compare = "count") + x(:v)) "no slots"
    @refuses render_svg(data(viol) + ribbon * density(compare = "area") +
                        x(:grp) + y(:v)) "not a reading this engine has"
    # The curve is still not a band: a `ribbon` needs two boundaries, and one
    # estimate along a continuous axis gives it one.
    @refuses render_svg(data(viol) + ribbon * density + x(:v)) "violin"
end

@testset "the ridgeline — the half violin laid down, overlapped and traced" begin
    viol = Dict("grp" => vcat(fill("wide", 40), fill("narrow", 10)),
                "v"   => repeat(0.0:9.0, 5))
    npolys(spec) = length(findall("<polygon", render_svg(spec)))

    @test npolys(data(viol) + area * density(reach = 2.5) + x(:v) + y(:grp)) == 2
    traced = render_svg(data(viol) + line * density + x(:v) + y(:grp))
    @test !occursin("<polygon", traced)
    @test occursin("<path", traced)
    @test render_svg(data(viol) + area * density(reach = 2.5) + x(:v) + y(:grp)) !=
          render_svg(data(viol) + area * density + x(:v) + y(:grp))
    @refuses render_svg(data(viol) + line * density(reach = 2) + x(:v)) "no slots"
    @refuses density(reach = -1) "positive number"
end

# ---------------------------------------------------------------------------
# Nest — the panel packed with regions (spec §15)
#
# The third answer to what carries a share: length flat, angle in polar, area
# here. What is checked is the property a treemap is read for — the regions are
# the panel and each is its own share of it — plus the refusals the space owns.
# ---------------------------------------------------------------------------

@testset "nest: the measure becomes an area, and the areas are the panel" begin
    sales = Dict("region"  => ["North", "North", "South", "South", "East", "East", "West"],
                 "product" => ["widgets", "gadgets", "widgets", "gadgets",
                               "widgets", "gadgets", "widgets"],
                 "revenue" => [32.0, 14, 25, 8, 19, 11, 6])

    # Every packed cell as (x, y, w, h). The legend's swatches carry `rx=` and the
    # outer region outlines are `fill="none"`; neither is a cell. The leading
    # space in each key matters — without it `width=` also matches `stroke-width=`.
    function cells(svg)
        out = NTuple{4,Float64}[]
        for line in split(svg, "\n")
            (occursin("<rect", line) && occursin("fill-opacity", line)) || continue
            (occursin("rx=", line) || occursin("fill=\"none\"", line)) && continue
            push!(out, Tuple(parse(Float64, split(split(line, " $k=\"")[2], "\"")[1])
                             for k in ("x", "y", "width", "height")))
        end
        out
    end

    one = render_svg(data(sales) + bar * GrammarOfGraphics.sum + y(:revenue) +
                     color(:region) + nest())
    cl = cells(one)
    @test length(cl) == 4
    total = Base.sum(c[3] * c[4] for c in cl)
    shares = sort([c[3] * c[4] / total for c in cl])
    # North 46, South 33, East 30, West 6 — of 115.
    for (got, want) in zip(shares, sort([6, 30, 33, 46] ./ 115))
        @test abs(got - want) < 0.002
    end

    # No axes at all, which is the space's defining property.
    @test !occursin("stroke=\"#5a5a64\"", one)
    flat_one = render_svg(data(sales) + bar * GrammarOfGraphics.sum + x(:region) +
                          y(:revenue) + color(:region))
    @test occursin("stroke=\"#5a5a64\"", flat_one)

    # A bound position packs a second level inside each region.
    two = render_svg(data(sales) + bar * GrammarOfGraphics.sum + x(:region) + y(:revenue) +
                     color(:product) + nest())
    # `Base.count`, not gog's `count` atom — the same masking `sum` above needs
    # qualifying for, and the collision Law 3 accepts by design.
    outlines(s) = Base.count(l -> occursin("<rect", l) && occursin("fill=\"none\"", l), split(s, "\n"))
    @test outlines(two) == 4
    @test outlines(one) == 0

    # The space's own refusals.
    @refuses render_svg(data(sales) + bar * GrammarOfGraphics.sum * stack + y(:revenue) +
                        color(:region) + nest()) "own region"
    @refuses render_svg(data(sales) + bar * GrammarOfGraphics.sum + y(:revenue) +
                        color(:region) + nest() + x_label("Revenue")) "names an axis"
    @refuses render_svg(data(sales) + point + x(:revenue) + y(:revenue) + nest()) "placed by a position"
    @refuses render_svg(data(sales) + bar * GrammarOfGraphics.sum + y(:revenue, scale = "log") +
                        color(:region) + nest()) "share of the total"
    @refuses nest(90) "takes no arguments"

    # A label at the center of its own region — what makes a packing readable
    # once the split is too wide for a legend to decode (2026-07-27). The label
    # layer needs no `x`: a packing places by region, Law 7's third relaxation.
    packed = render_svg(data(sales) + bar + y(:revenue) + color(:region) +
                        text + label(:product) + nest())
    # A mark's label carries `fill-opacity` and the legend's key entries do not —
    # the same discriminator `cells` uses one element over, and needed for the
    # same reason: the key spells out the very strings the labels draw, so
    # counting those would pass whether or not the mark drew anything.
    names = [l for l in split(packed, "\n")
             if startswith(strip(l), "<text") && occursin("fill-opacity", l)]
    @test !isempty(names)
    # Every drawn label sits inside a cell the bar drew, which is the property
    # that makes the mark worth having: the two marks read one packing, so a name
    # cannot land in a rectangle its own row did not get.
    boxes = cells(packed)
    for row in names
        lx = parse(Float64, split(split(row, "<text x=\"")[2], "\"")[1])
        @test any(b -> b[1] <= lx <= b[1] + b[3], boxes)
    end

    @refuses render_svg(data(sales) + bar + y(:revenue) + color(:region) +
                        text + label(:product) + style(nudge = "up") + nest()) "covers no point"
end

# ---------------------------------------------------------------------------
# Composition — separate plots arranged on one page (spec §11)
#
# `|` and `/` between two *plots* is a page; between a plot and `facet()` it is
# still a split. In Julia that distinction is literally multiple dispatch, which
# is the closest any of the four bindings comes to saying the design out loud.
#
# The engine's one rule does the rest: the same column on the same axis in two
# composed plots is one axis — one scale, one panel extent, drawn once.
# ---------------------------------------------------------------------------

@testset "plots compose onto a page, and a shared column is one axis" begin
    cars = Dict(
        "speed" => [4, 4, 7, 7, 8, 9, 10, 10, 10, 11, 11, 12, 12, 12, 12, 13, 13, 13,
                    13, 14, 14, 14, 14, 15, 15, 15, 16, 16, 17, 17, 17, 18, 18, 18,
                    18, 19, 19, 19, 20, 20, 20, 20, 20, 22, 23, 24, 24, 24, 24, 25],
        "dist" => [2, 10, 4, 22, 16, 10, 18, 26, 34, 17, 28, 14, 20, 24, 28, 26, 34,
                   34, 46, 26, 36, 60, 80, 20, 26, 54, 32, 40, 32, 40, 50, 42, 56, 76,
                   84, 36, 46, 68, 32, 48, 52, 56, 64, 66, 54, 70, 92, 93, 120, 85])
    scatter() = data(cars, name = "cars") + point + x(:speed) + y(:dist)
    top_hist() = data(cars, name = "cars") + bar * bin + x(:speed) + theme(height = 120)
    side_hist() = data(cars, name = "cars") + bar * bin + y(:dist) + theme(width = 120)

    page = top_hist() / (scatter() | side_hist())
    @test page isa Page
    svg = render_svg(page)
    @test length(collect(eachmatch(r"<svg", svg))) == 4

    # The panels of the two plots sharing `speed` run over the same pixels — the
    # whole promise of a marginal plot, and the reason it is not just two plots.
    panels = [parse(Float64, m[1]) for m in eachmatch(
        r"<rect x=\"([0-9.]+)\" y=\"[0-9.]+\" width=\"[0-9.]+\"[^>]*fill=\"#f5f5f8\"", svg)]
    @test abs(panels[1] - panels[2]) < 0.01
    # And the shared axis is drawn once, by the plot nearest the edge it lives on.
    @test length(collect(eachmatch(r">Speed<", svg))) == 1

    # Unrelated plots are only arranged.
    apart = render_svg(scatter() | (data(cars, name = "cars") + bar * bin + x(:dist)))
    @test length(collect(eachmatch(r"<svg", apart))) == 3

    # `theme(width =, height =)` sizes the image alone and the cell composed.
    alone = render_svg(data(cars, name = "cars") + point + x(:speed) + y(:dist) +
                       theme(width = 400, height = 300))
    @test occursin("width=\"400\" height=\"300\"", alone)

    # And a *page* states its own size, which is the one sentence no cell can
    # write. Composed side by side, two plots divide the page's width and each
    # keep the whole of its height, so only the page can say how much that is.
    sized_page = render_svg((scatter() | scatter()) + theme(height = 310))
    @test occursin("width=\"800\" height=\"310\"", sized_page)
    @test occursin("width=\"800\" height=\"600\"", render_svg(scatter() | scatter()))

    @refuses theme(width = 10) "at least 40"
    @refuses (scatter() | scatter()) | facet(:speed) "faceted a page"
    @refuses (scatter() | scatter()) + title("Cars") "belongs to a plot"
    # The size is the only theme property whose subject is the figure. Every
    # other one describes a panel, and a page has none.
    @refuses (scatter() | scatter()) + theme(grid = "none") "describes a panel"
    @refuses (scatter() | scatter()) + theme("minimal") "describes a panel"
    @refuses render_svg(
        (data(cars, name = "cars") + point + x(:speed) + y(:dist) + theme(height = 500)) /
        (data(cars, name = "cars") + point + x(:speed) + y(:dist) + theme(height = 500))
    ) "ask for 1000px"
end

@testset "partition — a hierarchy in columns, one ring per level" begin
    budget = Dict(
        :group => ["A", "A", "A", "B"],
        :item => ["p", "q", "q", "r"],
        :detail => ["", "deep", "also", ""],
        :amount => [4.0, 3.0, 3.0, 10.0],
    )
    sun = render_svg(data(budget, name = "budget") +
                     zone * partition(:group, :item, :detail) +
                     x(:amount) + color(:group) + polar())
    @test occursin("<path", sun)
    # The same sentence flat is the icicle — one coordinate space apart, which is
    # the whole derivation.
    icicle = render_svg(data(budget, name = "budget") +
                        zone * partition(:group, :item, :detail) +
                        x(:amount) + color(:group))
    @test occursin("<rect", icicle)
    @test sun != icicle
    # The second reader: `text` takes the center the same computation published.
    named = render_svg(data(budget, name = "budget") +
                       zone * partition(:group, :item, :detail) + x(:amount) +
                       text * partition(:group, :item, :detail) + label(:name) + polar())
    @test occursin(">deep<", named)

    @refuses render_svg(data(budget, name = "budget") +
                        bar * partition(:group, :item) + x(:amount)) "no reading for a region"
    @refuses partition() "outermost first"
    mixed = Dict(:group => ["A", "A"], :item => ["", "p"], :amount => [5.0, 5.0])
    @refuses render_svg(data(mixed, name = "mixed") +
                        zone * partition(:group, :item) + x(:amount)) "value of its own"
end

@testset "partition(cross = true) is the mosaic" begin
    # One parameter apart from the icicle, and it buys the whole plot: the levels
    # turn across each other instead of running down one axis. The engine pins the
    # arithmetic; here that the sentence draws and that crossing is visible in the
    # output rather than silently ignored.
    counts = (decade = ["1950s", "1950s", "1960s", "1960s"],
              theme  = ["Heartbreak", "Love", "Heartbreak", "Love"],
              n      = [10.0, 10.0, 30.0, 40.0])
    mosaic = render_svg(data(counts, name = "counts") + x(:n) +
                        zone * partition(:decade, :theme; cross = true) + color(:theme))
    nested = render_svg(data(counts, name = "counts") + x(:n) +
                        zone * partition(:decade, :theme) + color(:theme))
    @test occursin("<rect", mosaic)
    @test mosaic != nested
    @test occursin("Share of column", mosaic)

    # The labeling idiom, carried over from the sunburst: a shallower partition
    # of the same table lands its nodes in the same columns.
    labeled = render_svg(data(counts, name = "counts") + x(:n) +
                          zone * partition(:decade, :theme; cross = true) + color(:theme) +
                          text * partition(:decade; cross = true) + label(:name))
    @test occursin(">1960s<", labeled)

    # The settable rule spans a setting across its geometry class, and `zone` joined
    # the closed-glyph fills on 2026-07-27 because a mosaic without cell edges is
    # one blob wherever two neighbors share a color. Refused until that day.
    edged = render_svg(data(counts, name = "counts") + x(:n) +
                       zone * partition(:decade, :theme; cross = true) + color(:theme) +
                       style(border_color = "white", border_size = 2))
    @test occursin("stroke=\"white\"", edged)
    @test !occursin("stroke=\"white\"", mosaic)
end

# ---------------------------------------------------------------------------
# query() — the table that is not in memory
#
# The guard is the one that matters and it is the same in all four bindings: the
# *same sentence*, over a materialized table and over a query returning the same
# rows, must render byte-identical SVG. If those diverge, `query()` has stopped
# being a way of naming rows and become a second way of drawing them.
#
# SQLite and DBInterface are `[extras]`, so they resolve under `Pkg.test()` and
# not under a direct `julia test/runtests.jl`. The byte-identity half therefore
# skips when they are absent, exactly as R's skips without DBI — the package
# itself depends on neither, and `resolve_query` looks DBInterface up in the
# session rather than importing it.
# ---------------------------------------------------------------------------

const HAVE_DB = try
    @eval using SQLite, DBInterface
    true
catch
    false
end

@testset "query() — a table that lives in a database" begin
    rows = (status  = ["open", "shipped", "shipped", "closed", "open", "refunded"],
            revenue = [120.0, 240.5, 95.25, 310.75, 60.0, 45.0])
    frame = Dict("status" => collect(rows.status), "revenue" => collect(rows.revenue))

    if HAVE_DB
        db = SQLite.DB()
        DBInterface.execute(db, "CREATE TABLE orders (status TEXT, revenue REAL)")
        for (s, r) in zip(rows.status, rows.revenue)
            DBInterface.execute(db, "INSERT INTO orders VALUES (?, ?)", (s, r))
        end
        sql = "SELECT status, revenue FROM orders"

        for (label, sentence) in [
            ("point with two positions", t -> t + point + x(:revenue) + y(:status)),
            ("bar * count",              t -> t + bar * count + x(:status)),
            ("bar with a mapped color",
                t -> t + bar + x(:status) + y(:revenue) + color(:status)),
        ]
            from_table = render_svg(sentence(data(frame, name = "orders")))
            from_query = render_svg(sentence(query(db, sql, name = "orders")))
            @test from_table == from_query
        end
    else
        @info "SKIP: query() byte-identity needs SQLite and DBInterface (both [extras])"
    end

    # The query does not run when the sentence is written. An eager query would
    # foreclose pushing the transform down, since the planner has to see the
    # whole sentence before it knows what to ask the database for.
    lazy = query(nothing, "SELECT nonsense FROM nowhere", name = "orders")
    @test lazy.frames["orders"] isa GrammarOfGraphics.Query

    # `query("SELECT ...")` is the mistake `data()` invites, that atom taking one
    # argument. All four bindings answer it with the fix rather than the host
    # language's own arity error — here Julia's `MethodError`.
    err = try; query("SELECT 1"); catch e; e; end
    @test err isa GogError
    @test occursin("connection first", sprint(showerror, err))

    err = try; query(nothing, 123); catch e; e; end
    @test err isa GogError
    @test occursin("SELECT as text", sprint(showerror, err))
end

@testset "parentheses group plots, never marks" begin
    # `+` with a Plot on its right keeps the table and returns, which is right for
    # a bare `data(df)` and silent loss for `(data(df) + point + area)`: the marks
    # inside stopped existing and the plot rendered as though they were never
    # named. That is the dropped binding §12 forbids, and a sub-expression that
    # means one thing alone and nothing in context breaks Law 6.
    note = Dict("x" => [2.0], "y" => [5.0])
    base() = data(df, name = "df") + x(:x) + y(:y) + line

    @refuses base() + (data(note, name = "note") + point + area) "parentheses do not group marks"
    @refuses base() + (data(note, name = "note") + point + area) "note"
    @refuses base() + (data(note, name = "note") + point + area) "`|` and `/`"

    # Not only marks: a position or a title inside the parentheses was dropped too.
    @refuses base() + (data(note, name = "note") + x(:x)) "parentheses do not group marks"
    @refuses base() + (data(note, name = "note") + title("hi")) "parentheses do not group marks"

    # A bare `data()` carries nothing, so it still joins mid-sentence.
    seq = base() + data(note, name = "note") + point
    @test length(seq.spec["layers"]) + (seq.current_layer === nothing ? 0 : 1) == 2

    # Composition is what this refusal must not break.
    @test (data(df, name = "df") + point + x(:x) + y(:y)) |
          (data(df, name = "df") + line + x(:x) + y(:y)) isa Page
    @test (data(df, name = "df") + point + x(:x) + y(:y)) /
          (data(df, name = "df") + line + x(:x) + y(:y)) isa Page
end

# --- book_table(): the manual's tables, without a CSV reader to copy ---------
#
# Binding plumbing rather than a word of the grammar, which is why
# `book/check_vocabulary.R` excludes it from the kernel block beside
# `render_svg`. The offline checks always run; the fetch is guarded, because a
# suite has to pass on a laptop with no network.
@testset "book_table" begin
    @test GrammarOfGraphics.BOOK_DATA_URL ==
          "https://psychometrician.github.io/gog-book/data/"

    raw = ["1" "01" "x"; "2" "02" "y"]
    typed = GrammarOfGraphics._columns(raw, ["n", "s", "w"], ["s"])
    @test typed["n"] == [1.0, 2.0]          # every value a number, so numbers
    @test typed["s"] == ["01", "02"]        # named in `text`, so left alone
    @test typed["w"] == ["x", "y"]          # not numbers, so text

    fetched = try
        book_table("gapminder_2007")
    catch err
        @info "SKIP: book_table() live fetch" err = typeof(err)
        nothing
    end
    if fetched !== nothing
        @test length(fetched["country"]) == 142
        @test eltype(fetched["gdp"]) <: Real
        @test eltype(fetched["continent"]) <: AbstractString
    end
    # -----------------------------------------------------------------------
    # brush — the selection
    #
    # Four claims, and the second is the one the whole feature rests on: a plot
    # that names no brush must be exactly the plot it was before selection
    # existed.
    # -----------------------------------------------------------------------
end

@testset "brush" begin
    d = Dict("v" => [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
             "w" => [2.0, 4.0, 1.0, 5.0, 3.0, 6.0],
             "kind" => ["a", "a", "b", "b", "c", "c"])
    svg = render_svg(data(d; name = "bt") + point + x(:v) + y(:w) +
                     brush(:v, at = (2.5, 4.5)))
    @test occursin("<g opacity=\"0.150\">", svg)
    # A brush highlights; it never removes rows. That is what separates it
    # from `limits`, and it is the claim a reader is most likely to test.
    @test length(collect(eachmatch(r"<circle", svg))) == 6

    plain = render_svg(data(d; name = "bt") + point + x(:v) + y(:w))
    @test !occursin("data-gog-panel", plain)
    @test !occursin("<g opacity=", plain)

    cat = render_svg(data(d; name = "bt") + point + x(:v) + y(:w) +
                     brush(:kind, at = "b"))
    @test occursin("<g opacity=\"0.150\">", cat)

    line_msg = try
        render_svg(data(d; name = "bt") + line + x(:v) + y(:w) +
                   brush(:v, at = (2.0, 4.0)))
        ""
    catch e
        sprint(showerror, e)
    end
    @test occursin("one shape through many rows", line_msg)
    @test occursin("group()", line_msg)

    @test_throws GogError brush(:v, at = (1, 2, 3))
end

# A composed page of cubes carries the engine. A `Page` writes its list as
# `cells`, and this check read only `plots`, so it answered false for every
# composition of 3-D plots and shipped no engine: the page drew perfectly and
# would not turn. Python had the same gap; R and JavaScript read both spellings,
# which is what made two bindings look right while two were not.
@testset "a composed page of cubes carries the engine" begin
    t = (a = [1.0, 2.0], b = [1.0, 2.0], c = [1.0, 2.0])
    cube() = data(t) + point + x(:a) + y(:b) + z(:c) + space()
    spec, _ = GrammarOfGraphics.wire(cube() | cube())
    @test GrammarOfGraphics.needs_engine(spec)
    @test GrammarOfGraphics.needs_engine(spec["cells"][1])
    @test !GrammarOfGraphics.needs_engine(Dict("cells" => [Dict("layers" => [])]))
end

# The interactive block must reach the browser intact. Not reachable by comparing
# SVG: that path is the CLI's and is perfect, while the browser gets a separate
# payload nothing checked. A `data:` module import is refused by a
# content-security policy, silently, because a blocked import throws nothing.
@testset "interactive block is policy-safe" begin
    t = Dict("gdp" => [1000.0, 20000.0, 40000.0], "life" => [50.0, 70.0, 80.0])
    p = data(t; name = "t") + point + x(:gdp) + y(:life) +
        brush(:gdp, at = [2000, 30000])
    block = svg_block(render_svg(p), p)

    # No script means the browser engine was never built, which is the normal
    # state in CI. There is nothing to assert about a block that does not exist.
    if !occursin("<script", block)
        @info "SKIP: browser engine not built, so the interactive block cannot be checked"
    else
        @test !occursin("data:text/javascript", block)
        @test !occursin("data:application/wasm", block)
        @test !occursin("from \"./view.js\"", block)
        @test occursin("function mountView", block)   # the module is here, inline
        @test occursin("atob(", block)                # the engine travels as bytes
    end
end


# The engine beside the package is the package's own.
#
# Eight declarations agreeing says nothing about the binary that draws. They are
# separate artifacts and they went out of step exactly once it mattered: a
# package carried an engine a whole release behind its own manifest, and nothing
# in this repository could see it. Not the version guard, which reads files; not
# the parity harness, which drew all 740 sentences of the manual through both
# engines and found them identical, because two builds a patch apart agree on
# every sentence that did not change between them. Bytes cannot answer it
# either: an engine compiled inside an installed package hashes differently from
# the same sources built in a checkout, because the build path travels in the
# binary.
#
# The version is read out of `Project.toml` with a regular expression rather
# than with `TOML`, to keep the test environment free of a dependency the
# package itself does not carry, and `pkgversion` needs Julia 1.9 while this
# package supports 1.6.
@testset "engine version" begin
    declared = only(match(r"^version *= *\"([^\"]+)\""m,
                          read(joinpath(@__DIR__, "..", "Project.toml"), String)).captures)

    engine = find_gog_cli()

    # `devnull` on stdin is not tidiness. An engine older than the flag does not
    # reject `--version`; it ignores the argument and blocks reading stdin
    # forever, since stdin is how a plot arrives. The obvious spelling of this
    # check hangs on exactly the engine it exists to catch.
    reported = strip(read(pipeline(`$engine --version`, stdin = devnull), String))

    @test occursin(r"^\d+\.\d+\.\d+", reported)
    if occursin(r"^\d+\.\d+\.\d+", reported)
        @test reported == declared
    else
        @info "the engine cannot say which version it is; rebuild it" engine reported
    end
end

@testset "a page of tables the binding had to name itself" begin
    # Julia cannot read the name a table was bound to, so the binding invents
    # `data` for every one. That name is its own and means nothing to the
    # author, so on a page the second gives way rather than colliding — the same
    # rule a plot of two tables already follows.
    left = Dict{String,Any}("x" => [1.0, 2.0], "y" => [3.0, 4.0])
    right = Dict{String,Any}("x" => [3.0, 4.0], "y" => [5.0, 6.0])

    bare = (data(left) + point + x(:x) + y(:y)) | (data(right) + point + x(:x) + y(:y))
    named = (data(left, name = "one") + point + x(:x) + y(:y)) |
            (data(right, name = "two") + point + x(:x) + y(:y))
    @test length(bare.frames) == 2
    # The picture is the test: a rename that pointed both cells at one table
    # would draw too, and only this catches that.
    @test render_svg(bare) == render_svg(named)

    # A name the author wrote still cannot be moved.
    @test_throws GogError (data(left, name = "s") + point + x(:x) + y(:y)) |
                          (data(right, name = "s") + point + x(:x) + y(:y))
end

@testset "a refused plot leaves an existing file alone" begin
    # `save()` used to open the destination before it knew the render had
    # succeeded, and opening for writing truncates — so a refusal emptied
    # whatever was already there. A refusal must cost nothing on disk.
    gm = Dict{String,Any}("gdp" => [1.0, 2.0, 3.0], "life" => [4.0, 5.0, 6.0])
    good = data(gm) + point + x(:gdp) + y(:life)
    bad = data(gm) + point + x(:gdp) + y(:life) + palette("okabe")   # nothing maps color

    path = joinpath(mktempdir(), "plot.svg")
    save(good, path)
    before = read(path, String)
    @test !isempty(before)

    @test_throws GogError save(bad, path)
    @test read(path, String) == before
end

@testset "a refusal in a notebook cell reads as the message, not as a crash" begin
    # Thrown at the frontend, a refusal arrives as twenty-odd frames of
    # `limitstringmime` and `eventloop`, with the one useful line buried. The
    # display hooks show it instead; `render_svg` still throws, so a script and
    # every check that reads an exit code are unaffected.
    frame = Dict{String,Any}("gdp" => [1.0, 2.0], "life" => [3.0, 4.0])
    refused = data(frame) + point + x(:gdp) + y(:life) + palette("okabe")
    drawn = data(frame) + point + x(:gdp) + y(:life)

    html = repr("text/html", refused)
    @test occursin("palette()", html)
    @test !occursin("<div", html)
    # The message contains `color(<column>)`, so an unescaped `<` would be eaten
    # as a tag and the reader would lose the half naming the fix.
    @test occursin("&lt;column&gt;", html)

    # The SVG form answers the question it was asked: a host wanting a picture
    # is handed one, carrying the message.
    svg = repr("image/svg+xml", refused)
    @test startswith(svg, "<svg")
    @test occursin("palette()", svg)

    @test startswith(repr("text/html", drawn), "<div")
    @test_throws GogError render_svg(refused)
end

# ---------------------------------------------------------------------------
# range() — the band's two ends, as quantile probabilities
# ---------------------------------------------------------------------------

@testset "range takes a quantile band" begin
    frame = (g = fill("a", 10), v = Float64.(1:10))
    band = render_svg(data(frame) + interval * range(0.25, 0.75) + x(:g) + y(:v))
    whole = render_svg(data(frame) + interval * range + x(:g) + y(:v))
    @test band != whole
    # 1..10 by type 7: Q1 = 3.25 and Q3 = 7.75, the numbers `quantile` returns.
    @test occursin(">4</text>", band)
    @test !occursin(">10</text>", band)
    # Bare `range` is the whole group, which is what it has always drawn.
    @test occursin(">10</text>", whole)

    @test_throws GogError range(0.5, 1.5)
    @test_throws GogError range(-0.1)
    @test_throws GogError range("a")
    @test_throws GogError range(0.25, 0.75, 0.9)
    @test_throws GogError render_svg(
        data(frame) + interval * range(0.75, 0.25) + x(:g) + y(:v))
end

# ---------------------------------------------------------------------------
# deviation and quantile — the family's two newest members
# ---------------------------------------------------------------------------

@testset "deviation bands the spread, quantile needs its probability" begin
    frame = (g = fill("a", 8), v = Float64[2, 4, 4, 4, 5, 5, 7, 9])
    one_sd = render_svg(data(frame) + interval * deviation + x(:g) + y(:v))
    two_sd = render_svg(data(frame) + interval * deviation(2) + x(:g) + y(:v))
    @test one_sd != two_sd
    # A spread band and the mean's interval are different questions and must
    # draw differently, which is the whole reason both atoms exist.
    @test one_sd != render_svg(data(frame) + interval * confidence + x(:g) + y(:v))
    @test_throws GogError deviation(0)

    q90 = render_svg(data(frame) + bar * quantile(0.9) + x(:g) + y(:v))
    @test q90 != render_svg(data(frame) + bar * median + x(:g) + y(:v))
    # No default, because the sensible one is already `median`.
    @test_throws GogError render_svg(data(frame) + bar * quantile + x(:g) + y(:v))
    @test_throws GogError quantile(1.5)
    @test_throws GogError quantile(-0.1)
end
