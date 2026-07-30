# columns.jl — how a Julia expression names a column
#
# This is the module Julia barely needs, and saying why is the point. Spec §8
# ("the cross-language wrinkles"): every binding hands the engine a column
# *name*, and each language captures a bare name its own way.
#
#     R       data(gapminder) + point + x(gdp)      + y(life)
#     Julia   data(gapminder) + point + x(:gdp)     + y(:life)
#     Python  data(gapminder) + point + x(col.gdp)  + y(col.life)
#     JS      plot(data(gapminder), point, x(col.gdp), y(col.life))
#
# Julia has a symbol, so it has the distinction the other two had to build: `:red`
# is not `"red"`, and the grammar's oldest rule (**a channel takes a column, never
# a value**) is visible in the syntax itself. Spec §18 refuses `color("red")` as a
# channel argument precisely on that rule; R gets the refusal free because a bare
# `red` is not a string, and Julia gets it free because a symbol is not a string.
# Python and JavaScript are the two that had to spend an accessor restoring it.
#
# So this file is one predicate and three refusals, where `columns.py` is 175
# lines and `columns.js` is 160.

const IDENTIFIER = r"^[A-Za-z_][A-Za-z0-9_!]*$"

# Channels that also exist as a `style()` setting. Spec §7 is the distinction
# these two spellings sit either side of: a channel *maps* a column and earns a
# legend; a setting *fixes* one value and earns none. It is exactly the mistake a
# string in a channel is usually reaching for, so the refusal names both.
const SETTABLE = Set(["color", "size", "opacity", "shape", "pattern"])

"""Take the column name out of `:gdp`, refusing a value with direction."""
function column_name(value::Symbol, atom::AbstractString)
    String(value)
end

function column_name(value::AbstractString, atom::AbstractString)
    direction = occursin(IDENTIFIER, value) ?
        "`$atom(:$value)` maps the column called `$value`" :
        "`$atom(Symbol(\"$value\"))` maps the column of that name"
    setting = atom in SETTABLE ?
        "\n  To fix one value for the whole layer instead — no legend, nothing to " *
        "decode — that is a setting: `style($atom = \"$value\")`." : ""
    throw(GogError(
        "gog: `$atom(\"$value\")` binds a *value*, and a channel takes a *column*. " *
        "In Julia a column is a symbol, which is what keeps the two apart: " *
        "$direction.$setting"))
end

function column_name(value::AbstractVector, atom::AbstractString)
    # The *values* rather than the name. This is spec §18's refused sentence
    # arriving in Julia dress: a plot is a mapping from a table (Law 4 — the table
    # is the context that makes a bare name mean something), so a channel takes a
    # column and never values, and vector-direct plotting is a decided refusal.
    throw(GogError(
        "gog: `$atom()` takes a column *name*, and this is a column's *values*. " *
        "gog plots a table: a channel names one of its columns, so that a legend, " *
        "an axis and a second layer all know what they are talking about. Put the " *
        "values in a table first — `data((value = values,)) + point + $atom(:value)` " *
        "— or, if the table already exists, name the column: `$atom(:<name>)`."))
end

function column_name(value, atom::AbstractString)
    throw(GogError("gog: `$atom()` takes a column — `$atom(:<name>)`. " *
                   "Got $(typeof(value))."))
end
