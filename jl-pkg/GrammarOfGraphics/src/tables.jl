# tables.jl — the book's example tables, fetched by name.
#
# Not a word of the grammar, and deliberately so. This is the same category as
# `render_svg`: something the binding needs and the vocabulary does not.
#
# It exists because every example in the manual begins with a table, and a
# reader who wants to run one should not have to write a CSV reader first. The
# tables are not shipped with the package; they are fetched from the book's own
# site, so one copy serves all four languages and nothing goes stale inside a
# registered version.
#
# `Downloads` and `DelimitedFiles` are both standard library, so this adds no
# dependency — the same discipline `json.jl` keeps for the wire format.

using Downloads, DelimitedFiles

const BOOK_DATA_URL = "https://psychometrician.github.io/gog-book/data/"

"""
    _columns(raw, header, text)

Turn a parsed CSV into columns with the right types. A CSV is text, so every
value arrives as text. A column becomes numbers when *every* value in it parses
as one, and stays text otherwise. Naming a column in `text` keeps it text no
matter what it looks like.
"""
function _columns(raw, header, text)
    table = Dict{String,Any}()
    for (i, column) in enumerate(header)
        key = strip(String(column), '"')
        values = raw[:, i]
        if key in text
            table[key] = values
            continue
        end
        numbers = tryparse.(Float64, values)
        table[key] = any(isnothing, numbers) ? values : numbers
    end
    table
end

"""
    book_table(name; text = String[])

Read one of the book's example tables.

`name` is the table's name without the extension, such as `"gapminder_2007"`;
the full list is in the book's data chapter. `text` names columns that must stay
text, because a CSV records what a value is and never what kind of thing it is,
so a column of `01`, `02`, `03` comes back as the numbers 1, 2, 3 otherwise.

```julia
gapminder_2007 = book_table("gapminder_2007")
data(gapminder_2007) + point + x(:gdp) + y(:life)
```
"""
function book_table(name::AbstractString; text = String[])
    path = Downloads.download(BOOK_DATA_URL * name * ".csv")
    # The download is a temporary file and this function is its only holder, so
    # it is removed on the way out — a session that fetches many tables must
    # not leave one file per call in the temp directory.
    try
        raw, header = readdlm(path, ',', String; header = true)
        _columns(raw, header, text)
    finally
        rm(path; force = true)
    end
end

# Anything that is not a string falls through the typed method above and lands
# here, so the refusal is gog's sentence rather than a bare `MethodError` — the
# same words Python, JavaScript and R use.
book_table(name; text = String[]) = throw(GogError(
    "gog: book_table() takes one table name, as in book_table(\"gapminder_2007\"). " *
    "The names are listed in the book's data chapter."))
