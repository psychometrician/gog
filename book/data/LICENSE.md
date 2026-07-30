# The book's example data — provenance and terms

These CSVs are the tables every chapter of *GOG: A Grammar of Graphics* draws
from. They are published so that a reader can run the manual's examples in R,
Python, Julia or JavaScript, rather than only read them.

They are built by `book/R/make-data.R` and read by `book/R/data.R`. The ruling
that put them here is spec §20, "The cast is fetched, not shipped".

Three tiers, and they are not under one license.

## Original to this book — Apache License 2.0

Twenty-one frames are written out as literals or generated from a fixed seed by
this project's author, and carry the same license as the rest of the code:

`actuals` · `cashflow` · `census` · `commutes` · `day_cycle` · `forecast` ·
`gdp_threshold` · `life_bands` · `listening` · `medals` · `milestones` ·
`quarterly` · `recessions` · `ripples` · `score_band` · `sessions` ·
`six_weeks` · `speed_target` · `spending` · `target_band` · `tide` ·
`thermals` · `thermal_marks` · `winds`

Illustrative rather than authoritative. `census` is two plausible city age
profiles, not a census; `medals` is a medal table's shape, not a record of any
particular games. Do not cite them as data about the world.

## Derived from gapminder — CC0 1.0 (public domain)

Eleven frames are cuts of the `gapminder` R package's table, with three columns
renamed for readability (`gdpPercap` → `gdp`, `lifeExp` → `life`,
`pop` → `population`):

`gm_all` · `gapminder_2007` · `gapminder_asia` · `gm_continents` · `gm_eras` ·
`gm_europe` · `gdp_rug` · `life_rug`

The `gapminder` package is released under **CC0 1.0**, a public domain
dedication, so no permission or attribution is required. It is credited anyway:
the package is by Jennifer Bryan, and the underlying figures are the Gapminder
Foundation's — <https://www.gapminder.org/data/>.

## Derived from R's `datasets` package — GPL-2 | GPL-3

Three frames are reshaped from tables that ship with R itself:

| Frame | From | Reshaping |
|---|---|---|
| `iris_flowers` | `datasets::iris` | columns renamed to bare words |
| `maunga_whau` | `datasets::volcano` | matrix unrolled to long form, every second row and column |
| `quakes_fiji` | `datasets::quakes` | depth negated to an elevation, and cut into 90 km bands |

**These are GPL, and that is why nothing here is shipped inside a package.** R's
`datasets` is licensed GPL-2 | GPL-3, which is copyleft and cannot be absorbed
into an Apache-2.0 wheel or tarball. Hosting them beside the book is ordinary
distribution, which the GPL permits when its terms travel with the files — this
note is that. A reader who uses these three frames is using GPL data and should
treat it accordingly.

The underlying observations are old and public: Edgar Anderson's iris
measurements published by R. A. Fisher in 1936, a 1967 topographic survey of
Maungawhau in Auckland, and seismic events near Fiji. The GPL attaches to R's
compilation of them, not to the facts.
