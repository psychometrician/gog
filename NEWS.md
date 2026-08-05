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
