# Run the manual's sentences through the Julia binding and compare with R.
#
#     julia --project=jl-pkg/GrammarOfGraphics jl-pkg/GrammarOfGraphics/test/book_parity/run.jl
#
# The counterpart of `run.py` and `run.mjs`, held to the same bar. The book is 49
# chapters of R that the engine draws live, so it is the best corpus of real gog
# sentences in existence; `extract.R` records each one with the SVG R got for it,
# and this runs the same sentence in Julia and asks whether the two bindings said
# the same thing.
#
# Three outcomes count as agreement, and they are reported apart because they mean
# different things:
#
#   * the same plot        byte-identical SVG — the engine saw the same spec
#   * the same refusal     word-identical diagnostic — an engine refusal, which
#                          must not depend on who asked
#   * the same refusal,
#     said in Julia        a *binding* refusal, whose message teaches the
#                          caller's own syntax; the two texts differ on purpose

using JSON
using SHA
using Dates
using GrammarOfGraphics
using GrammarOfGraphics: bin, count, sum, min, max, range, size, step, stack

const HERE = @__DIR__
const ROOT = abspath(joinpath(HERE, "..", "..", "..", ".."))
# The corpus is R's recording and language-neutral. It lives beside the Python
# harness because that is where `extract.R` was built; with a third consumer it
# should move somewhere neutral, which is an open thread rather than something
# this file does silently.
const CORPUS = joinpath(ROOT, "py-pkg", "gog", "tests", "book_parity", "corpus")
const STAMP = joinpath(ROOT, "py-pkg", "gog", "tests", "book_parity", "corpus_stamp.py")
const TRANSLATOR = joinpath(HERE, "translate.R")
const TRANSLATIONS = joinpath(HERE, "translations.json")

# jsonlite writes a scalar as a one-element array unless told otherwise.
scalar(v) = v isa AbstractVector ? v[1] : v

# ---------------------------------------------------------------------------
# A table in Julia, from the wire form R sent for it
# ---------------------------------------------------------------------------

function rebuild(wire)
    table = Dict{String,Any}()
    dates = get(wire, "dates", Dict())
    for (name, values) in get(wire, "floats", Dict())
        unit = haskey(dates, name) ? scalar(dates[name]) : nothing
        if unit == "day"
            # Julia is the only binding besides R with both types, so the corpus's
            # unit maps onto a Julia type rather than onto a convention.
            table[name] = Any[v === nothing ? missing : Date(unix2datetime(Float64(v)))
                              for v in values]
        elseif unit == "second"
            table[name] = Any[v === nothing ? missing : unix2datetime(Float64(v))
                              for v in values]
        else
            table[name] = Any[v === nothing ? missing : Float64(v) for v in values]
        end
    end
    for (name, values) in get(wire, "strings", Dict())
        plain = Any[v === nothing ? missing : String(v) for v in values]
        levels = get(get(wire, "levels", Dict()), name, nothing)
        table[name] = levels === nothing ? plain :
                      ordered(plain, String[String(l) for l in levels])
    end
    table
end

# ---------------------------------------------------------------------------
# Evaluating one translated sentence
#
# A fresh module per sentence, so a chapter that redefines a table name between
# two sentences leaves no residue in the next one — which the manual does,
# deliberately.
# ---------------------------------------------------------------------------

function evaluate(source::AbstractString, tables::Dict{String,Any})
    sandbox = Module(:Sentence)
    Core.eval(sandbox, :(using GrammarOfGraphics))
    # The nine kernel words Base also exports. Without this the sentence would
    # die on an ambiguity that is Julia's, not the grammar's.
    Core.eval(sandbox, :(using GrammarOfGraphics: bin, count, sum, min, max,
                                                  range, size, step, stack))
    for (name, table) in tables
        Core.eval(sandbox, Expr(:(=), Symbol(name), table))
    end
    Core.eval(sandbox, Meta.parse(source))
end

function outcome_of(source::AbstractString, tables::Dict{String,Any})
    plot = try
        evaluate(source, tables)
    catch error
        error isa GogError && return (:refused, "REFUSED\n" * error.msg)
        error isa UndefVarError && return (:missing, sprint(showerror, error))
        # A LoadError wrapping one of the above — `Core.eval` of a parsed
        # expression can arrive either way depending on where it threw.
        if error isa LoadError
            error.error isa GogError && return (:refused, "REFUSED\n" * error.error.msg)
            error.error isa UndefVarError &&
                return (:missing, sprint(showerror, error.error))
        end
        return (:crash, sprint(showerror, error))
    end
    try
        svg = render_svg(plot)
        (:drew, "SVG " * bytes2hex(sha256(rstrip(svg))))
    catch error
        error isa GogError && return (:refused, "REFUSED\n" * error.msg)
        (:crash, sprint(showerror, error))
    end
end

# ---------------------------------------------------------------------------

function main()
    cli = try
        find_gog_cli()
    catch
        ""
    end

    # **Is the corpus still about this book and this engine?** Asked before any
    # comparison, because a stale corpus does not fail here — it *narrows*. This
    # loop iterates the corpus, so a sentence the manual gained since the last
    # recording is not a disagreement, it is absent, and the run reports a clean
    # pass over a book that no longer exists. One implementation of that question
    # lives in `corpus_stamp.py`; re-deriving its hashes here would be the drift
    # it exists to catch.
    stamp = IOBuffer()
    ok = success(pipeline(`python3 $STAMP check $ROOT $cli`; stdout = stamp,
                          stderr = devnull))
    if !ok
        println("The corpus is not current, so this run would not mean what it says:\n")
        for complaint in split(strip(String(take!(stamp))), "\n")
            println("  * ", complaint, "\n")
        end
        return 1
    end

    # One R process for the whole corpus. The emitter is R because R's parser is
    # what supplies the tree; for Julia it only has to re-emit it.
    driver = "source(\"$TRANSLATOR\"); " *
             "r <- write_julia_translations(\"$CORPUS\", \"$TRANSLATIONS\"); " *
             "cat(paste(r\$gaps, collapse = \"\\n\"))"
    gaps_buffer = IOBuffer()
    if !success(pipeline(`Rscript -e $driver`; stdout = gaps_buffer, stderr = devnull))
        println("The translator failed, so there is nothing to compare.")
        return 1
    end
    gaps = strip(String(take!(gaps_buffer)))

    sentences = JSON.parsefile(joinpath(CORPUS, "sentences.json"))
    wire_tables = JSON.parsefile(joinpath(CORPUS, "tables.json"))
    pool = [rebuild(w) for w in JSON.parsefile(joinpath(CORPUS, "pool.json"))]
    translations = Dict{String,Any}()
    for t in JSON.parsefile(TRANSLATIONS)
        translations[scalar(t["id"])] = t
    end

    # `chapter/name` first, then the shared `/name` — the same nearest-wins order
    # the chapters themselves resolve in.
    by_chapter = Dict{String,Dict{String,Any}}()
    for (key, index) in wire_tables
        cut = findlast('/', key)
        chapter = cut === nothing ? "" : key[1:cut-1]
        name = cut === nothing ? key : key[cut+1:end]
        get!(by_chapter, chapter, Dict{String,Any}())[name] = pool[scalar(index)]
    end

    tally = Dict{String,Int}()
    bump(name) = tally[name] = get(tally, name, 0) + 1
    failures = Tuple{String,String,String,String}[]
    binding_refusals = Tuple{String,String,String}[]
    language_specific = Tuple{String,String,String}[]
    untranslated = Tuple{String,String}[]

    for sentence in sentences
        id = scalar(sentence["id"])
        translation = get(translations, id, nothing)

        if translation !== nothing && get(translation, "blocked", nothing) !== nothing
            bump("language-specific (not translated)")
            push!(language_specific, (id, scalar(translation["blocked"]),
                                      first(split(sentence["source"], "\n"))))
            continue
        end
        if translation === nothing || get(translation, "julia", nothing) === nothing
            bump("THE SURFACE COULD NOT EXPRESS")
            push!(untranslated, (id, first(split(sentence["source"], "\n"))))
            continue
        end

        tables = Dict{String,Any}()
        merge!(tables, get(by_chapter, "", Dict{String,Any}()))
        merge!(tables, get(by_chapter, scalar(sentence["chapter"]), Dict{String,Any}()))
        # An empty R list is `[]` in JSON, not `{}` — the sentence refused before
        # a spec existed, so it has no tables of its own.
        own = get(sentence, "tables", nothing)
        if own isa AbstractDict
            for (name, index) in own
                tables[name] = pool[scalar(index)]
            end
        end

        kind, text = outcome_of(scalar(translation["julia"]), tables)
        source = scalar(translation["julia"])

        if kind === :missing
            bump("table or name missing from the corpus")
            push!(failures, (id, "UndefVarError", text, source))
            continue
        elseif kind === :crash
            bump("CRASHED")
            push!(failures, (id, "crash", text, source))
            continue
        end

        expected = scalar(sentence["outcome"])
        if text == expected
            bump(kind === :refused ? "identical refusal" : "identical plot")
        elseif kind === :refused && startswith(expected, "REFUSED")
            bump("refused in both, worded per binding")
            push!(binding_refusals, (id, expected[9:end], text[9:end]))
        else
            bump("DISAGREED")
            how = kind === :refused ? "R drew, Julia refused" :
                  startswith(expected, "REFUSED") ? "R refused, Julia drew" :
                  "both drew, different SVG"
            detail = kind === :refused ? text[9:end] :
                     startswith(expected, "REFUSED") ? expected[9:end] :
                     "R $expected vs Julia $text"
            push!(failures, (id, how, detail, source))
        end
    end

    println("$(length(sentences)) sentences from the manual\n")
    for (name, n) in sort(collect(tally), by = last, rev = true)
        println("  ", lpad(n, 4), "  ", name)
    end

    if !isempty(gaps)
        println("\nconstructs the emitter does not handle:")
        for gap in split(gaps, "\n")
            println("  * ", gap)
        end
    end

    if !isempty(binding_refusals)
        println("\n$(length(binding_refusals)) refusals worded per binding (expected — a " *
                "message teaches the caller's own syntax):")
        for (id, r, jl) in binding_refusals[1:Base.min(6, end)]
            println("  $id\n      R : ", first(split(r, "\n"))[1:Base.min(104, end)])
            println("      jl: ", first(split(jl, "\n"))[1:Base.min(104, end)])
        end
    end

    if !isempty(language_specific)
        println("\n$(length(language_specific)) sentences that do not carry over:")
        for (id, why, source) in language_specific
            println("  ", rpad(id, 24), source[1:Base.min(64, end)], "\n      ", why)
        end
    end

    if !isempty(untranslated)
        println("\n$(length(untranslated)) the surface could not express:")
        for (id, source) in untranslated
            println("  ", rpad(id, 24), source[1:Base.min(78, end)])
        end
    end

    if !isempty(failures)
        println("\n$(length(failures)) to look at:")
        for (id, kind, detail, source) in failures[1:Base.min(25, end)]
            println("  ", rpad(id, 24), kind, "\n      ", detail[1:Base.min(200, end)])
            println("      ", first(split(source, "\n"))[1:Base.min(110, end)])
        end
    end

    (get(tally, "DISAGREED", 0) + get(tally, "CRASHED", 0) +
     get(tally, "THE SURFACE COULD NOT EXPRESS", 0)) > 0 ? 1 : 0
end

exit(main())
