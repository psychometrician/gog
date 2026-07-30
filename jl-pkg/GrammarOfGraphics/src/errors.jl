# errors.jl — the one exception type the binding throws
#
# A refusal is a refusal whether the engine made it or the binding did, so both
# arrive as `GogError` and both carry a message that starts `gog: ` and says what
# to write instead (spec §12: errors must give direction). The mirror of
# `py-pkg/gog/gog/errors.py` and `js-pkg/gog/src/errors.js`; R needs no
# equivalent, because `stop()` already is one thing.

struct GogError <: Exception
    msg::String
end

# Keep the engine's wording verbatim. Julia's default would prefix the type name,
# and a diagnostic this project wrote to be read should arrive as it was written.
Base.showerror(io::IO, error::GogError) = print(io, error.msg)
