# The blog's pre-render, and it is short because the book's is reusable as it
# stands. `book/R/copy-assets.R` takes its source root from `getwd()/..` and
# writes its destinations under `getwd()`, and Quarto runs a pre-render script
# with the project directory as the working directory. `blog/` and `book/` sit
# at the same depth, so that file needs no argument and no fork to stage the
# browser engine here instead of there. It brings `same()` and `stage()` with
# it, and its own reasons for existing at pre-render time rather than in
# `setup.R` apply here unchanged.
source("../book/R/copy-assets.R")

# The stylesheet is the book's, and there is one of it. It is what turns a
# `gog:` refusal into a tinted block rather than a `<pre>` that scrolls
# sideways, which matters more on a blog than in the book: a post has one or
# two refusals in it and no surrounding chapter to explain the shape.
#
# Copied rather than named across the project boundary. `css:` is resolved
# inside the project that declares it, and a path climbing out of one is not
# something Quarto documents carrying, so a copy avoids the question at the
# cost of a file the render makes and git ignores.
if (stage("../book/gog.css", file.path(getwd(), "gog.css"))) {
  cat("gog: the book's stylesheet staged for the render\n")
}

# The highlighter, which is the stylesheet's other half: `gog.css` says what a
# table, a mark, a channel and a transform are colored, and this decides which
# words those are. One file for the same reason there is one stylesheet, and it
# travels the same direction.
if (stage("../book/gog-syntax.html", file.path(getwd(), "gog-syntax.html"))) {
  cat("gog: the highlighter staged for the render\n")
}

# The palette, for the same reason and by the same route. Both sites declare a
# light theme and a dark one, and they are one project, so the colors are
# written once and read twice. `theme:` is resolved inside the project that
# declares it exactly as `css:` is, so these are copied in rather than named
# across the boundary.
themes <- c("theme-light.scss", "theme-dark.scss")
if (any(vapply(themes,
               function(f) stage(file.path("..", "book", f),
                                 file.path(getwd(), f)),
               logical(1)))) {
  cat("gog: the shared palette staged for the render\n")
}

# The architecture diagram, which the README shows as well. It is a project
# asset rather than a blog asset, so it lives in the repository's `images/` and
# is copied here for the render. One file, two readers: a second copy kept by
# hand is a second copy that stops matching the first.
if (stage("../images/pipeline.svg", file.path(getwd(), "images", "pipeline.svg"))) {
  cat("gog: the architecture diagram staged for the render\n")
}
