# json.jl — the wire, written out
#
# Julia's standard library has no JSON, and the ecosystem's `JSON.jl` would be a
# perfectly ordinary dependency to take. This is ~60 lines instead, for one
# reason that is worth stating: **the binding only ever writes JSON.** It hands
# `{spec, data}` to the engine on stdin and the engine answers with an SVG, so
# there is nothing to parse and none of the hard half of a JSON library is
# needed. A dependency the package would owe compat bounds for, in exchange for
# the easy half, is a bad trade — and the engine's own dependency-free discipline
# is the reason this project can claim its bindings are thin.
#
# (The *test* harness does parse JSON, because the parity corpus is JSON. That is
# test code and takes whatever it needs; the package stays clean.)

json_escape(io::IO, s::AbstractString) = begin
    print(io, '"')
    for c in s
        if c == '"'
            print(io, "\\\"")
        elseif c == '\\'
            print(io, "\\\\")
        elseif c == '\n'
            print(io, "\\n")
        elseif c == '\r'
            print(io, "\\r")
        elseif c == '\t'
            print(io, "\\t")
        elseif c < ' '
            # Every other control character, which JSON forbids raw. Non-ASCII is
            # written through as UTF-8, which JSON permits and which keeps a
            # Hangeul column name readable in the payload.
            print(io, "\\u", lpad(string(UInt16(c), base = 16), 4, '0'))
        else
            print(io, c)
        end
    end
    print(io, '"')
end

to_json(io::IO, ::Nothing) = print(io, "null")
to_json(io::IO, ::Missing) = print(io, "null")
to_json(io::IO, value::Bool) = print(io, value ? "true" : "false")
to_json(io::IO, value::AbstractString) = json_escape(io, value)
to_json(io::IO, value::Symbol) = json_escape(io, String(value))

function to_json(io::IO, value::Real)
    # A NaN or an Inf would be written as a bare token that is not JSON, and
    # serde rejects it with a byte offset naming nothing the caller can act on.
    # Every missing value has already become `nothing` in `to_wire`, so this can
    # only fire on a bug — and failing loudly beats handing the engine something
    # it must guess at.
    isfinite(value) || throw(GogError(
        "gog: $value cannot be written to the engine — a scale has nowhere to put " *
        "it. Use `missing` for a missing value."))
    print(io, value isa Integer ? string(value) : string(Float64(value)))
end

function to_json(io::IO, values::AbstractVector)
    print(io, '[')
    for (i, value) in enumerate(values)
        i == 1 || print(io, ',')
        to_json(io, value)
    end
    print(io, ']')
end

function to_json(io::IO, mapping::AbstractDict)
    print(io, '{')
    first = true
    for (key, value) in mapping
        first || print(io, ',')
        first = false
        json_escape(io, key isa Symbol ? String(key) : string(key))
        print(io, ':')
        to_json(io, value)
    end
    print(io, '}')
end

to_json(io::IO, value::Dates.Date) = to_json(io, Dates.datetime2unix(Dates.DateTime(value)))
to_json(io::IO, value::Dates.DateTime) = to_json(io, Dates.datetime2unix(value))

function to_json(value)
    io = IOBuffer()
    to_json(io, value)
    String(take!(io))
end
