# errors.jl — the one exception type the binding throws
#
# A refusal is a refusal whether the engine made it or the binding did, so both
# arrive as `GogError` and both carry a message that starts `gog: ` and says what
# to write instead (spec §12: errors must give direction). The mirror of
# `py-pkg/gog/gog/errors.py` and `js-pkg/gog/src/errors.js`; R needs no
# equivalent, because `stop()` already is one thing.

"""
    GogError(msg)

The error gog throws when it refuses a plot.

A refusal can come from the package, at the line where you wrote a word, or
from the engine, when the plot is drawn. Both are the same kind of error. Its
message starts with `gog:` and says what to write instead. gog never draws part of what you
wrote and ignores the rest: it draws all of it, or it refuses and says why.

A script stops on a refusal, as on any error. In a notebook, a refused plot shows
the message in place of the picture.

# Arguments
- `msg`: the message, kept in the `msg` field. It is shown as it was written,
  with no type name in front of it.

# Examples
```julia
cars = (wt = [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
        mpg = [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4])

# A channel takes a column, so a string here is refused.
try
    data(cars) + point + x(:wt) + y("mpg")
catch err
    err isa GogError && println(err.msg)
end

p = data(cars) + point + x(:wt) + y(:mpg)
```
"""
struct GogError <: Exception
    msg::String
end

# Keep the engine's wording verbatim. Julia's default would prefix the type name,
# and a diagnostic this project wrote to be read should arrive as it was written.
Base.showerror(io::IO, error::GogError) = print(io, error.msg)
