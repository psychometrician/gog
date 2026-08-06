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
