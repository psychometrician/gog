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

if (file.exists(wasm) && file.exists(js)) {
  copied <- c(stage(wasm, file.path(getwd(), "gog.wasm")),
              stage(js,   file.path(getwd(), "interactive.js")))
  if (any(copied)) {
    cat("gog: browser engine staged for the render\n")
  } else {
    cat("gog: browser engine already staged, nothing copied\n")
  }
} else {
  # Not an error. The engine is built by a separate cargo invocation from the one
  # that builds `gog-cli`, so a checkout that has not run it renders the whole
  # book correctly with still cubes. Said out loud because a silently static book
  # is exactly what would otherwise go unnoticed.
  cat("gog: no WebAssembly engine built - 3-D plots will render static.\n",
      "  cargo build --release --target wasm32-unknown-unknown",
      " --manifest-path gog-wasm/Cargo.toml\n", sep = "")
}
