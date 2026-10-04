# gog 0.5.1 (2026-10-04)

The R package now needs less memory to install. On Linux the install compiles
the engine, and it now compiles one part at a time. If the machine still has too
little memory, as some Posit Cloud projects do, the install says so and gives the
R code that downloads the engine already built, so nothing has to be compiled.
The R package also carries the browser engine already built, so on Linux its
3-D plots turn and its brushes move, even with a Rust that cannot build for
WebAssembly. Every plot's picture is the same as in 0.5.0, in all four
languages.

# gog 0.5.0 (2026-10-03)

Two columns that name one set of places now draw the arc diagram and the chord
diagram. `flow(exporter, importer, shared = TRUE)` gives each country one slot,
whichever column names it, as long as everything the country exports and
imports. The slot shows its exports first and its imports after them, shaded,
so a reader can tell the two apart where the bands meet it. Flat, each band
arches from one country to another along one axis, and the countries' names
stand under it. Add `polar()` to the same sentence,
and the axis bends into a ring: each band curves toward the center of the
circle. A flow can also be brushed now.
`brush(name, at = "China")` keeps every band that leaves or reaches China and
dims the rest, and in the browser a click on a country's slot selects that
country instead.

The Sankey diagram is drawn from a table of links.
`flow(source, target, layered = TRUE)` puts each place in a layer computed from
the links, so a band can skip a layer and a flow can stop early: in a
household's budget, taxes come one step after the income while the energy bill
lies three steps on. Click a place, or write `brush(name, at = ...)`, to keep
that place's own links.

A plot's legends can now sit on any side of it. `theme(legend = "bottom")`
places each legend's entries side by side under the plot, so the panel gets
shorter instead of narrower. The legend of a continuous color is drawn
horizontally. `"top"` and `"left"` are the other two sides, and `"right"` is
still the default. `theme(legend = )` only chooses the side: to leave one
legend out, write `legend = FALSE` on its channel, as before.

# gog 0.4.1 (2026-10-01)

On Linux the R package builds its engine from source, using the version of Rust
already installed. It now builds with Rust 1.75 or newer, the version that Ubuntu
22.04 and 24.04 provide, so the usual install command works there as it is. gog
0.4.0 needed Rust 1.89 without saying so, and on those machines the install
stopped with an error about a lock file. If every version of Rust that the
install finds is too old, it now stops before building. The message names each
version found, the version needed, and the command that installs a newer one.
Plots draw exactly as they did in 0.4.0, in all four languages.

# gog 0.4.0 (2026-09-30)

A `smooth` line can now carry its confidence band. `smooth_band` is to `smooth`
what `confidence` is to `mean`, so `ribbon * smooth_band + line * smooth` draws
the fitted curve inside its band. The level is 0.95 unless you name another, as
in `smooth_band(0.99)`. It needs five rows in each group, because with fewer
the curve passes through every row and nothing is left to measure the band
from.

Several sentences gog used to refuse now draw. Over a category, `point * dodge`
is a beeswarm: each point moves across its slot only as far as it must to clear
its neighbors, so the outline of each swarm shows its group's distribution.
`text` takes a halo, a band of color under its letters set with
`style(border_color =, border_size =)`, so a name stays readable over lines and
dense points. A `surface` takes `quantile`, as it already took the other
summaries. An atom written after a facet, as in `plot | facet(g) + title("A
title")`, now draws in all four languages.

Colors and legends follow what you name. `palette(c(Asia = "tomato", Europe =
"steelblue"))` gives each category the color written beside it, whatever order
the rows arrive in, and a misspelled name is answered with the category it was
probably meant to be. `legend = FALSE` on a channel keeps the channel and leaves
its legend out, for a plot that already names its categories on an axis or
beside its marks.

There is one way to write the drawing to a file. `save_svg(plot, "plot.svg")`
writes it in all four languages, byte for byte what `render_svg()` returns, and
a plot gog refuses leaves a file already at that path untouched. It replaces
`save()` in Python, JavaScript and Julia, where `save()` now refuses and names
it.

On a web page, a selection reaches more plots. A brush moves across an
`interval` or a `zone` placed by `bounds`, and a brushed plot in the cube or on
the globe reports what its bound caught. Plots on one page that read one table
count each row once, and the same plot twice on one page keeps its own controls
and textures.

# gog 0.3.0 (2026-09-20)

Every plot with a y axis name looks different, and that is the headline rather
than a side effect. gog had been drawing two conventions at once: the y name sat
horizontally above the panel while the x name sat centered beside its own axis,
so there was no rule a chapter could state. Now one property states it for both.
By default each name is centered along its own axis, which turns the y name
through ninety degrees and gives the panel back the band the horizontal one
used. `theme(axis_label = "end")` asks for the other convention, both names at
their axis's far end and horizontal, so nothing on the plot is read sideways.

The three vocabularies that answer *which one?* grew, and each stopped where its
own geometry runs out. `shape` draws seven glyphs rather than five, gaining a
star and a wye, which completes the symbol family it had been drawing part of. A
stroke's `pattern` gains `dotdash` and `longdash` for five, and a fill's gains
`stripes` for six. `stripes` fills the hole the other five already made: one
diagonal is a hatch, both are a crosshatch, both orthogonals are a grid, and
nothing drew a single orthogonal until now. A glyph shows its whole silhouette
at ten pixels, a fill is the largest thing on the panel, and a dash reads only
where enough uninterrupted line is visible, which is why the three counts
differ.

A vocabulary that runs out now says so. Map a column with more categories than
there are kinds and the set starts over, so two categories draw the same glyph
and look like one group. gog names the count, the limit and two ways out, and
draws the plot anyway, because it is legal and the grammar does not forbid what
it can draw.

Julia installs an engine. A plot is drawn by a compiled binary, and the Julia
package was the one of four that shipped none, so `Pkg.add` used to give a
reader a package that loaded and then refused on their first plot with
instructions to install Rust. It now arrives as an artifact, which is how a
Julia package distributes a binary, so the install is one command and there is
nothing to build.

# gog 0.1.0 (2026-08-18)

The version leaves 0.0.x because the vocabulary grew rather than the count of
things that work. Four families arrived at once, and one of them added the
fourteenth mark, which is the largest change this grammar makes.

A table of relations is now a picture. `edge * layout(from, to) + network()`
reads the two endpoint columns of an edge table, computes a position for every
name in the engine, and draws one stroke per row. Three marks read the one
layout, so the connections, the nodes and their names always agree about where
everything sits, and the layout hands each node its `degree` so `size(degree)`
draws the busiest largest. Nothing in the table says where a node belongs;
saying it is what the layout is for. State a viewing angle and the same layout
is computed in a cube you can turn.

The earth is round again. `globe()` takes the longitude and latitude a map
takes and stands them on a sphere, where a route bends along a great circle, a
rule holds a whole meridian, and a country the horizon cuts is closed along the
edge of the disk. Half the earth faces away from any view, so the plot says how
many rows it hid rather than letting you take what you see for everything. A
bar here stands on the radius, the one axis a sphere has to spare.

A magnitude can be followed through its stages. `ribbon * flow(class, sex,
survived) + y(n)` draws each path as a band as thick as its count at both ends,
which is the picture the world calls a Sankey diagram. One table, one row per
path, and the totals match at every stage because the shape of the input makes
them.

Things that are alike can be put beside each other. `path * cluster(amount,
over = nutrient)` joins the two closest leaves, then the next two, and draws
the tree; `zone * cluster(over = nutrient)` keeps the tiles and takes the
tree's order. Compose them and a clustered heatmap is three plots and two
operators rather than a figure type of its own.

Labels now rest where there is room. `text * repel` pulled every name back
toward a starting place above its dot, so a crowd of them stacked into a column
whatever the picture underneath looked like. Each label now settles on
whichever side of its point has space. The finding, the measurements against
ggrepel, and the shape of the fix came from a reader, and it is the first
outside contribution to this grammar.

JavaScript can hand a page a plot the reader can turn. `html_block()` returns
the picture wrapped for a web page, with the controls under it and the browser
engine that turns a cube or a globe. The other three languages answer that
question through their notebook's own display hook and never needed a name for
it.

# gog 0.0.5 (2026-08-12)

A plot that plays can be stopped and stepped. Three buttons sit on the same line
as the zoom and the camera, and they move the clock rather than the plot: step
back, stop or start, step forward. Stepping stops the clock, because a running
one would carry you off the frame you asked for. The ends join up, so stepping
back from the first frame reaches the last, which is what the sequence already
does when it loops. Stop the clock and the camera saves the frame you stopped
on. The grammar grew no word for any of it, so a sentence is unchanged and a
printed sequence is still its first frame.

The warnings were always written; now you can read them. gog reports what it had
to assume on the way to a picture, and those reports went to standard error,
which a notebook and a browser do not have. Rows a log axis could not place, a
custom palette with the wrong number of colors, and a many-row line with no
`group` all warned into nothing for anyone not working at a command line. The
same words now travel with the drawing. `save_gif()` had the same hole from the
other side, building every diagnostic and then dropping the list, so a written
file was silent about what it assumed.

One missing value no longer erases a bar. A single non-finite value in
`bar * mean + x(category)` turned that whole category into nothing, while the
same transform without an `x` quietly dropped it instead, so the same data gave
two different answers depending on how it was grouped. Every keying now drops
non-finite values the same way, and a group with nothing finite draws nothing
rather than something wrong. A value pushed outside its scale by a transform is
held at the scale's ends now, instead of asking for a negative radius or an
opacity above one.

One thing to know before you upgrade. `book_table()` is now `gog_table()`, and
there is no alias. Change the call and nothing else: the arguments, the table
names and everything it returns are the same. The old name did not say which
package it came from, which mattered once `god` arrived with a helper of its
own doing the same job for its own book.

# gog 0.0.4 (2026-08-05)

Clicking a mark keeps its row on the plot. Hovering reads one row and forgets it
as soon as you move, so two rows could never be compared. A click leaves a card
that stays, and dragging the card moves it clear of the crowd it names while a
line stretches back to the point. On a plot that plays, a card waits for its own
moment and comes back each time the loop returns to it. The camera saves the
cards where you put them.

Labels that overlap move apart. `text * repel` is the fourth collision modifier,
beside `dodge`, `stack` and `jitter`, and the first whose collision is made of
ink rather than of position: two words overlap where their points never did.
Every label ends up outside its own dot, and one that moved far keeps a thin line
back. The placement uses no random numbers, so one specification always draws the
same picture.

Three more statistics have plain names. `deviation` is the spread of the data, as
`confidence` is the uncertainty of the mean. `quantile(p)` reduces a group to the
value at one probability, so `line * quantile(0.9)` is a service level or a
growth chart. And `range(0.25, 0.75)` names a band by its two ends, so an
interval is the middle half and two ribbons at different widths are a fan chart.

One thing to know before you upgrade. In R, `range` now masks `base::range`,
because a transform that takes a parameter has to be a function. Write
`base::range(x)` for the smallest and largest of a vector. Python, Julia and
JavaScript are unaffected.

# gog 0.0.3 (2026-08-04)

A plot that moves can leave the browser. `save_gif(plot, "wave.gif")` writes a
plot that binds `play()` as a file that plays, so a slide, a message or a post
gets the sequence instead of one still frame. Nothing has to be installed first,
and the frames come from the drawing the plot already made, so the file cannot
disagree with the picture beside it.

A page of plots can say how big it is. `theme(width =, height =)` sizes a
composed figure the way it already sizes a single plot, so two cubes set side by
side stop coming out small with bands of nothing above and below them. Cubes on a
page turn as well, and one drag moves every panel while each keeps the angle its
own sentence asked for.

# gog 0.0.2 (2026-08-03)

Four packages, one grammar, and the first release all of them share.

`query()` binds a table that lives in a database rather than in memory, and the
sentence you write does not change. `brush()` selects rows by dragging across a
plot, and one drag reaches every plot on the page that names the same column.
`map()` is a coordinate space for longitude and latitude, so a region with a
value is a choropleth. `book_table()` fetches any of the manual's example tables
by name, so an example runs without writing a data reader first.

Every plot in a browser now carries controls: zoom in, zoom out, fit, grab to
pan, and save as a PNG. A plot in the cube turns under the mouse, and hover reads
the row beneath the pointer.
