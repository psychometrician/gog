# Contributing to gog

Thanks for looking. This document has two halves: **the laws the grammar obeys**,
which decide whether a change is acceptable at all, and **where code goes**, which
keeps the crate navigable as it grows.

The user-facing telling of the laws — with worked examples and rendered plots — is
the manual's [design-laws chapter](https://psychometrician.github.io/gog-book/design-laws.html).

---

## The Nine Laws

Every design decision in gog answers to these, and a change that breaks one is
rejected however useful it looks. Refer to them **by name**, not only by number.
The one enemy behind all nine: *the expert's convenience*.

| | Law | What it means |
|---|---|---|
| **1** | **Orthogonality** | Every compatible atom combines with every other. If `bin` works on `bar` it works on `line`, `area` and `step` too. |
| **2** | **No Exceptions** | A transform behaves identically on every mark. No silent letters, no special cases that only an expert knows. |
| **3** | **Plain Names** | Common English words; two words maximum, joined by `_`. Only `x`, `y` and `z` are exempt. |
| **4** | **Bind-Once** | A table is named once, columns are bare names, and the nearest table wins. |
| **5** | **Explicit Over Implicit** | Short beats long — unless short is ambiguous, and then it is said out loud. |
| **6** | **Compositional Invariance** | A composed sub-expression means the same thing in every context it appears in. |
| **7** | **Minimum Syllable** | A visual is a mark plus its required positions. Neither renders alone. |
| **8** | **Pronounceable ≠ Useable** | Enforce well-formedness hard; guide taste softly; never forbid the ugly-but-legal. |
| **9** | **Universal Transcription** | The IR encodes the *visual*, never one backend's draw commands. |

Two consequences worth stating separately, because they are the ones most often
broken by accident:

**A setting spans its geometry class.** A `style()` setting is available on every
mark whose geometry can carry it, and absent where composition does the job
better. `border_*` is on every closed-glyph fill (`bar`, `box`, `point`,
`surface`, `zone`); `pattern` is on every path stroke (`line`, `step`,
`interval`). `area` and `ribbon` take neither, because an edge is `area + line`. A
per-mark gap inside a class is the Law 1 violation this catches, and there are
tests that name every mark rather than two hand-written lists.

**Errors must give direction.** There are three diagnostic kinds — **Illegal**,
**Unsupported**, **Assumption**. Fatal ones exit 2 and render nothing;
`GOG_STRICT=0` downgrades them to warnings. **Never** accept a binding and
silently drop it: a plot that draws must mean what was written. Warnings go to
stderr with the `gog: ` prefix and say what to do, not just what went wrong. When
you add a mark or a channel, `rule_for` must gain an entry — the
`every_mark_channel_pair_has_a_rule` test fails otherwise.

### A note on `§` references in comments

Source comments cite the project's design specification by section, like
`(spec §15)` or `// the §12 drop`. That specification is not published. The
comment text always carries the meaning on its own. The section number is
provenance rather than a link you are expected to follow, and nothing here needs
it in order to make a change correctly. The laws above are the part you need, and
they are here in full.

---

## The shape of the repository

```
gog-core/                    the engine — all the logic lives here
gog-cli/                     a thin bridge: stdin JSON → stdout SVG. Holds no logic.
gog-wasm/                    the second bridge: the same JSON, read from linear memory
r-pkg/gog/                   the R front end
py-pkg/gog/                  the Python front end
jl-pkg/GrammarOfGraphics/    the Julia front end
js-pkg/gog/                  the JavaScript front end
book/                        the Quarto manual — every plot in it is live-rendered
```

Language bindings are deliberately thin. **A rule implemented in a binding is a
rule the other three will get wrong**, so anything more than one front end needs
belongs in `gog-core`.

**A feature is not done until all four bindings have it.** The vocabulary never
differs between them, underscores included (Law 3). Only the idiom does: R's bare
names, Python's and JavaScript's `col.x`, Julia's `:x`, and JavaScript's trailing
options object. Three parity harnesses draw every sentence in the manual and
compare against R. A binding left behind shows up there as a disagreement rather
than as a quiet debt.

## The module map

Modules are listed in dependency order. Nothing here may depend on anything below
it — the layering is acyclic, and keeping it that way is the point.

| Module | One sentence | Depends on |
|---|---|---|
| `color.rs` | Which colors exist, and is this string one of them | — |
| `time.rs` | How a number becomes a date | — |
| `data.rs` | The in-memory table (temporary; Arrow is planned) | `time` |
| `ir.rs` | The grammar as typed structs; the JSON contract | — |
| `wire.rs` | The request envelope around that contract — spec plus tables, and which rows a missing value costs | `data`, `ir`, `time` |
| `transform.rs` | bin, smooth, count, density, the aggregations | `data` |
| `scale.rs` | How a number becomes a position | `data`, `ir` |
| `legality.rs` | Which bindings are legal, and what to say when they are not | `color`, `data`, `ir`, `scale`, `time` |
| `render/text.rs` | Escaping, and enough metrics to reserve space | — |
| `render/shape.rs` | The glyphs the `shape` channel draws | — |
| `render/encode.rs` | Channel fraction → a visual attribute (opacity, radius) — shared by the marks and the legend | — |
| `render/pattern.rs` | The `pattern` texture aesthetic realized for SVG — a stroke's dash, a fill's hatch tile | — |
| `render/ticks.rs` | Tick selection — linear, logarithmic, calendar | `time` |
| `render/palette.rs` | Which colors a scale hands out — palettes and ramps | `color`, `ir`, `text` |
| `render/mod.rs` | `Layout` and `RenderContext`, shared by the render modules | `data`, `ir` |
| `render/layout.rs` | Where every rectangle on the canvas comes from — margins, panels, strips | `render/mod`, `text`, `ticks` |
| `render/legend.rs` | The key that lets a reader decode a channel | `data`, `ir`, `palette`, `shape`, `text`, `ticks`, `time`, `encode` |
| `render/project.rs` | The 3-D projector — rotate, orthographic-drop and depth-sort for the `space` coordinate | `ir` |
| `render/polar.rs` | The plane bent into a circle — a normalized (angle, radius) → pixels, and the annular sector a bar becomes | `ir`, `render/mod` |
| `render/nest.rs` | The `nest` coordinate space — the panel packed with nested regions, where a measure becomes an area | `render/mod` |
| `render/svg.rs` | Orchestration, axes, and the chrome around the marks | all of the above |
| `render/marks/*.rs` | One drawing routine per mark, plus the `dodge`/`jitter`/bar-thickness toolkit and `place`, the one "where does this datum go" for either coordinate space. `violin.rs` is the exception: one routine that `ribbon` and `area` both call, because a violin is their geometry fed by a slot density rather than a mark of its own | `svg` (mutually), `encode`, `polar`, and the render helpers |
| `render/page.rs` | Separate plots arranged onto one page, each keeping its own coordinate space — and the derivation that the same column on the same axis in two composed plots is *one* axis | `data`, `ir`, `legality`, `layout`, `svg` |
| `plot.rs` | The one way into the engine — a spec in, an SVG or a refusal out | `data`, `ir`, `legality`, `render/svg` |

**`plot.rs` is the top of the crate, and the only public door.** It runs
`legality::check` and applies `GOG_STRICT` before calling the renderer. That is
why `SvgRenderer::render` is `pub(crate)`: the gate must not be a step a caller
remembers to run. It used to be exactly that. The policy lived in
`gog-cli/src/main.rs`, so every caller that was not the CLI drew illegal plots in
silence — all four examples, and any future Rust, WASM or FFI binding. Nothing
depends on `plot.rs`; it depends on the two halves it joins. Add a binding by
calling it, never by reaching past it, and the compiler will hold you to that.

**There are two bridges now, and `wire.rs` is why that is safe.** `gog-cli` reads
the request from stdin. `gog-wasm` reads the identical JSON from a pointer into
linear memory, because a web page has no subprocess to spawn, and a 3-D plot only
becomes turnable once the engine is *in* the page. Only the transport differs.
Everything else is reached through `wire::decode` and `plot::render_figure`:
decoding, the missing-value policy, the legality gate and the renderer. So the
two bridges cannot disagree about which rows get plotted. That is why the
decoding moved down. A disagreement there would not crash. It would quietly draw
one dataset in the browser and a different one on the command line, which is rule
4's lesson in a new place. A test renders the same spec both ways at four viewing
angles and compares bytes.

**`gog-wasm` is deliberately not a workspace member**, and its manifest says why
at length, and there are two reasons. Cargo ignores a non-root package's
`[profile]`, so membership would silently cost the size tuning that takes the
module from ~1.6 MB to ~823 KB. And the R package's `.prepare` stages the
workspace manifest beside `gog-core/` and `gog-cli/` only, so a third member
listed but not copied breaks the build that `remotes::install_github()` and
r-universe run. It therefore needs its own test invocation; `cargo test` at the
root does not reach it.

**`svg.rs` is the orchestrator: the one place that knows the *order* things happen
in** — resolve scope, transform, build axes, draw marks, draw guides. The per-mark
drawing routines used to live here too; they are now one file each under
`render/marks/`, kept as `pub(crate)` methods on `SvgRenderer` and dispatched by
the `match layer.mark` in `render`. That split is the table's one deliberate
cycle: a mark file names `SvgRenderer` and reuses a few renderer-owned constants,
while `svg.rs` calls the mark methods back. That is the unavoidable price of
keeping one type's methods across files.
The *shared visual-scale* vocabulary both the marks and the
legend need lives in neither: it is `render/encode.rs`, below both, which is what
let `legend.rs` stop reaching up into `svg.rs`. An orchestrator is allowed to be
long; what it is not allowed to be is *miscellaneous*.

---

## The rules

### 1. Split along concerns, not line counts

Rust has no file-length limit and neither do we. `rustc` and `clippy` do not care,
and real Rust crates carry files far bigger than these.

The test is not "how long is this file" but:

> **Can the module be described in one sentence, with no "and"?**

Every row of the table above passes that test. When a file stops passing it, split
it — at the seam that makes both halves describable, not in the middle.

Concretely, `svg.rs` was once 2,064 lines holding eleven concerns it had itself
labeled in section banners: palettes, ramps, shapes, opacity, size, legend types,
layout, text metrics, the renderer, helpers, tests. The count was not the problem;
the eleven was.

A later split followed the same test. `svg.rs` had grown back to ~4,600 lines
because every new mark added a `write_<mark>` method to it — nine of them by
`ribbon`, each self-contained, dispatched by one `match`. "Draw the marks" is one
concern, but "draw the box" and "draw the ribbon" are each their own; the seam was
the `match` arm. Each `write_<mark>` moved to `render/marks/<mark>.rs`, the shared
`dodge`/`jitter`/bar-thickness helpers to `marks/mod.rs`. Verified byte-identical
across eleven plots and the full test suite — a reorganization that changes a pixel
is not one (rule 6).

### 2. Tests live beside the code they test

```rust
#[cfg(test)]
mod tests {
    use super::*;
    …
}
```

This is idiomatic Rust and compiled out of release builds, so **test lines are not
file bloat**. `legality.rs` is around 39% tests, and that is a good sign, not a
reason to split. When you move a function to a new module, move its tests with it.

### 3. Dependencies point one way

Before adding a `use`, check the table. If it points upward, the code is in the
wrong module.

A real example: color validation lived in `legality.rs`, so when the renderer
needed to interpolate `palette(c("white", "navy"))` it called
`crate::legality::css_rgb` — the *drawing* code asking the *rule* code for a color
value. The fix was not a better import, it was recognizing that the color
vocabulary is neither: it is `color.rs`, and both depend on it.

### 4. A helper shared by two callers belongs to neither

If two modules need the same function, it moves down a level rather than one of
them reaching sideways for it. `write_shape` is drawn by both the marks and the
legend, so it is `render/shape.rs`, not a `pub(crate)` poking out of `svg.rs`.

This is the lesson a since-deleted `png.rs` taught the project. It duplicated
eight concerns from `svg.rs` — `Layout`, `compute_layout`, `map_x`, `nice_ticks`,
`PALETTE` and more — only because there was no shared module to hold them. The two
copies then drifted, until `bar * bin` rendered untransformed data to PNG. A
missing abstraction became a silent correctness bug. The file was deleted; the
lesson is this rule.

### 5. Prefer `pub(crate)` to `pub`

`gog-core`'s public API is small on purpose: `PlotSpec` and friends from `ir`,
`plot::render`, and `legality::check` for a caller that wants the diagnostics
without the picture. Everything else is an implementation detail and should say
so, so that moving it later is not a breaking change.

**`pub` is not only about churn. It is sometimes about correctness.**
`SvgRenderer::render` was `pub` for no reason beyond having been written that way.
That one word was the whole of the Rust-path legality hole, because it let a
caller draw without passing the gate. When a function must not be called out of order,
`pub(crate)` behind the function that orders it *is* the enforcement, and the
compiler is what checks it.

### 6. Refactors must not change output

A reorganization that changes a pixel is not a reorganization. Prove it:

```bash
# render a few plots, checksum them, do the refactor, render again
md5 -q output/*.svg
```

### 7. Do not extract on speculation

The layout stage was deliberately *not* extracted until faceting arrived — the
first feature whose requirements actually defined the interface. `render/layout.rs`
took the shape faceting needed, not a shape guessed earlier, and the unfaceted plot
fell out as its 1×1 degenerate case, pixel-identical to the old inline arithmetic.

Extract when a concern is *already* separable, not when you suspect it might be.

---

## Before you open a pull request

```bash
cargo build --release
cargo test --release

Rscript r-pkg/gog/tests/test_basic.R                 # run from the repo root
python3 py-pkg/gog/tests/test_basic.py
node --test js-pkg/gog/test/*.mjs
julia --project=jl-pkg/GrammarOfGraphics jl-pkg/GrammarOfGraphics/test/runtests.jl

cd book && quarto render --to html                   # every plot is live-rendered
```

The book's Python chapter runs real Python, so rendering it needs an interpreter
with pandas. One venv beside the book is enough:

```bash
uv venv book/.venv && uv pip install --python book/.venv/bin/python pandas
```

`GOG_BOOK_PYTHON` overrides the choice; a bare `python3` on `PATH` is the
fallback.

Three of those are less obvious than they look:

- **`cargo test` passing does not mean `gog-cli` was rebuilt.** They are separate
  artifacts. The book and the bindings shell out to the binary, so build it
  explicitly.
- **`quarto render` exits 0 even with broken links**, and emits `WARN:`, not
  `warning:`. Grep the output for `-inE "warn|error|unable|cannot|fail|not found"`.
  A clean exit code proves nothing.
- **The R tests load the package with `pkgload::load_all(export_all = FALSE)`**,
  which honors the hand-written `NAMESPACE` — so a missing `export()` or S3
  registration fails at load time. `NAMESPACE` is **hand-maintained, not
  roxygen-generated**; add new atoms to it yourself. Only `R CMD INSTALL` proves
  the package installs, so run
  `R CMD INSTALL --no-docs --library=$(mktemp -d) r-pkg/gog` before submitting.

### If your change alters what a plot looks like

It must appear in the book (`book/`) as a **live** ```` ```{r} ```` chunk. A
```` ```r ```` block does not execute; it shows syntax without proving it works,
and it is how a manual comes to document things the engine cannot do.

**Then re-render the whole book, not only the chapter you edited.** Every plot
comes from the `gog-cli` binary, so one engine change invalidates every plot at
once. But Quarto tracks `.qmd` dependencies rather than the binary. A per-chapter
render therefore leaves the untouched chapters showing stale plots.

Then check what your change made *wrong*. New features quietly invalidate existing
pages — `book/grammar.qmd` states the whole vocabulary in two tables and a kernel
block, and a stale kernel chapter is worse than a missing one.
`book/check_vocabulary.R` (run by the R tests) catches prose that names an atom
which does not exist, in both directions, and `book/check_refusals.R` evaluates
every chunk the book presents as a refusal and fails if one renders instead.

## After you push

A push to `main` runs the tests, and republishes the manual if it touched the book,
the engine, or the R or Python binding. It **cannot** publish a package to any
index: PyPI and npm are reached only by a `py-v*` or `js-v*` tag, and Julia only by
a comment addressed to its registration bot.

The exception is R. r-universe polls this repository and rebuilds `r-pkg/gog` on
every commit, publishing at whatever `DESCRIPTION` says — so a push does change
what an R user installs, without changing the version number they see.

Version numbers are never assigned by CI. One number is written down by hand in
several manifests, and the only automation is a check that they agree. That check
covers seven declarations today; `gog-wasm/Cargo.toml` carries an eighth that it
does not yet reach.
[`.github/RELEASING.md`](.github/RELEASING.md) has the full account: which workflow
each trigger starts, where the seven declarations live, what each registry does with
a number, and which parts of a release cannot be taken back.
