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
