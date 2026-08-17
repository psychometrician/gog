# Put the browser engine where Quarto will find it, before the render starts.
#
# Run as the project's `pre-render` script, and the timing is the entire reason
# it exists rather than living in `setup.R` with everything else. Quarto reads
# `resources:` and decides what to copy into `_book/` at the *start* of a render.
# `setup.R` runs later, once per chapter, as the first chunk of each one. So a
# copy made there is made too late: the file lands in `book/` correctly, and the
# render finishes with `_book/gog.wasm` absent because it did not exist when
# Quarto looked.
#
# That failure hides on a machine that has rendered before, which is every
# machine except the one that matters. A local build finds last time's copy
# already sitting in `book/` and works; a clean CI checkout has none and
# publishes a book whose 3-D plots quietly do not turn. It was caught by the
# guard in `book.yml` rather than by a person, which is what that guard is for.
#
# `setup.R` still points the bindings at these files, because the URL it sets
# depends on how deep the chapter is and only a chapter knows that.

root <- normalizePath(file.path(getwd(), ".."), mustWork = FALSE)
wasm <- file.path(root, "gog-wasm", "target", "wasm32-unknown-unknown",
                  "release", "gog_wasm.wasm")
js   <- file.path(root, "js-pkg", "gog", "src", "interactive.js")
view <- file.path(root, "js-pkg", "gog", "src", "view.js")

# Copy only when the destination is actually different. `overwrite = TRUE` alone
# rewrites the file every time, which gives it a new modification time even when
# not one byte changed. Both of these files are declared under `resources:`, and
# `quarto preview` watches every resource for changes — so an unconditional copy
# announces a change to the project on every single render, including renders
# that changed nothing. Comparing the bytes first makes a no-op render a no-op.
same <- function(from, to) {
  file.exists(to) &&
    file.size(from) == file.size(to) &&
    identical(readBin(from, "raw", file.size(from)),
              readBin(to,   "raw", file.size(to)))
}

stage <- function(from, to) {
  if (same(from, to)) return(FALSE)
  file.copy(from, to, overwrite = TRUE)
  TRUE
}

# **The two modules are staged apart from the engine, and that split is the
# point.** `view.js` gives every plot its zoom, its pan and its fit, and it asks
# the engine nothing — so gating it on a WebAssembly build that has not happened
# would leave 587 plots without controls to protect nine that need a cube. This
# used to be one `if` over all three, which meant a checkout with no engine staged
# no JavaScript either.
copied <- c(
  if (file.exists(view)) stage(view, file.path(getwd(), "view.js")) else FALSE,
  if (file.exists(js))   stage(js,   file.path(getwd(), "interactive.js")) else FALSE
)
if (!file.exists(view) || !file.exists(js)) {
  cat("gog: browser modules missing from js-pkg/gog/src - plots will be still.\n")
} else if (any(copied)) {
  cat("gog: browser modules staged for the render\n")
}

if (file.exists(wasm)) {
  if (stage(wasm, file.path(getwd(), "gog.wasm"))) {
    cat("gog: browser engine staged for the render\n")
  }
} else {
  # Not an error. The engine is built by a separate cargo invocation from the one
  # that builds `gog-cli`, so a checkout that has not run it renders the whole
  # book correctly with still cubes. Said out loud because a silently static book
  # is exactly what would otherwise go unnoticed. Zoom is unaffected: it never
  # loads this.
  cat("gog: no WebAssembly engine built - 3-D plots will render static.\n",
      "  cargo build --release --target wasm32-unknown-unknown",
      " --manifest-path gog-wasm/Cargo.toml\n", sep = "")
}

# The list of table names, published beside the tables themselves.
#
# `gog_table()` in all four packages reads this when a name is not found, so it
# can say *did you mean* instead of only *no*. A misspelt name is the commonest
# mistake that helper has, and the four packages cannot answer it from anything
# they carry: a list shipped inside a wheel or a tarball is fixed at the version
# it shipped with, so the day a table is added, an installed copy would deny a
# table that exists. A confident, wrong refusal is worse than a vague one.
#
# Generated here rather than written by hand for the same reason. It is the
# directory read back to itself, so it cannot fall behind the directory. It is
# gitignored: a committed copy would be one more thing to remember, which is
# what this is replacing.
#
# Byte-compared like the three above, and for the sharper reason: this sits
# *inside* `data/`, which is declared under `resources:`, so rewriting it on
# every render is what makes `quarto preview` unable to follow a link.
tables <- sort(sub("[.]csv$", "", list.files(file.path(getwd(), "data"),
                                             pattern = "[.]csv$")))
manifest <- file.path(getwd(), "data", "tables.txt")
if (!file.exists(manifest) ||
      !identical(readLines(manifest, warn = FALSE), tables)) {
  writeLines(tables, manifest)
  cat("gog: ", length(tables), " table names listed for gog_table()\n", sep = "")
}
