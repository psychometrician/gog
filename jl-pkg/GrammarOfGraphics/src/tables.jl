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
#
# The name carries the package's, and that is the whole of why it is no longer
# `book_table()`. This package and `god` are built to be loaded together, so
# `gog_table()` and `god_table()` stand side by side at a prompt and read as one
# idea in two spellings. They still differ by the one letter that separates the
# two projects everywhere else, so neither masks the other.
#
# The old name is gone rather than deprecated. An alias would have been the
# careful move on a package with a readership, and this one does not have one
# yet: the window where a rename costs nobody anything is open now and closes
# for good. Two spellings of one function is a debt Law 3 would have carried
# until someone finally removed it, so it was not taken on.

using Downloads, DelimitedFiles

const BOOK_DATA_URL = "https://psychometrician.github.io/gog-book/data/"
const BOOK_DATA_CHAPTER = "https://psychometrician.github.io/gog-book/book-data.html"

"""
    _table_names()

The names of the tables, read from the site rather than carried.

A list shipped inside the registered version would be fixed at the version it
shipped with, so the day a table is added an installed copy would deny a table
that exists. That is the worst kind of refusal: confident and wrong. The site
publishes the list beside the tables themselves, generated from the directory,
so the answer is always the one the site can actually serve.

Read only when a name has already failed, so the cost falls on the error path
and never on a plot. Returns nothing rather than failing, because a diagnostic
that can itself fail is not a diagnostic. It reads into a buffer rather than a
temporary file, so unlike `gog_table` it leaves nothing behind to remove.
"""
function _table_names()
    body = try
        io = IOBuffer()
        Downloads.download(BOOK_DATA_URL * "tables.txt", io)
        String(take!(io))
    catch
        return String[]
    end
    filter(!isempty, strip.(split(body, '\n')))
end

"""
    _edit_distance(a, b)

Levenshtein distance, two-row variant — the engine's, in Julia.

`Base.min` is spelled out. This package exports `min` as a transform, so inside
the module the bare name is gog's atom and not the arithmetic: the same collision
`max`, `sum` and `map` carry, and here it is a plain function call that resolves
to the wrong function rather than a syntax error.
"""
function _edit_distance(a, b)
    a, b = collect(a), collect(b)
    previous = collect(0:length(b))
    current = zeros(Int, length(b) + 1)
    for i in 1:length(a)
        current[1] = i
        for j in 1:length(b)
            substitute = a[i] == b[j] ? 0 : 1
            current[j + 1] = Base.min(previous[j + 1] + 1, current[j] + 1,
                                      previous[j] + substitute)
        end
        previous, current = current, previous
    end
    previous[end]
end

"""
    _nearest_table(name, known)

The closest name, or `nothing`.

The rule is the engine's, which suggests a color the same way: within two edits,
and fewer edits than the candidate has letters, so a short name cannot match
everything. Deliberately conservative — a wrong suggestion sends the reader to a
second wall, which is worse than sending them to the chapter.
"""
function _nearest_table(name, known)
    lower = lowercase(strip(name))
    best, shortest = nothing, typemax(Int)
    for candidate in known
        distance = _edit_distance(lower, candidate)
        if distance <= 2 && distance < length(candidate) && distance < shortest
            best, shortest = candidate, distance
        end
    end
    best
end

"""
    _unknown_table(name, known)

What to say about a name the site does not have.

A near-miss is named on its own, because it is the whole answer. Without one the
chapter is the answer, and the full list of names is not printed here: the engine
declines a color the same way, naming the one candidate or pointing at the
vocabulary, never reciting it.
"""
function _unknown_table(name, known)
    near = _nearest_table(name, known)
    near === nothing || return "gog: there is no table called \"$name\". " *
                               "Did you mean \"$near\"?"
    "gog: there is no table called \"$name\". The table names are listed in " *
    "the book's data chapter: " * BOOK_DATA_CHAPTER
end

"""
    _unreachable(name)

The site answered nothing at all — a different problem, said differently.

Kept apart from the unknown-name refusal because the two ask opposite things of
the reader: one is a name to correct, the other is a connection to check. Telling
someone their table does not exist when the network is down is the
confidently-wrong refusal this whole path exists to avoid.
"""
_unreachable(name) =
    "gog: could not reach the book's data site to read \"$name\". " *
    "The tables are fetched from " * BOOK_DATA_URL * ", so this needs a " *
    "network connection."

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
    gog_table(name; text = String[])

Read one of the book's example tables.

`name` is the table's name without the extension, such as `"gapminder_2007"`;
the full list is in the book's data chapter. `text` names columns that must stay
text, because a CSV records what a value is and never what kind of thing it is,
so a column of `01`, `02`, `03` comes back as the numbers 1, 2, 3 otherwise.

```julia
gapminder_2007 = gog_table("gapminder_2007")
data(gapminder_2007) + point + x(:gdp) + y(:life)
```
"""
function gog_table(name::AbstractString; text = String[])
    # A misspelt name is the commonest mistake this function has, and until the
    # refusal below it was answered by whichever words the host language happened
    # to use for a failed request. Julia said `RequestError`, which names neither
    # the table nor the fix, and which `catch err; err isa GogError` does not
    # catch — so a session that wraps the grammar's refusals missed this one.
    # The two cases are told apart by the response rather than by the message:
    # an HTTP status arrives with `code` zero, a network failure with a curl one.
    path = try
        Downloads.download(BOOK_DATA_URL * name * ".csv")
    catch err
        err isa Downloads.RequestError || rethrow()
        status = err.response === nothing ? 0 : err.response.status
        throw(GogError(status == 404 ? _unknown_table(name, _table_names()) :
                       _unreachable(name)))
    end
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
gog_table(name; text = String[]) = throw(GogError(
    "gog: gog_table() takes one table name, as in gog_table(\"gapminder_2007\"). " *
    "The names are listed in the book's data chapter."))
