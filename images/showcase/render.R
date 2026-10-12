# images/showcase/render.R
# Draws the README's showcase pictures from the README's own code.
#
# Every picture in the showcase is what the R block printed under it draws. This
# script reads those blocks out of the root README.md, between the
# `<!-- showcase -->` and `<!-- /showcase -->` markers, runs each one in a folder
# of its own, and keeps the file it saves. So a picture cannot show something
# its code does not draw, and an engine change that moves a picture moves it
# here too. Run it from the repository root, then commit the images that moved:
#
#   Rscript images/showcase/render.R
#
# It draws with this checkout's package when pkgload is installed, and with the
# installed gog otherwise. It needs the network, because `gog_table()` reads the
# tables the book publishes, and `rsvg-convert`, because a registry page shows a
# PNG and not an SVG. The three other languages draw the same bytes; the release
# test runs their blocks in each package README and compares.

out <- file.path("images", "showcase")
if (!dir.exists(out)) stop("Run this from the repository root.", call. = FALSE)

if (requireNamespace("pkgload", quietly = TRUE) && dir.exists("r-pkg/gog")) {
  pkgload::load_all("r-pkg/gog", export_all = FALSE, quiet = TRUE)
} else {
  library(gog)
}

readme <- readLines("README.md", warn = FALSE)
from <- grep("<!-- showcase -->", readme, fixed = TRUE)
to <- grep("<!-- /showcase -->", readme, fixed = TRUE)
if (length(from) != 1L || length(to) != 1L || to < from) {
  stop("README.md needs one `<!-- showcase -->` ... `<!-- /showcase -->` pair.",
       call. = FALSE)
}
lines <- readme[from:to]
opens <- which(lines == "```r")
closes <- which(lines == "```")

for (open in opens) {
  close <- min(closes[closes > open])
  code <- lines[(open + 1L):(close - 1L)]
  dir <- tempfile("showcase")
  dir.create(dir)
  owd <- setwd(dir)
  eval(parse(text = code), envir = new.env(parent = globalenv()))
  setwd(owd)
  for (f in list.files(dir)) {
    from_file <- file.path(dir, f)
    if (grepl("[.]svg$", f)) {
      png <- file.path(out, sub("[.]svg$", ".png", f))
      status <- system2("rsvg-convert", c("-w", "900", shQuote(from_file),
                                          "-o", shQuote(png)))
      if (status != 0L) stop("rsvg-convert could not convert ", f, call. = FALSE)
      cat("drew", png, "\n")
    } else {
      file.copy(from_file, file.path(out, f), overwrite = TRUE)
      cat("drew", file.path(out, f), "\n")
    }
  }
  unlink(dir, recursive = TRUE)
}
