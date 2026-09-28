"""Basic sanity test for the Python binding — plain Python, no test runner.

Run from the project root:

    python3 py-pkg/gog/tests/test_basic.py

The mirror of `r-pkg/gog/tests/test_basic.R`: does a sentence reach the engine,
does the engine draw it, and do the refusals refuse. It loads the package from
source (the binding is not installed anywhere yet) and finds `gog-cli` the way
a user's first plot would.
"""

import builtins
import contextlib
import io
import json
import re
import subprocess
import math
import os
import sys
import tempfile
import warnings
from datetime import date

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
sys.path.insert(0, os.path.join(ROOT, "py-pkg", "gog"))

from gog import *  # noqa: E402  — the grammar is meant to be spoken bare
from gog import GogError  # noqa: E402

passed = 0


def ok(message: str) -> None:
    global passed
    passed += 1
    print(f"PASS: {message}")


def refuses(what: str, thunk) -> None:
    """A sentence the grammar must refuse — and the message must say what to do."""
    try:
        thunk()
    except GogError as error:
        text = str(error)
        assert text.startswith("gog:") or "gog:" in text, f"no `gog:` prefix: {text}"
        ok(f"{what} refused — {text.splitlines()[0][:88]}")
        return
    raise AssertionError(f"FAIL: {what} was accepted, and should have been refused")


# ---------------------------------------------------------------------------
# The tables. A dict of lists is Python with nothing installed.
# ---------------------------------------------------------------------------

df = {
    "x": [1.0, 2.0, 3.0, 4.0, 5.0],
    "y": [2.5, 3.1, 1.8, 4.0, 3.5],
    "group": ["A", "B", "A", "B", "A"],
}

bar_df = {"category": ["A", "B", "C"], "value": [10.0, 25.0, 15.0]}

# Every third value missing, so the engine's drop-and-report path is exercised.
gaps = {"a": [1.0, None, 3.0, 4.0], "b": [2.0, 2.5, None, 4.5]}

# A deterministic spread for the histogram — no random module, so the byte
# output of this test is the same every run. `builtins.range` because the
# star-import above put gog's transform over Python's function: the one real
# cost of speaking the grammar bare, and it is checked as a refusal below.
heights = {"height": [150 + (i * 37) % 45 + (i % 7) for i in builtins.range(120)]}

days = {
    "when": [date(2026, 1, 1), date(2026, 2, 1), date(2026, 3, 1), date(2026, 4, 1)],
    "level": [3.0, 5.0, 4.0, 8.0],
}

# ---------------------------------------------------------------------------
# The sentences
# ---------------------------------------------------------------------------

svg = render_svg(
    data(df)
    + x(col.x)
    + y(col.y)
    + point
    + color(col.group)
    + title("Basic scatter")
    + x_label("X value")
    + y_label("Y value")
)
assert "<svg" in svg, "output does not look like SVG"
ok(f"scatter rendered ({len(svg)} chars)")

svg = render_svg(data(df) + x(col.x) + y(col.y) + line + title("Basic line"))
assert "<polyline" in svg or "<path" in svg, "line drew no stroke"
ok(f"line rendered ({len(svg)} chars)")

svg = render_svg(data(bar_df) + x(col.category) + y(col.value) + bar + title("Bar chart"))
assert "<rect" in svg, "bar drew no rectangle"
ok(f"bar rendered ({len(svg)} chars)")

# `*` — a transform derives a layer from a mark. Bare and parameterized must
# reach the same code path, as they do in R.
svg = render_svg(data(heights) + bar * bin + x(col.height) + y(col.count) + title("Histogram"))
assert "<rect" in svg
ok(f"bar * bin (histogram) rendered ({len(svg)} chars)")

svg = render_svg(data(heights) + bar * bin(12) + x(col.height) + y(col.count))
assert "<rect" in svg
ok(f"bar * bin(12) rendered ({len(svg)} chars)")

# Two layers, the second styled — and `style()` reaches only the mark before it.
svg = render_svg(
    data(df)
    + x(col.x)
    + y(col.y)
    + point
    + style(color="tomato", size=6)
    + line
    + style(color="#999999")
)
assert "tomato" in svg and "#999999" in svg
ok("two layers, each with its own style")

# `|` facets into panel columns, and its precedence lets a whole plot sit left.
svg = render_svg(data(df) + point + x(col.x) + y(col.y) | facet(col.group))
assert svg.count("<rect") >= 1
ok("facet by `|` rendered")

# The cube takes a facet too, one projected box per panel. Refused as "not drawn
# yet" until 2026-07-28, when it turned out the renderer had always built its
# scene from the panel's own rectangle and only the check said otherwise.
# `[1.0, 5.0, ...]` written out rather than built with `range`, which `import *`
# shadows with gog's transform — the suite's own refusal message says so.
_cube_df = dict(df)
_cube_df["z"] = [1.0, 5.0, 2.0, 6.0, 3.0]
svg = render_svg(
    data(_cube_df) + point + x(col.x) + y(col.y) + z(col.z) | facet(col.group)
)
_panels = len(set(df["group"]))
assert svg.count('fill="#f5f5f8"') == _panels, "a faceted cube draws one panel per level"
assert svg.count('stroke="#d8d8de"') == _panels, "and each panel projects its own cube"
ok("`+ z(col.z) | facet(col.group)` draws one projected cube per panel")

# `wrap` folds the line of panels into a rectangle. Ten levels wrapped at four
# is a 4 x 3 rectangle holding ten panels: the two cells the fold left over are
# slack, not combinations, so nothing is drawn in them.
_pairs = [(level, k) for level in "ABCDEFGHIJ" for k in (0.0, 1.0)]
wrap_df = {
    "x": [k for _, k in _pairs],
    "y": [float(i) for i, _ in enumerate(_pairs)],
    "g": [level for level, _ in _pairs],
    "h": ["u" if k else "v" for _, k in _pairs],
}
wrapped = render_svg(data(wrap_df) + point + x(col.x) + y(col.y) | facet(col.g, wrap=4))
assert wrapped.count('fill="#f5f5f8"') == 10, "ten levels are ten panels, not twelve cells"
for name in "ABCDEFGHIJ":
    assert f">{name}</text>" in wrapped, f"a wrapped panel carries its own name; missing {name}"
ok("`| facet(col.g, wrap=4)` folds ten panels into a rectangle and names each")

# The direction is the operator's, never the count's.
wrapped_down = render_svg(data(wrap_df) + point + x(col.x) + y(col.y) / facet(col.g, wrap=4))
assert wrapped != wrapped_down, "`|` and `/` must run the levels different ways"
ok("`wrap` says where the line turns; the operator says which way it runs")

refuses(
    "wrapping a crossed facet",
    lambda: render_svg(
        data(wrap_df) + point + x(col.x) + y(col.y) | facet(col.g, wrap=2) / facet(col.h)
    ),
)
refuses("`facet(wrap=)` given something other than a whole number",
        lambda: facet(col.g, wrap="four"))

# A free scale fits each panel from its own rows — and only the axis that asked,
# so x stays shared. Three groups three orders of magnitude apart.
free_df = {"x": [1.0, 2.0, 1.0, 2.0, 1.0, 2.0],
           "y": [1.0, 2.0, 100.0, 200.0, 10.0, 20.0],
           "g": ["a", "a", "b", "b", "c", "c"]}
shared_svg = render_svg(data(free_df) + point + x(col.x) + y(col.y) | facet(col.g))
freed_svg = render_svg(data(free_df) + point + x(col.x) + y(col.y, free=True) | facet(col.g))
assert ">20</text>" not in shared_svg, "a shared y spans 1..200 and never ticks 20"
assert ">200</text>" in freed_svg and ">20</text>" in freed_svg
ok("`y(col.v, free=True)` fits each panel from its own rows")

refuses("a free scale with no panels to free it across",
        lambda: render_svg(data(free_df) + point + x(col.x) + y(col.y, free=True)))
refuses("a free scale beside a stated domain",
        lambda: render_svg(data(free_df) + point + x(col.x)
                           + y(col.y, limits=(0, 300), free=True) | facet(col.g)))
refuses("`free=` given something other than True or False",
        lambda: y(col.y, free="yes"))

# `play` is that same split read in time: one frame per distinct value, laid
# out in sequence instead of across the page.
play_df = {"x": [1.0, 2.0, 3.0, 10.0, 20.0, 30.0],
           "y": [1.0, 2.0, 3.0, 10.0, 20.0, 30.0],
           "year": [1957.0, 1957.0, 1957.0, 1962.0, 1962.0, 1962.0]}
svg = render_svg(data(play_df) + point + x(col.x) + y(col.y) + play(col.year))
# Two moments, once for the marks and once for the strip that names them.
assert svg.count('<animate attributeName="display"') == 4
assert ">1957</text>" in svg and ">1962</text>" in svg
assert ">1957.0<" not in svg, "a year is named, not measured"
ok("`play(year)` cuts one frame per value and names each")

# The invariant the feature rests on: no play, no timing, no bytes.
assert "<animate" not in render_svg(data(play_df) + point + x(col.x) + y(col.y))
ok("a plot that does not play is untouched")

svg = render_svg(data(play_df) + point + x(col.x) + y(col.y) + play(col.year, speed=2))
assert svg.count('<animate attributeName="display"') == 4
assert 'dur="0.800s"' in svg
ok("`speed=2` runs the same frames twice as fast")

# The same sequence written where SVG animation is not read. Checked as a file,
# because everything this adds happens after the SVG above: the header proves it
# is a GIF, the trailer proves it was finished rather than left half-written, and
# NETSCAPE2.0 is what makes it loop instead of freezing on the last moment.
with tempfile.TemporaryDirectory() as folder:
    played = data(play_df) + point + x(col.x) + y(col.y) + play(col.year)
    written = save_gif(played, os.path.join(folder, "wave.gif"))
    raw = open(written, "rb").read()
    assert raw[:6] == b"GIF89a", "save_gif() should write a GIF"
    assert raw[-1:] == b"\x3b", "the GIF should end with its trailer"
    assert b"NETSCAPE2.0" in raw, "the GIF should loop"
    ok("`save_gif()` writes a looping GIF of a played plot")

    # A plot with no moments cannot become a sequence, and the refusal says what
    # to write instead rather than leaving a one-frame file nobody asked for.
    try:
        save_gif(data(play_df) + point + x(col.x) + y(col.y),
                 os.path.join(folder, "still.gif"))
        raise AssertionError("save_gif() on an unplayed plot should refuse")
    except GogError as e:
        assert "does not play" in str(e) and "play(year)" in str(e), str(e)
        # A sequence is made of frames, the word every other `play()` message uses.
        assert "no frames to write" in str(e) and "moments" not in str(e), str(e)
    ok("`save_gif()` refuses a plot with no frames, with direction")

    # The name says what the file is, so a path that says otherwise is refused
    # rather than quietly corrected.
    try:
        save_gif(played, os.path.join(folder, "wave.png"))
        raise AssertionError("save_gif() should refuse a path that is not a .gif")
    except GogError as e:
        assert "ends in `.gif`" in str(e), str(e)
        # The correction keeps the directory that was asked for. R dropped it
        # here and the other three did not, so a reader was told to write into
        # the working directory while looking for the file somewhere else. The
        # refusal is worth nothing if the path it hands back is a different path.
        assert os.path.join(folder, "wave.gif") in str(e), str(e)
    ok("`save_gif()` refuses to write GIF bytes into another name")

# A second table naming its own positions — the per-layer position rule.
notes = {"at": [2.0], "val": [4.0], "what": ["peak"]}
svg = render_svg(
    data(df) + point + x(col.x) + y(col.y) + data(notes) + text + x(col.at) + y(col.val) + label(col.what)
)
assert "peak" in svg
ok("second table with its own positions rendered")

# A polar plot: same sentence, one more atom.
svg = render_svg(data(bar_df) + bar + x(col.category) + y(col.value) + polar)
assert "<path" in svg
ok("polar rendered (bare `polar`, no parentheses)")

# A date column becomes a time axis, not four category labels: `datetime.date`
# crosses the wire as epoch seconds with its resolution declared, exactly as an
# R `Date` does, and the ticks come back as a calendar.
svg = render_svg(data(days) + line + x(col.when) + y(col.level))
assert "Jan" in svg or "Feb" in svg, "a date column did not draw calendar ticks"
ok("date column rendered as a time axis")

# Missing values: the engine drops the row and says so on stderr.
svg = render_svg(data(gaps) + point + x(col.a) + y(col.b))
assert "<svg" in svg
ok("missing values dropped by the engine, plot still drawn")

# ---------------------------------------------------------------------------
# The refusals — a binding that accepts a sentence it cannot draw is worse
# than one that refuses it (spec §12).
# ---------------------------------------------------------------------------

refuses("a string where a column belongs", lambda: x("gdp"))
refuses("a value mapped as a channel", lambda: color("red"))
refuses("a column where a value belongs", lambda: style(color=col.group))
refuses("an arithmetic expression as a channel", lambda: x(col.x + col.y))
refuses("`style()` setting nothing", lambda: style())
refuses("an unknown setting", lambda: style(nonsense=1))

# One spelling of English, and the refusal has to say which. A reader arriving
# from ggplot2 types `colour` because there it works, so the refusal names the
# word to write rather than listing twelve settings to scan.
for british, american in (("colour", "color"), ("border_colour", "border_color"),
                          ("centre", "center")):
    try:
        style(**{british: "red"})
    except GogError as error:
        assert f"gog spells it `{american}`" in str(error), f"not named: {error}"
        assert "ggplot2" in str(error), f"ggplot2's difference unsaid: {error}"
        ok(f"the British spelling of `{american}` names its fix")
    else:
        raise AssertionError(f"FAIL: `style({british}=)` was accepted")
refuses("the British spelling of the color channel", lambda: colour(col.species))
refuses("a table with no `data()`", lambda: df + point)
refuses("atoms with no plot", lambda: point + x(col.x))
refuses("`facet()` joined with `+`", lambda: data(df) + point + x(col.x) + y(col.y) + facet(col.group))
refuses("a shadowed builtin called as one", lambda: range(120))
refuses("a bad scale name", lambda: x(col.x, scale="logarithmic"))

# `category` is the third scale chosen from the column's *type*, and since
# 2026-07-28 the third that may be said out loud for nothing — the allowance
# `linear` has on a number and `time` has on a date (spec §10). Byte-identical,
# because "means nothing extra" is a claim about the picture.
_cat = {"place": ["a", "b", "c"], "life": [4.0, 5.0, 6.0], "gdp": [1.0, 2.0, 3.0]}
assert render_svg(data(_cat, name="c") + bar * mean + x(col.place) + y(col.life)) == \
       render_svg(data(_cat, name="c") + bar * mean + x(col.place, scale="category") + y(col.life))
ok("saying `category` on a text column costs nothing")

# And it may not contradict the column: a scale says how a measured column is
# placed, and whether an axis measures at all is the column's type (§18).
refuses("`category` asked of a number column",
        lambda: render_svg(data(_cat, name="c") + point + x(col.gdp, scale="category") + y(col.life)))
refuses("a speed of zero", lambda: play(col.x, speed=0))
refuses(
    "one column naming both the frames and the panels",
    lambda: render_svg(
        data(play_df) + point + x(col.x) + y(col.y) + play(col.year) | facet(col.year)
    ),
)
refuses("a log base of 1", lambda: x(col.x, scale="log", base=1))
refuses("`bin()` given both a count and a width", lambda: bin(10, width=2))
refuses("a mixed-type column", lambda: render_svg(data({"m": [1.0, "two"]}, name="m") + point + x(col.m) + y(col.m)))
# The engine's own refusal, arriving through the bridge as a non-zero exit.
refuses("an illegal plot (bar with no y)", lambda: render_svg(data(bar_df) + bar + x(col.category)))

# ---------------------------------------------------------------------------
# `+` returns a new plot — R gets this from copy-on-modify, Python must not
# lose it, or two variants built from one base would silently share a layer.
# ---------------------------------------------------------------------------

base = data(df) + x(col.x) + y(col.y)
one = base + point
two = base + line
assert len(one._wire()[0]["layers"]) == 1
assert two._wire()[0]["layers"][0]["mark"] == "line"
assert base._wire()[0]["layers"] == [], "`+` mutated the plot on its left"
ok("`+` leaves the plot on its left unchanged")

# An unnamed table is an Assumption, said out loud (spec §12's omission rule).
with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always")
    anonymous = data({"a": [1.0], "b": [2.0]})
    assert any("no name" in str(w.message) for w in caught), "unnamed table warned nothing"
assert anonymous._wire()[0]["data"] == "data"
ok("an unnamed table is named `data`, with an Assumption")

# And a named variable keeps its name, which is what Law 4 resolves against.
assert (data(df) + point)._wire()[0]["data"] == "df"
ok("`data(df)` captures the table's name from the caller's frame")

# ---------------------------------------------------------------------------
# ordered() — a declared category order for a table that is only lists
# ---------------------------------------------------------------------------

# R says this with `factor()`; JavaScript and Julia each grew `ordered()` because
# neither language has one. Python was left out on the grounds that pandas has
# `Categorical` — true only for the users who have pandas, which `pyproject.toml`
# deliberately does not require. So the binding's *own* advertised table, a dict
# of lists, could not say Low < Medium < High at all.
severity = {"level": ordered(["High", "Low", "Medium"], ["Low", "Medium", "High"]),
            "count": [30.0, 10.0, 20.0]}
declared = re.findall(r">(High|Low|Medium)</text>",
                      render_svg(data(severity) + bar + x(col.level) + y(col["count"])))
assert declared == ["Low", "Medium", "High"], f"declared order dropped: {declared}"
ok("ordered() sets the category order")

# The same table without it, so the assertion above cannot pass vacuously: what
# it proves is a *difference*, and this is the fallback it differs from.
plain = {"level": ["High", "Low", "Medium"], "count": [30.0, 10.0, 20.0]}
rows = re.findall(r">(High|Low|Medium)</text>",
                  render_svg(data(plain) + bar + x(col.level) + y(col["count"])))
assert rows == ["High", "Low", "Medium"], f"plain text should read in row order: {rows}"
ok("a plain list still reads in the order the rows arrived")

refuses("`ordered()` given a value where a column belongs", lambda: ordered("Low", ["Low"]))

# ---------------------------------------------------------------------------
# theme() — the page rather than the ink (spec §7)
# ---------------------------------------------------------------------------

theme_df = {"g": ["Alpha", "Beta", "Gamma"], "v": [3.0, 7.0, 5.0],
            "side": ["Left", "Right", "Left"]}


def theme_lines(atom):
    return render_svg(data(theme_df) + bar + x(col.g) + y(col.v) + atom).count("<line")


assert theme_lines(theme(grid="none")) < theme_lines(style(opacity=1))
ok("theme(grid='none') drops the gridlines")

# The preset is resolved in the engine, not here — four bindings expanding one
# preset is four chances for them to disagree about what "minimal" means.
assert theme_lines(theme("minimal")) == theme_lines(theme(grid="none"))
ok("a named preset resolves in the engine")

# A preset a caller cannot adjust sends them back to knobs.
assert render_svg(data(theme_df) + bar + x(col.g) + y(col.v) + theme("minimal", ratio=1)) != \
       render_svg(data(theme_df) + bar + x(col.g) + y(col.v) + theme("minimal"))
ok("a preset can be adjusted")

assert "rotate" in render_svg(data(theme_df) + bar + x(col.g) + y(col.v) + theme(tick_angle=45))
ok("theme(tick_angle=) turns the x labels")

# One number, three sizes: the ticks take the number and the axis names and the
# title are a fixed step above it, so a plot's text is one decision.
def _font_sizes(svg):
    return sorted({float(v) for v in re.findall(r'font-size="([0-9.]+)"', svg)}, reverse=True)


def _typed(*atoms):
    p = data(theme_df) + bar + x(col.g) + y(col.v) + title("T")
    for a in atoms:
        p = p + a
    return render_svg(p)


assert _font_sizes(_typed()) == [16, 13, 11]
assert _font_sizes(_typed(theme(font_size=16))) == [23, 19, 16]
ok("theme(font_size=) is one number and three sizes")

# Asking for the size you already have must draw the plot you already had, or the
# default is an approximation of the scale rather than a point on it.
assert _typed(theme(font_size=11)) == _typed()
ok("theme(font_size=11) draws the untouched default")

refuses("theme('dark')",
        lambda: render_svg(data(theme_df) + bar + x(col.g) + y(col.v) + theme("dark")))
refuses("theme(grid='diag')", lambda: theme(grid="diag"))
refuses("theme(ratio=-1)", lambda: theme(ratio=-1))
refuses("theme(tick_angle=120)", lambda: theme(tick_angle=120))
refuses("theme() with nothing set", lambda: theme())
refuses("theme(frame='box')", lambda: theme(frame="box"))
# The mistake the pixel unit invites: reading the number as a multiplier.
refuses("theme(font_size=1.5)", lambda: theme(font_size=1.5))
refuses("theme(background='whte')",
        lambda: render_svg(data(theme_df) + bar + x(col.g) + y(col.v) + theme(background="whte")))

# A preset is only a bundle of properties a caller could set themselves. Faceted
# on purpose: this passed for the whole life of `theme("bw")` while the preset
# left gray strips over its white panels, because an unfaceted plot draws none.
_bw = render_svg((data(theme_df) + bar + x(col.g) + y(col.v) + theme("bw")) | facet(col.side))
assert _bw == render_svg(
    (data(theme_df) + bar + x(col.g) + y(col.v)
     + theme(background="white", frame="full", strip="white")) | facet(col.side))
ok("a preset is only a bundle of properties you could set yourself")

# The band above each panel is furniture too, and a gray tint prints badly.
assert "#e4e4ec" not in _bw
assert "#e4e4ec" in render_svg(
    (data(theme_df) + bar + x(col.g) + y(col.v)) | facet(col.side))
assert "seagreen" in render_svg(
    (data(theme_df) + bar + x(col.g) + y(col.v) + theme(strip="seagreen")) | facet(col.side))
ok("theme(strip=) colors the band, and `bw` covers it")

# The ink derives from the band, so `strip='black'` is a whole instruction rather
# than half of one: without this it prints near-black on near-black.
assert 'fill="#ffffff" text-anchor="middle"' in render_svg(
    (data(theme_df) + bar + x(col.g) + y(col.v) + theme(strip="black")) | facet(col.side))
assert 'fill="#3c3c46" text-anchor="middle"' in render_svg(
    (data(theme_df) + bar + x(col.g) + y(col.v)) | facet(col.side))
assert "gold" in render_svg(
    (data(theme_df) + bar + x(col.g) + y(col.v)
     + theme(strip="navy", strip_text="gold")) | facet(col.side))
ok("the strip's ink derives from its band, and a named one wins")
refuses("theme(strip_text='gld')", lambda: render_svg(
    (data(theme_df) + bar + x(col.g) + y(col.v) + theme(strip_text="gld")) | facet(col.side)))
refuses("theme(strip='whte')", lambda: render_svg(
    (data(theme_df) + bar + x(col.g) + y(col.v) + theme(strip="whte")) | facet(col.side)))

assert render_svg(data(theme_df) + bar + x(col.g) + y(col.v) + theme("bw")) == \
       render_svg(data(theme_df) + bar + x(col.g) + y(col.v) +
                  theme(background="white", frame="full"))
ok("a preset is only a bundle of properties you could set yourself")

# The furniture goes black and white; the data does not.
assert "#" in render_svg(data(theme_df) + bar + x(col.g) + y(col.v) + color(col.g) + theme("bw"))
ok("`bw` is the furniture, never the data")

# ---------------------------------------------------------------------------
# limits — the domain, when the data is not the authority (spec §10)
# ---------------------------------------------------------------------------

hrs = {"hour": [1, 4, 7, 10, 13, 16, 19, 22], "n": [2, 5, 9, 14, 20, 15, 8, 3]}

# The forcing case: a periodic axis cannot tell that a variable is periodic, so
# the period is stated — and a stated end is flush, or the circle would not close.
cycle = render_svg(data(hrs, name="hrs") + line + x(col.hour, limits=(0, 24)) + y(col.n) + polar)
assert ">0</text>" in cycle, "a stated cycle should reach its start"
ok("limits hold a polar axis open to its period")

# Extending drops nothing; restricting is the instruction, so it draws and
# reports rather than refusing — the one place this parts from `scale="log"`.
with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always")
    cut = render_svg(data(hrs, name="hrs") + point + x(col.hour, limits=(0, 10)) + y(col.n))
assert "<circle" in cut, "a restricted plot should still draw"
ok("a restricted domain still draws")

# A domain that keeps no row is the empty panel, and that is fatal.
refuses("a domain that keeps no row",
        lambda: render_svg(data(hrs, name="hrs") + point + x(col.hour, limits=(100, 200)) + y(col.n)))

# `limits` reaches every channel that measures, not only the axes (Law 1).
assert render_svg(data(hrs, name="hrs") + point + x(col.hour) + y(col.n) + color(col.n, limits=(0, 100))) != \
       render_svg(data(hrs, name="hrs") + point + x(col.hour) + y(col.n) + color(col.n, limits=(0, 200)))
ok("limits reach the color ramp, not just the axes")

# A category has no range to lie inside; the refusal points at `order`.
refuses("limits on a categorical axis",
        lambda: render_svg(data({"g": ["a", "b"], "v": [1.0, 2.0]}, name="cats") +
                           bar + x(col.g, limits=(0, 5)) + y(col.v)))

# --- the named palettes, and what centers a diverging one -------------------
# `limits` doing double duty is the ruling here: a diverging ramp has no
# midpoint parameter, because the middle of a stated domain already is one.
# The data is one-sided (0..40) so the two readings differ visibly.
signed = {"a": [1.0, 2, 3, 4, 5], "b": [1.0, 2, 3, 4, 5], "d": [0.0, 10, 20, 30, 40]}
fills = lambda s: set(re.findall(r'<circle[^>]*fill="([^"]*)"', s))

for name, dark in [("magma", "#000004"), ("inferno", "#000004"), ("plasma", "#0d0887"),
                   ("cividis", "#00204d"), ("gray", "#a9a9a9")]:
    drawn = fills(render_svg(data(signed, name="signed") + point + x(col.a) + y(col.b)
                             + color(col.d) + palette(name)))
    assert dark in drawn, f"palette({name!r}) did not reach the output: {sorted(drawn)}"
    assert "#8faed5" not in drawn, f"palette({name!r}) fell back to the blue ramp"
ok("the sequential ramps each render as themselves")

for name in ("blue_red", "brown_teal"):
    drawn = fills(render_svg(data(signed, name="signed") + point + x(col.a) + y(col.b)
                             + color(col.d, limits=(-40, 40)) + palette(name)))
    assert "#a9a9a9" in drawn, f"{name} put nothing on the neutral at zero: {sorted(drawn)}"
    assert not {"#004383", "#6b3d10"} & drawn, \
        f"{name} reached its low end on data that never goes negative"
ok("symmetric limits put zero on a diverging ramp's neutral")

assert "#004383" in fills(render_svg(data(signed, name="signed") + point + x(col.a)
                                     + y(col.b) + color(col.d) + palette("blue_red"))), \
    "an unstated domain should fit the ramp to the data, low end included"
ok("without limits the ramp fits the data and zero is not the center")

# `gray` is in the vocabulary and `grey` is not, which is the American-English
# rule enforced at the door rather than merely obeyed inside it.
refuses("the British spelling of a palette",
        lambda: render_svg(data(signed, name="signed") + point + x(col.a) + y(col.b)
                           + color(col.d) + palette("grey")))

# `soft` is the muted categorical set, and it reaches a *fill* — which is the
# geometry it exists for, so testing it on a point would miss the point.
cats = {"g": ["a", "b", "a", "c"], "v": [1.0, 2.0, 3.0, 4.0]}
bars = render_svg(data(cats, name="cats") + bar * count + x(col.g) + color(col.g)
                  + palette("soft"))
assert "#66c2a5" in bars, "palette('soft') did not reach the bars"
assert "#4e79a7" not in bars, "palette('soft') fell back to the default palette"
ok("palette('soft') paints the fills")

# --- a color bound to a level by name, and a legend turned off (spec §10) ----
# The column meets its levels Asia, Europe, Africa; the names are written in
# another order, which is the reason to write them at all.
lvl = {"a": [1.0, 2, 3, 4, 5, 6], "b": [3.0, 1, 4, 1, 5, 9],
       "g": ["Asia", "Europe", "Africa", "Asia", "Europe", "Africa"]}
named_pal = palette({"Africa": "seagreen", "Europe": "steelblue", "Asia": "tomato"})
assert named_pal.fields["value"] == {"levels": {"Africa": "seagreen", "Europe": "steelblue",
                                                "Asia": "tomato"}}, named_pal.fields


def _refusal(thunk) -> str:
    """The refusal's text, for a test that checks what it says."""
    try:
        thunk()
    except GogError as error:
        return str(error)
    raise AssertionError("FAIL: accepted, and should have been refused")


def _swatch(svg: str, level: str) -> str:
    """The last color written before a legend row's name is that row's swatch."""
    before = svg[:svg.index(f">{level}</text>")]
    return re.findall(r'fill="([^"]+)"', before)[-1]


_named = render_svg(data(lvl, name="lvl") + point + x(col.a) + y(col.b) + color(col.g) + named_pal)
for level, want in [("Asia", "tomato"), ("Europe", "steelblue"), ("Africa", "seagreen")]:
    assert _swatch(_named, level) == want, f"{level}: {_swatch(_named, level)}"
assert _named.index(">Asia</text>") < _named.index(">Africa</text>"), \
    "the key should run in the column's order"
ok("palette({'Asia': ...}) gives each level the color beside its name")

_m = _refusal(lambda: render_svg(data(lvl, name="lvl") + point + x(col.a) + y(col.b)
                                 + color(col.g) + palette({"Africa": "seagreen",
                                                           "Europe": "steelblue",
                                                           "Asai": "tomato"})))
for want in ['names "Asai", and `g` has no level called that', 'Did you mean "Asia"?',
             '"Asia", "Europe", and "Africa"']:
    assert want in _m, f"the misspelling refusal should say {want!r}: {_m}"
_m = _refusal(lambda: render_svg(data(lvl, name="lvl") + point + x(col.a) + y(col.b)
                                 + color(col.g) + palette({"Asia": "tomato",
                                                           "Europe": "steelblue"})))
assert 'leaves out "Africa"' in _m, _m
_m = _refusal(lambda: render_svg(data(lvl, name="lvl") + point + x(col.a) + y(col.b)
                                 + color(col.b) + palette({"Asia": "tomato"})))
assert "numbers have no levels to name" in _m, _m
refuses("a named palette whose name is not text", lambda: palette({1: "tomato"}))
ok("a named palette refuses a name that is no level, and a level with no name")

# `legend=False`: the channel still maps, its key is not drawn, and nothing is said.
_keyed = render_svg(data(lvl, name="lvl") + point + x(col.a) + y(col.b) + color(col.g))
with warnings.catch_warnings(record=True) as _said:
    warnings.simplefilter("always")
    _err = io.StringIO()
    with contextlib.redirect_stderr(_err):
        _off = render_svg(data(lvl, name="lvl") + point + x(col.a) + y(col.b)
                          + color(col.g, legend=False))
assert not _said and not _err.getvalue(), f"legend=False should print nothing: {_said} {_err.getvalue()}"
assert ">Asia</text>" in _keyed and ">Asia</text>" not in _off, "legend=False drew the key"
assert "#4e79a7" in _off and "#f28e2b" in _off, "legend=False must not drop the mapping"
for _atom in (size(col.a, legend=False), opacity(col.a, legend=False), shape(col.g, legend=False)):
    _s = render_svg(data(lvl, name="lvl") + point + x(col.a) + y(col.b) + _atom)
    assert 'rx="4"' not in _s, f"{_atom.kind}(legend=False) drew a key"
_s = render_svg(data(lvl, name="lvl") + bar * count + x(col.g) + pattern(col.g, legend=False))
assert 'rx="4"' not in _s, "pattern(legend=False) drew a key"
ok("legend=False leaves the key out of all five channels and keeps the mapping")

_m = _refusal(lambda: render_svg(data(lvl, name="lvl") + point + x(col.a, legend=False)
                                 + y(col.b)))
assert "`x(a, legend = FALSE)` — `x` is read off its axis" in _m, _m
_m = _refusal(lambda: render_svg(data(lvl, name="lvl") + line + x(col.a) + y(col.b)
                                 + group(col.g, legend=False)))
assert "`group` splits the rows without encoding anything" in _m, _m
_m = _refusal(lambda: render_svg(data(lvl, name="lvl") + text + x(col.a) + y(col.b)
                                 + label(col.g, legend=False)))
assert "`label` is the text a `text` mark writes" in _m, _m
refuses("legend= that is not True or False", lambda: color(col.g, legend="no"))
ok("legend= on a channel with no key is refused with direction")

# Caught in the binding, at the line that wrote it.
refuses("a backwards domain", lambda: x(col.hour, limits=(20, 5)))
refuses("one number as a domain", lambda: x(col.hour, limits=5))

# `shape` measures nothing, so it offers no domain either — the same absence as
# `scale`, which is what makes it one rule rather than two lists. Refused by the
# signature, exactly as R's "unused argument" does it.
try:
    shape(col.g, limits=(0, 1))
    raise AssertionError("shape should take no limits")
except TypeError:
    ok("shape offers no limits to misuse")

# A domain on a temporal axis is written in dates, and the binding converts them
# the way it converts the column — otherwise the two disagree by a factor of
# 86400 and every row falls outside.
dts = {"day": [date(2024, 3, 1 + i) for i in builtins.range(20)],
       "orders": [float(20 + i) for i in builtins.range(20)]}
year = render_svg(data(dts, name="dts") + line + y(col.orders) +
                  x(col.day, limits=(date(2024, 1, 1), date(2024, 12, 31))))
assert ">Jan 2024</text>" in year and ">Nov 2024</text>" in year, \
    "a stated year should tick across the year"
ok("limits on a date axis are written in dates")


# ---------------------------------------------------------------------------
# surface — the sheet through the samples (spec §15)
#
# The engine tests pin the mesh against the lattice; these pin the *binding*: that
# `surface` is exported, that a grid table reaches the engine as a grid, and that
# the refusals a reader will actually hit arrive with direction.
# ---------------------------------------------------------------------------

# One row per (x, y) crossing is the mark's whole contract with the caller, and
# writing the grid out by hand is how Python says `expand.grid`.
_side = [-3.0 + 6.0 * i / 14 for i in builtins.range(15)]
surf = {"gx": [x for x in _side for _ in _side],
        "gy": [y for _ in _side for y in _side]}
surf["h"] = [math.sin(math.sqrt(a * a + b * b) + 1e-9) / math.sqrt(a * a + b * b + 1e-9)
             for a, b in zip(surf["gx"], surf["gy"])]

sheet = render_svg(data(surf, name="surf") + surface + x(col.gx) + y(col.gy) + z(col.h))
assert sheet.count('<path d="M') == 196, "a 15x15 grid of nodes is 14x14 faces"
ok("one face per complete cell of the grid")

# Binding `z` is what puts a plot in the cube, so a surface needs no `space()` —
# and `space()` still sets the angle, which must change the picture.
assert sheet != render_svg(data(surf, name="surf") + surface + x(col.gx) + y(col.gy) +
                           z(col.h) + space(turn=110, tilt=40))
ok("`z` is the trigger and `space()` sets the angle")

# The mesh lines: the seam hairline each face already carried, handed to the caller.
assert 'stroke="white"' in render_svg(
    data(surf, name="surf") + surface + x(col.gx) + y(col.gy) + z(col.h) +
    style(border_color="white", border_size=0.6))
ok("style(border_color=) is the wireframe over the sheet")

# A mapped opacity fades the sheet face by face — `color`'s per-face reading, one
# channel over. A mesh has parts small enough to each hold one value, so it can hold
# this one, and the sheet then thins where its measure is small.
_faded = render_svg(data(surf, name="surf") + surface + x(col.gx) + y(col.gy) +
                    z(col.h) + opacity(col.h))
assert len(set(re.findall(r'fill-opacity="([0-9.]+)"', _faded))) >= 20, \
    "a mapped opacity should fade face by face"
ok("opacity maps on a surface, one value per face")

# A flat surface is one failure, not two, and the direction names both routes in.
refuses("a surface with no height",
        lambda: render_svg(data(surf, name="surf") + surface + x(col.gx) + y(col.gy)))

# A scatter is the empty panel this refusal exists to prevent.
scat = {"sx": [(i * 37 % 101) / 101 for i in builtins.range(60)],
        "sy": [(i * 53 % 97) / 97 for i in builtins.range(60)],
        "sh": [(i * 29 % 89) / 89 for i in builtins.range(60)]}
refuses("a scatter drawn as a surface",
        lambda: render_svg(data(scat, name="scat") + surface + x(col.sx) + y(col.sy) + z(col.sh)))

# And the sentence that refusal advises must draw: the field raised, no `z()`.
est = render_svg(data(scat, name="scat") + surface * density + x(col.sx) + y(col.sy) + space)
assert est.count('<path d="M') > 100, "surface * density should raise a mesh"
ok("surface * density raises the estimated field, with no z() bound")

# `bin` cuts the floor into adjacent cells and the sheet lays a flat lid on each —
# the terraced surface, for a design that measures one value per cell. A 3x3 grid
# read as *nodes* is 2x2 blocks of four corners, so four faces; read as cells it is
# nine lids plus the twelve risers that connect them.
_t = [-2.0, 0.0, 2.0]
terr = {"ta": [a for a in _t for _ in _t], "tb": [b for _ in _t for b in _t]}
terr["tv"] = [a * a + b * b for a, b in zip(terr["ta"], terr["tb"])]

nodes = render_svg(data(terr, name="terr") + surface + x(col.ta) + y(col.tb) + z(col.tv))
assert nodes.count('<path d="M') == 4, "nine nodes are four faces"
lids = render_svg(data(terr, name="terr") + surface * bin(3) * mean +
                  x(col.ta) + y(col.tb) + z(col.tv))
assert lids.count('<path d="M') == 21, "nine cells are 9 lids + 12 risers"
ok("a cut floor lays one plateau per cell where nodes span the gaps")

# What is still refused is a floor of *slots*: categories leave air between them,
# and tiles that float apart are not a sheet.
refuses("surface * count over categorical slots",
        lambda: render_svg(data(scat, name="scat") + surface * count + x(col.sx) + y(col.sy) + space))

# A face spans the gap between two samples; two categories have no gap to span.
cats = dict(surf)
cats["band"] = ["low" if i % 2 else "high" for i in builtins.range(len(surf["gx"]))]
refuses("a category on a surface's floor",
        lambda: render_svg(data(cats, name="cats") + surface + x(col.band) + y(col.gy) + z(col.h)))


# ---------------------------------------------------------------------------
# tick_count — how many ticks an axis aims for (spec §10)
#
# The last property that was real in the IR, read by the renderer, and reachable
# from no binding. It rides the binding beside `scale` and `limits` because it
# describes the **scale**; `theme()` declined it on that ground (§7).
# ---------------------------------------------------------------------------

grid5 = {"a": [0.0, 25.0, 50.0, 75.0, 100.0], "b": [1.0, 2.0, 3.0, 4.0, 5.0]}


def _ticks(plot):
    svg = render_svg(plot)
    return re.findall(r">([-0-9.]+)</text>", svg)


# A target rather than a promise: the count picks a step and the step is rounded
# to a human number. So the claim is monotone rather than exact.
few = _ticks(data(grid5, name="g5") + point + x(col.a, tick_count=3) + y(col.b))
many = _ticks(data(grid5, name="g5") + point + x(col.a, tick_count=11) + y(col.b))
assert len(many) > len(few), f"tick_count changed nothing: {len(few)} vs {len(many)}"
ok(f"an axis draws more ticks when asked for more ({len(few)} -> {len(many)})")

# Thinning the labels is not coarsening the step: a sparse axis's ticks are a
# subset of a dense one's, so a value read off either is on the same scale.
assert set(few) <= set(many), f"a sparse axis invented labels: {set(few) - set(many)}"
ok("a sparse axis's ticks are a subset of a dense one's")

# A legend is not a short axis: `limits` reaches all six magnitude channels,
# `tick_count` only the three that draw an axis. Refused by the signature.
try:
    color(col.a, tick_count=4)
    raise AssertionError("color should take no tick_count")
except TypeError:
    ok("a legend has no tick count to ask for")

# Caught in the binding, at the line that wrote it.
refuses("a tick count below two", lambda: x(col.a, tick_count=1))
refuses("a fractional tick count", lambda: x(col.a, tick_count=2.5))
refuses("a tick count that is not a number", lambda: x(col.a, tick_count="8"))

# A category axis has one tick per level, so the count is the data's.
refuses("tick_count on a categorical axis",
        lambda: render_svg(data({"g": ["a", "b"], "v": [1.0, 2.0]}, name="cats2") +
                           bar + x(col.g, tick_count=5) + y(col.v)))

# One axis, one count — a layer stating its own is the plot-scoped-scale rule.
refuses("a layer stating its own tick count",
        lambda: render_svg(data(grid5, name="g5") + x(col.a, tick_count=4) + y(col.b) +
                           point + x(col.a, tick_count=9)))

# A count past the most one axis draws (26) widens the step rather than cutting
# the axis short. The cut kept the first 26 ticks: 40 on gdp labeled 0K to 25K on
# an axis that runs to 49K, and said nothing.
wide = {"gdp": [277.55, 12000.0, 49357.19], "life": [40.0, 60.0, 82.0]}
with contextlib.redirect_stderr(io.StringIO()) as said:
    svg = render_svg(data(wide, name="wide") + point + x(col.gdp, tick_count=40) + y(col.life))
labels = re.findall(r">([^<>]*)</text>", svg)
assert "48K" in labels and "25K" not in labels, \
    f"a count past the maximum should widen the step to the far end: {labels}"
assert "a tick every 2K instead" in said.getvalue(), f"a widened step went unreported: {said.getvalue()!r}"
with contextlib.redirect_stderr(io.StringIO()) as quiet:
    render_svg(data(wide, name="wide") + point + x(col.gdp, tick_count=26) + y(col.life))
assert "ticks on this axis" not in quiet.getvalue(), f"a count that fits said: {quiet.getvalue()!r}"
ok("a count past the maximum widens the step to the far end, and says so")

# ---------------------------------------------------------------------------
# Polar — every mark that draws flat draws bent (spec §15)
#
# Five marks were refused in this space until 2026-07-26 on one recorded ground,
# *their straight edges would have to become arcs*. Three never needed one. What
# each check pins is the property the refusal was really about: a segment that
# **holds** a value across a span must follow the ring, since a chord falls
# inside the circle and puts the mark where the data is not.
# ---------------------------------------------------------------------------

wind = {
    "dir": ["N"] * 6 + ["E"] * 6 + ["S"] * 6 + ["W"] * 6,
    "spd": [4.0, 5, 6, 5, 4, 6, 8, 9, 11, 10, 9, 8,
            6, 7, 5, 6, 7, 6, 3, 4, 2, 3, 4, 3],
    "season": ["Summer", "Winter"] * 12,
}
band = {"dir": ["N", "E", "S", "W"], "lo": [2.0, 6, 4, 1], "hi": [6.0, 11, 8, 5]}


def arcs(svg: str) -> int:
    return svg.count(" A ")


for name, plot in [
    ("step",     data(wind, name="wind") + step * mean + x(col.dir) + y(col.spd) + polar),
    ("interval", data(wind, name="wind") + interval * range + x(col.dir) + y(col.spd) + polar),
    ("box",      data(wind, name="wind") + box + x(col.dir) + y(col.spd) + polar),
    ("ribbon",   data(band, name="band") + ribbon * bounds(col.lo, col.hi) + x(col.dir) + polar),
    ("zone",     data(wind, name="wind") + zone * count + x(col.dir) + y(col.season) + polar),
]:
    out = render_svg(plot)
    assert "<svg" in out and "NaN" not in out, f"{name} does not draw in polar"
ok("all five span marks draw in polar")

# A stair's treads become arcs; a flat one draws none. The segment the whole
# space was waiting on, and the only genuinely new geometry in the change.
assert arcs(render_svg(data(wind, name="wind") + step * mean + x(col.dir) + y(col.spd) + polar)) > 0
assert arcs(render_svg(data(wind, name="wind") + step * mean + x(col.dir) + y(col.spd))) == 0
ok("a stair's treads are arcs bent and straight flat")

# A band's boundaries are **chords** — the correction to the recorded refusal.
assert arcs(render_svg(data(band, name="band") + ribbon * bounds(col.lo, col.hi) + x(col.dir) + polar)) == 0
ok("a radar band is drawn with chords, not arcs")

# A hexagonal mesh has no polar reading — `bin(tiling = )`'s third refusal.
# `range` here is gog's transform — the collision this binding warns about, and
# the reason `builtins` is imported at the top of the file.
mesh = {"a": [float(i % 6) for i in builtins.range(36)],
        "b": [float(i // 6) for i in builtins.range(36)]}
refuses("a hex mesh in polar",
        lambda: render_svg(data(mesh, name="mesh") + zone * bin(tiling="hex") +
                           x(col.a) + y(col.b) + polar))
assert arcs(render_svg(data(mesh, name="mesh") + zone * bin(tiling="rect") +
                       x(col.a) + y(col.b) + polar)) > 0
ok("hex is refused in polar and rect bends into sectors")


# ---------------------------------------------------------------------------
# Nest — the panel packed with regions (spec §15)
#
# The third answer to what carries a share: length flat, angle in polar, area
# here. What is checked is the property a treemap is read for — the regions are
# the panel and each is its own share of it — plus the refusals the space owns.
# ---------------------------------------------------------------------------

sales = {"region": ["North", "North", "South", "South", "East", "East", "West"],
         "product": ["widgets", "gadgets", "widgets", "gadgets", "widgets", "gadgets", "widgets"],
         "revenue": [32.0, 14, 25, 8, 19, 11, 6]}


def cells(svg: str) -> list:
    """Every packed cell as (x, y, w, h).

    The legend's swatches carry `rx=` and the outer region outlines are
    `fill="none"`; neither is a cell. The leading space in each key matters —
    without it `width=` also matches `stroke-width=`.
    """
    out = []
    for line in svg.splitlines():
        if "<rect" not in line or "fill-opacity" not in line:
            continue
        if "rx=" in line or 'fill="none"' in line:
            continue
        out.append(tuple(float(line.split(f' {k}="')[1].split('"')[0])
                         for k in ("x", "y", "width", "height")))
    return out


one = render_svg(data(sales, name="sales") + bar * sum + y(col.revenue) + color(col.region) + nest())
cl = cells(one)
assert len(cl) == 4, f"expected one region per region-name, got {len(cl)}"
total = builtins.sum(c[2] * c[3] for c in cl)
shares = sorted(c[2] * c[3] / total for c in cl)
# North 46, South 33, East 30, West 6 — of 115.
for got, want in zip(shares, sorted(v / 115 for v in (46, 33, 30, 6))):
    assert abs(got - want) < 0.002, f"region got {got:.4f} of the panel, wanted {want:.4f}"
ok("every packed region is its share of the panel")

assert 'stroke="#5a5a64"' not in one, "a packed panel drew axis lines"
flat_one = render_svg(data(sales, name="sales") + bar * sum + x(col.region) + y(col.revenue) + color(col.region))
assert 'stroke="#5a5a64"' in flat_one, "the flat sentence drew no axes, so the test proves nothing"
ok("a packed panel draws no axes and the flat one does")

two = render_svg(data(sales, name="sales") + bar * sum + x(col.region) + y(col.revenue) +
                 color(col.product) + nest())
outer = [l for l in two.splitlines() if "<rect" in l and 'fill="none"' in l]
assert len(outer) == 4, f"expected one outline per region, got {len(outer)}"
assert not [l for l in one.splitlines() if "<rect" in l and 'fill="none"' in l], \
    "a one-level packing outlined a region against nothing"
ok("a bound position packs a second level inside each region")

refuses("a collision modifier in a packed panel",
        lambda: render_svg(data(sales, name="sales") + bar * sum * stack + y(col.revenue) +
                           color(col.region) + nest()))
refuses("naming an axis a packed panel does not have",
        lambda: render_svg(data(sales, name="sales") + bar * sum + y(col.revenue) +
                           color(col.region) + nest() + x_label("Revenue")))
refuses("a point in a packed panel",
        lambda: render_svg(data(sales, name="sales") + point + x(col.revenue) + y(col.revenue) + nest()))
refuses("a log scale on a packed measure",
        lambda: render_svg(data(sales, name="sales") + bar * sum + y(col.revenue, scale="log") +
                           color(col.region) + nest()))

# A label at the center of its own region — what makes a packing readable once
# the split is too wide for a legend to decode (2026-07-27). The label layer
# needs no `x`: a packing places by region, which is Law 7's third relaxation.
#
# Every local below is prefixed, and the loop variable is `row` rather than the
# obvious `line`: `line` is a **mark**, and rebinding it here shadows the atom for
# the rest of the file — which is exactly the collision the `text` comment two
# hundred lines down records, caught a second time the day it was written.
packed_svg = render_svg(data(sales, name="sales") + bar + y(col.revenue) + color(col.region) +
                        text + label(col.product) + nest())
# A mark's label carries `fill-opacity` and the legend's key entries do not —
# the same discriminator `cells()` uses one element over, and needed for the same
# reason: the key spells out the very strings the labels draw, so counting them
# as labels would pass whether or not the mark drew anything.
packed_names = [row for row in packed_svg.splitlines()
                if row.strip().startswith("<text") and "fill-opacity" in row]
assert packed_names, "a packed label drew nothing"
# Every drawn label sits inside a cell the bar drew, which is the property that
# makes the mark worth having: the two marks read one packing, so a name cannot
# land in a rectangle its own row did not get.
packed_boxes = cells(packed_svg)
for row in packed_names:
    lx = float(row.split('<text x="')[1].split('"')[0])
    assert any(bx <= lx <= bx + bw for bx, _, bw, _ in packed_boxes), \
        f"a label landed outside every region: {row}"
ok("a packed label sits inside its own region")

refuses("a nudge in a packed panel, where a label covers no point",
        lambda: render_svg(data(sales, name="sales") + bar + y(col.revenue) + color(col.region) +
                           text + label(col.product) + style(nudge="up") + nest()))

# ---------------------------------------------------------------------------
# Space — the three slot marks stand on the cube's floor (spec §15)
#
# `interval` and `box` joined `bar` in the cube on 2026-07-26 and needed no
# ruling of their own: `is_slot_mark` had grouped the three since orientation
# was decided. The cube's remaining blanks are the other half — four *decided*
# refusals and two blocked on occlusion, and until this change every one of them
# said "not drawn yet".
# ---------------------------------------------------------------------------

plots = {
    "site":   ["North"] * 20 + ["Center"] * 20 + ["South"] * 20,
    "season": ["Wet", "Dry"] * 30,
    "yield":  [50.0 + (i % 7) for i in builtins.range(20)]
              + [58.0 + (i % 5) for i in builtins.range(20)]
              + [46.0 + (i % 9) for i in builtins.range(20)],
}

for name, plot in [
    ("interval", data(plots, name="plots") + interval * range + x(col.site) + y(col.season) + z(col["yield"]) + space),
    ("conf",     data(plots, name="plots") + interval * confidence + x(col.site) + y(col.season) + z(col["yield"]) + space),
    ("box",      data(plots, name="plots") + box + x(col.site) + y(col.season) + z(col["yield"]) + space),
]:
    out = render_svg(plot)
    assert "<svg" in out and "NaN" not in out, f"{name} does not stand in the cube"
ok("interval and box stand on the cube's floor")

# One per **cell**, not one per row: six cells, each a span plus a crossed cap at
# either end — 6 x 5 = 30 strokes carrying a linecap.
whiskers = render_svg(data(plots, name="plots") + interval * range
                      + x(col.site) + y(col.season) + z(col["yield"]) + space)
assert whiskers.count("stroke-linecap") == 30, whiskers.count("stroke-linecap")
ok("a pair transform in the cube groups by the floor")

# A decided refusal states its ruling and does not promise a renderer.
try:
    render_svg(data(plots, name="plots") + line + x(col["yield"]) + y(col["yield"]) + z(col["yield"]) + space)
    raise AssertionError("FAIL: a 3-D line should be refused")
except GogError as error:
    # Not `text`: that is a **mark**, and binding it here shadowed the atom for
    # every line below in a module-level `except` block. Found 2026-07-27 by the
    # first sentence in this file to use `text` after line 600, and it is the same
    # class of collision spec §8 records for `order` — a kernel word is a kernel
    # word even in a test.
    said = str(error)
    assert "no left to right" in said, said
    assert "not drawn yet" not in said and "does not draw it yet" not in said, said
    assert "path" in said, said
ok("a 3-D line is refused with its ruling, not with a promise")

# The two blocked on occlusion say *that*, which is a different sentence.
refuses("a rule in the cube",
        lambda: render_svg(data(plots, name="plots") + rule + x(col["yield"]) + z(col["yield"]) + space))


# ---------------------------------------------------------------------------
# The composed cut — which transform owns the measurement (spec §5)
#
# `bin` says where the cells are *and* what is in them, and only the first is
# what makes it a `bin`. Composed with a statistic it keeps the cut and gives
# the tally up: the binned mean profile, and the summary heatmap one dimension
# up. The other three synthesizing transforms measure without cutting, so there
# is nothing left of them to compose.
# ---------------------------------------------------------------------------

cut = render_svg(data(df, name="df") + bar * bin * mean + x(col.x) + y(col.y))
assert "<svg" in cut
ok("the composed cut draws the binned mean profile")

# Order cannot decide anything here: a cell has to exist before it is measured.
assert cut == render_svg(data(df, name="df") + bar * mean * bin + x(col.x) + y(col.y))
ok("the cut runs first wherever it is written")

# And the statistic must reach the plot. Until 2026-07-26 it did not: `bin`
# overwrote the named column with its own tally, the reduction handed that back
# unchanged, and only the axis *title* changed — a histogram labeled `Life`.
import re as _re
_strip = lambda s: _re.sub(r"<text[^<]*</text>", "", s)
assert _strip(cut) != _strip(render_svg(data(df, name="df") + bar * bin + x(col.x)))
ok("the composed statistic changes what is measured, not just the axis label")

refuses("count composed with a statistic",
        lambda: render_svg(data(df, name="df") + bar * count * mean + x(col.group) + y(col.y)))
refuses("density composed with a statistic",
        lambda: render_svg(data(df, name="df") + bar * density * mean + x(col.x) + y(col.y)))
refuses("two synthesizing transforms",
        lambda: render_svg(data(df, name="df") + bar * bin * count + x(col.x)))
refuses("smooth composed with a cut",
        lambda: render_svg(data(df, name="df") + bar * bin * smooth + x(col.x) + y(col.y)))


# ---------------------------------------------------------------------------
# `proportion` is a normalizer, and `stack(share=)` fills a pile (spec §5)
# ---------------------------------------------------------------------------

# Read the drawn heights back as data values through the axis's own two ticks.
# Comparing the bars *with each other* is the point: the defect behind this
# session was twelve equal bars at 1/12, and the check that missed it read only
# the axis range. A range is not a shape.
def bar_values(spec):
    s = render_svg(spec)
    ticks = re.findall(r'<text x="([0-9.]+)" y="([0-9.]+)">([0-9.]+)</text>', s)
    # The y ticks share an x; the x ticks share a y. Take the commonest x rather
    # than a pixel threshold, which a short x label slips under.
    xs = [t[0] for t in ticks]
    axis = builtins.max(set(xs), key=xs.count)
    ticks = [t for t in ticks if t[0] == axis]
    per_px = (float(ticks[1][2]) - float(ticks[0][2])) / (float(ticks[0][1]) - float(ticks[1][1]))
    heights = [float(h) for h in re.findall(r'<rect[^>]*height="([0-9.]+)"[^>]*fill-opacity', s)]
    return [h * per_px for h in heights if h != 12.0]   # drop legend swatches


share = {
    "dir": ["N"] * 6 + ["E"] * 10 + ["S"] * 4 + ["W"] * 20,
    # Uneven inside each slot as well as between them: an alternating split makes
    # every slot 50/50, which a fill that ignored the values would also draw.
    "season": (["Su"] * 4 + ["Wi"] * 2 + ["Su"] * 3 + ["Wi"] * 7 +
               ["Su"] * 1 + ["Wi"] * 3 + ["Su"] * 15 + ["Wi"] * 5),
    "v": [float(i) for i in builtins.range(1, 41)],
}
# Skewed on purpose: a uniform column binned evenly gives near-equal bars, the
# one shape this test must be able to tell apart from the 1/12 defect.
skew = {"v": [float(builtins.round(math.exp(i * 4.6 / 199))) for i in builtins.range(200)]}

# 1. Unchanged: a bare `proportion` sums to 1.
assert abs(builtins.sum(bar_values(data(share, name="share") + bar * proportion + x(col.dir))) - 1) < 0.01

# 2. The fix. A `color` split used to give each group its own denominator, so the
#    plot summed to 2 — two conditional distributions, where §5 had always said
#    the word means a share of the whole frame (Law 6).
split = builtins.sum(bar_values(data(share, name="share") + bar * proportion +
                                x(col.dir) + color(col.season)))
assert abs(split - 1) < 0.01, f"a split `proportion` summed to {split}"
ok("`proportion` normalizes over the whole frame, split or not")

# 3. The relative-frequency histogram, refused for one day as two synthesizing
#    transforms. The bars must *differ* — all-equal is the 1/12 defect itself.
h = bar_values(data(skew, name="skew") + bar * bin(12) * proportion + x(col.v))
assert len(h) == 12, f"expected 12 bars, got {len(h)}"
assert abs(builtins.sum(h) - 1) < 0.01, f"shares summed to {builtins.sum(h)}"
assert len(set(builtins.round(v, 3) for v in h)) > 1, "twelve equal bars — the 1/12 defect is back"
n = bar_values(data(skew, name="skew") + bar * bin(12) + x(col.v))
assert builtins.max(abs(c / builtins.sum(n) - s) for c, s in zip(n, h)) < 0.01
ok("the relative-frequency histogram is the histogram's counts over n")

# 4. `stack(share=True)` fills every pile to exactly 1, whatever measured it.
tops = bar_values(data(share, name="share") + bar * count * stack(share=True) +
                  x(col.dir) + color(col.season))
half = len(tops) // 2
for i in builtins.range(half):
    assert abs(tops[i] + tops[i + half] - 1) < 0.01, f"a filled pile reached {tops[i] + tops[i + half]}"
assert len(set(builtins.round(v, 3) for v in tops)) > 1, "the fill lost the composition"
# It composes with any measurement, which is why it is a `stack` parameter and
# not a second reading of `proportion`: there is no column for `proportion` to sum.
render_svg(data(share, name="share") + bar * sum * stack(share=True) +
           x(col.dir) + y(col.v) + color(col.season))
refuses("stack(share=) with a number", lambda: stack(share=1))
ok("`stack(share=True)` fills every pile to 1, on any measurement")

# `stack(baseline=)` says where the pile hangs — the streamgraph. A displaced pile
# draws no numbers on the measure axis, because no value on it corresponds to a
# measurement once the foot has moved.
flows = {"t": [1.0, 2, 3, 4, 5, 6] * 3,
         "g": ["a"] * 6 + ["b"] * 6 + ["c"] * 6,
         "v": [4.0, 9, 3, 8, 2, 7, 5, 5, 5, 5, 5, 5, 2, 3, 9, 2, 8, 3]}
_plain = render_svg(data(flows, name="flows") + area * stack +
                    x(col.t) + y(col.v) + color(col.g))
_strm = render_svg(data(flows, name="flows") + area * stack(baseline="wiggle") +
                   x(col.t) + y(col.v) + color(col.g))
_ticks = lambda s: [t for t in re.findall(r">([^<>]+)</text>", s)
                    if re.fullmatch(r"-?[0-9.]+", t)]
assert len(_ticks(_plain)) > len(_ticks(_strm)), \
    "a displaced pile should drop its measure-axis numbers"
assert _ticks(_strm), "the domain axis lost its numbers too"
assert _plain.count("<polygon") == _strm.count("<polygon"), \
    "a displaced pile drew a different number of bands"
refuses("stack(baseline=) with a number", lambda: stack(baseline=1))
refuses("a baseline that is not one of the three",
        lambda: render_svg(data(flows, name="flows") + area * stack(baseline="sym") +
                           x(col.t) + y(col.v) + color(col.g)))
refuses("a displaced pile in polar",
        lambda: render_svg(data(flows, name="flows") + area * stack(baseline="center") +
                           x(col.t) + y(col.v) + color(col.g) + polar()))
ok("`stack(baseline=)` hangs the pile, and a displaced axis draws no numbers")

# A *composed* `proportion` synthesizes nothing, so its `y` names an input column
# and a misspelling of it must still be caught. Found by a reader looking at a
# plot: `bar * sum * proportion + y(pop)` — `pop` renamed `population` in the
# book's own data — drew an empty panel on fabricated 0..1 axes.
refuses("a misspelled column under a composed proportion",
        lambda: render_svg(data(share, name="share") + bar * sum * proportion +
                           x(col.dir) + y(col.nosuchcolumn)))
# …while a bare `proportion` still names the column it writes.
render_svg(data(share, name="share") + bar * proportion + x(col.dir) + y(col.whatever))
ok("a composed `proportion` still checks the column it rescales")


# --- `repel`: the fourth offset, and the one that moves ink (spec §5) --------
#
# What the other three cannot see. `dodge`, `stack` and `jitter` resolve marks
# that share a *position*; a label is as wide as the word it draws, so two labels
# overlap where their points never did.
crowd = {"px": [5.0] * 6, "py": [5.0] * 6,
         "who": ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"]}


def label_at(svg):
    """The mark's own labels, which are the `<text>` that carry an opacity."""
    return re.findall(r'<text x="([0-9.-]+)" y="([0-9.-]+)" fill="[^"]*" fill-opacity=', svg)


plain = label_at(render_svg(data(crowd, name="crowd") + text + x(col.px) + y(col.py) +
                            label(col.who)))
spec = (data(crowd, name="crowd") + text * repel + x(col.px) + y(col.py) + label(col.who))
moved_svg = render_svg(spec)
moved = [(float(a), float(b)) for a, b in label_at(moved_svg)]
assert len(set(plain)) == 1, "six coincident rows should draw six labels in one place"
for i in builtins.range(6):
    for j in builtins.range(i + 1, 6):
        apart = builtins.max(abs(moved[i][0] - moved[j][0]), abs(moved[i][1] - moved[j][1]))
        assert apart > 7, f"repel left labels {i} and {j} on top of each other"
# Nothing is dropped, however impossible the packing (spec §12).
assert len(moved) == 6, "repel must draw every label, never leave one out"
# One specification is one picture: the placement anneals, and an annealing that
# reached for a clock would redraw the book differently on every build.
assert moved_svg == render_svg(spec), "repel must render identically every run"
# A label pushed clear of its dot keeps a line back to it. Six names ring their
# shared point at resting distance, so none travels; it takes a deeper crowd,
# whose outer ranks are held off by the inner ones, to earn the connector.
deep = {"px": [5.0] * 14, "py": [5.0] * 14,
        "who": [f"crew {c}" for c in "ABCDEFGHIJKLMN"]}
deep_svg = render_svg(data(deep, name="deep") + text * repel +
                      x(col.px) + y(col.py) + label(col.who))
assert 'stroke-width="0.7"' in deep_svg, "a travelled label should keep its leader"
# It is `text`-only, and each refusal names the offset that fits.
refuses("point * repel", lambda: render_svg(
    data(crowd, name="crowd") + point * repel + x(col.px) + y(col.py)))
refuses("bar * repel", lambda: render_svg(
    data(crowd, name="crowd") + bar * repel + x(col.who) + y(col.py)))
# `style(nudge=)` is the constant counterpart, and the two compose.
render_svg(data(crowd, name="crowd") + text * repel + x(col.px) + y(col.py) +
           label(col.who) + style(nudge="right"))
ok("`text * repel` separates a label crowd, keeps every label, and composes")


# --- the violin: the slot reading of `density` (spec §5) ---------------------
#
# Not a new mark, and the test says so by drawing it with the two that already
# exist: `ribbon` closes on its own reflection, `area` on the slot's center line.
viol = {
    "grp": ["wide"] * 40 + ["narrow"] * 10,
    # Written out rather than built with `range`, which `from gog import *`
    # deliberately shadows with the transform (see the refusal it raises).
    "v": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] * 5,
}


def npolys(spec) -> int:
    return render_svg(spec).count("<polygon")


assert npolys(data(viol, name="viol") + ribbon * density + x(col.grp) + y(col.v)) == 2, \
    "a violin should draw one shape per category"
assert npolys(data(viol, name="viol") + area * density + x(col.grp) + y(col.v)) == 2, \
    "a half violin should draw one shape per category"
# Lying down, the orientation read off the bindings — the form with room for long
# category names, exactly as `box + x(pay) + y(dept)` is.
assert npolys(data(viol, name="viol") + ribbon * density + x(col.v) + y(col.grp)) == 2, \
    "a sideways violin should draw one shape per category"
ok("`ribbon * density` and `area * density` over a category draw violins")

# `compare` chooses what the widths mean between slots, and must change the plot.
counted = render_svg(data(viol, name="viol") + ribbon * density + x(col.grp) + y(col.v))
shaped = render_svg(data(viol, name="viol") + ribbon * density(compare="shape") +
                    x(col.grp) + y(col.v))
assert counted != shaped, "`density(compare=)` had no effect on the plot"
refuses("compare on a density curve",
        lambda: render_svg(data(viol, name="viol") + line * density(compare="count") + x(col.v)))
refuses("an unknown compare",
        lambda: render_svg(data(viol, name="viol") + ribbon * density(compare="area") +
                           x(col.grp) + y(col.v)))
# The curve is still not a band: a `ribbon` needs two boundaries, and one estimate
# along a continuous axis gives it one.
refuses("a ribbon density curve",
        lambda: render_svg(data(viol, name="viol") + ribbon * density + x(col.v)))
ok("`density(compare=)` reads only in the violin, and by name")


# The ridgeline: the half violin laid down, with overlap and a traced edge.
assert npolys(data(viol, name="viol") + area * density(reach=2.5) + x(col.v) + y(col.grp)) == 2, \
    "an overlapping ridgeline should still draw one shape per category"
traced = render_svg(data(viol, name="viol") + line * density + x(col.v) + y(col.grp))
assert "<polygon" not in traced, "a traced violin should fill nothing"
assert "<path" in traced, "a traced violin should stroke something"
assert render_svg(data(viol, name="viol") + area * density(reach=2.5) + x(col.v) + y(col.grp)) != \
    render_svg(data(viol, name="viol") + area * density + x(col.v) + y(col.grp)), \
    "`density(reach=)` had no effect"
refuses("reach on a density curve",
        lambda: render_svg(data(viol, name="viol") + line * density(reach=2) + x(col.v)))
refuses("a negative reach", lambda: density(reach=-1))
ok("the ridgeline draws, `reach` opens the overlap, and a stroke traces it")


# ---------------------------------------------------------------------------
# Composition — separate plots arranged on one page (spec §11)
#
# `|` and `/` between two *plots* is a page; between a plot and `facet()` it is
# still a split. The engine's one rule does the rest: the same column on the
# same axis in two composed plots is one axis — one scale, one panel extent,
# drawn once. The marginal plot is that rule and nothing else.
# ---------------------------------------------------------------------------

cars = {
    "speed": [4, 4, 7, 7, 8, 9, 10, 10, 10, 11, 11, 12, 12, 12, 12, 13, 13, 13, 13,
              14, 14, 14, 14, 15, 15, 15, 16, 16, 17, 17, 17, 18, 18, 18, 18, 19,
              19, 19, 20, 20, 20, 20, 20, 22, 23, 24, 24, 24, 24, 25],
    "dist": [2, 10, 4, 22, 16, 10, 18, 26, 34, 17, 28, 14, 20, 24, 28, 26, 34, 34,
             46, 26, 36, 60, 80, 20, 26, 54, 32, 40, 32, 40, 50, 42, 56, 76, 84, 36,
             46, 68, 32, 48, 52, 56, 64, 66, 54, 70, 92, 93, 120, 85],
}
scatter = data(cars, name="cars") + point + x(col.speed) + y(col.dist)
top_hist = data(cars, name="cars") + bar * bin + x(col.speed) + theme(height=120)
side_hist = data(cars, name="cars") + bar * bin + y(col.dist) + theme(width=120)

page = top_hist / (scatter | side_hist)
assert isinstance(page, Page), "two plots joined by `/` should be a page"
page_svg = render_svg(page)
assert page_svg.count("<svg") == 4, "a page of three plots is one document holding three"
ok("`top / (main | right)` composes three plots into one page")

# The panels of the two plots sharing `speed` run over the same pixels — the
# whole promise of a marginal plot, and the reason it is not just two plots.
panels = _re.findall(r'<rect x="([0-9.]+)" y="[0-9.]+" width="[0-9.]+"[^>]*fill="#f5f5f8"', page_svg)
assert abs(float(panels[0]) - float(panels[1])) < 0.01, \
    "the marginal histogram's panel should start where the scatter's does"
ok("a shared column gives the two panels one extent")

assert page_svg.count(">Speed<") == 1, \
    "a shared axis should be named once, by the plot nearest its edge"
ok("a shared axis is drawn once, not once per plot")

side_by_side = render_svg(scatter | (data(cars, name="cars") + bar * bin + x(col.dist)))
assert side_by_side.count("<svg") == 3, "two plots side by side are one document holding two"
ok("unrelated plots compose without sharing anything")

alone = render_svg(data(cars, name="cars") + point + x(col.speed) + y(col.dist)
                   + theme(width=400, height=300))
assert 'width="400" height="300"' in alone, "`theme(width=, height=)` should size the image"
ok("`theme(width=, height=)` is the image alone and the cell composed")

# And a *page* states its own size, which is the one sentence no cell can write.
# Composed side by side, two plots divide the page's width and each keep the
# whole of its height, so only the page can say how much height that is.
sized_page = render_svg((scatter | scatter) + theme(height=310))
assert 'width="800" height="310"' in sized_page, "a page is drawn at the size it states"
assert 'width="800" height="600"' in render_svg(scatter | scatter), \
    "a page that states nothing still takes the canvas"
ok("a page states its own size, and takes the canvas when it does not")

refuses("a size no plot can be drawn at", lambda: theme(width=10))
refuses("a page asked to facet", lambda: (scatter | scatter) | facet(col.speed))
refuses("an atom added to a page", lambda: (scatter | scatter) + title("Cars"))
# The size is the only theme property whose subject is the figure. Every other
# one describes a panel, and a page has none.
refuses("a panel property said about a page", lambda: (scatter | scatter) + theme(grid="none"))
refuses("a preset said about a page", lambda: (scatter | scatter) + theme("minimal"))

# --- parentheses group plots, never marks ------------------------------------
# `+` with a Plot on its right keeps the table and returns, which is right for a
# bare `data(df)` and silent loss for `(data(df) + point + area)`: the marks
# inside stopped existing and the plot rendered as though they were never named.
# That is the dropped binding §12 forbids, and a sub-expression that means one
# thing alone and nothing in context breaks Law 6. The composition asserts above
# are the other half — a refusal on `+` sits one method away from breaking them.

note = {"speed": [10.0], "dist": [40.0]}
try:
    (data(cars, name="cars") + point + x(col.speed) + y(col.dist)
     + (data(note, name="note") + point + area))
    raise AssertionError("FAIL: a parenthesized group should be refused")
except GogError as error:
    for want in ("parentheses do not group marks", "repeat", "note", "`|` and `/`"):
        assert want in str(error), f"the refusal should say {want!r}; got: {error}"
ok("a parenthesized group refuses, naming the table and the sequence to write")

refuses("a position inside parentheses",
        lambda: (data(cars, name="cars") + point + x(col.speed) + y(col.dist)
                 + (data(note, name="note") + x(col.speed))))
refuses("a title inside parentheses",
        lambda: (data(cars, name="cars") + point + x(col.speed) + y(col.dist)
                 + (data(note, name="note") + title("hi"))))

# A bare `data()` carries nothing, so it still joins mid-sentence.
seq = (data(cars, name="cars") + point + x(col.speed) + y(col.dist)
       + data(note, name="note") + point)
assert len(seq.spec["layers"]) + (1 if seq.current_layer else 0) == 2, \
    "a bare mid-sentence data() should still bind the next mark"
ok("a bare mid-sentence `data()` still binds the next mark")
refuses("an atom added to a page", lambda: (scatter | scatter) + title("Cars"))
refuses(
    "plots asking for more page than there is",
    lambda: render_svg(
        (data(cars, name="cars") + point + x(col.speed) + y(col.dist) + theme(height=500))
        / (data(cars, name="cars") + point + x(col.speed) + y(col.dist) + theme(height=500))
    ),
)


# --- partition: a hierarchy in columns, one ring per level -------------------
budget = {
    "group": ["A", "A", "A", "B"],
    "item": ["p", "q", "q", "r"],
    "detail": [None, "deep", "also", None],
    "amount": [4.0, 3.0, 3.0, 10.0],
}
sun = render_svg(
    data(budget, name="budget") + zone * partition(col.group, col.item, col.detail)
    + x(col.amount) + color(col.group) + polar()
)
assert "<path" in sun, "a partition in polar draws sectors"
ok("`zone * partition` in polar draws sectors")
icicle = render_svg(
    data(budget, name="budget") + zone * partition(col.group, col.item, col.detail)
    + x(col.amount) + color(col.group)
)
assert "<rect" in icicle and sun != icicle, "flat, the same sentence is the icicle"
ok("the same sentence flat is the icicle")
named = render_svg(
    data(budget, name="budget") + zone * partition(col.group, col.item, col.detail)
    + x(col.amount)
    + text * partition(col.group, col.item, col.detail) + label(col.name) + polar()
)
assert ">deep<" in named, "`text * partition + label(name)` names each node"
ok("a partition feeds a rectangle and a label at once")

refuses(
    "a mark with no region reading",
    lambda: render_svg(data(budget, name="budget")
                       + bar * partition(col.group, col.item) + x(col.amount)),
)
refuses("partition with no levels named", lambda: partition())

# --- flow: a magnitude laid through its stages -------------------------------
# Three marks read one layout: `ribbon` the bands, `zone` the slots, `text` the
# names. The band is the renderer's first cubic curve, and two renders of one
# sentence are one picture.
voyage = {
    "klass": ["First", "First", "Third", "Third"],
    "survived": ["yes", "no", "yes", "no"],
    "n": [203.0, 122.0, 178.0, 528.0],
}


def alluvial():
    return render_svg(
        data(voyage, name="voyage") + y(col.n)
        + ribbon * flow(col.klass, col.survived) + color(col.klass)
        + zone * flow(col.klass, col.survived)
        + text * flow(col.klass, col.survived) + label(col.name)
    )


first = alluvial()
assert " C " in first, "a flow's band is a cubic curve"
assert "<rect" in first, "a flow's slots are rectangles"
assert ">First<" in first, "`label(col.name)` names each slot"

# The refusal, which this suite did not exercise at all until the bed found the
# four bindings disagreeing on it. Two claims: the endpoint clause every binding
# has to share, and the spelling Python actually needs. `col.klass` names a
# column called "klass"; the book's table has one called "class", and
# `col["class"]` is the escape hatch `columns.py` documents for exactly this.
refuses("flow with one stage", lambda: flow(col["class"]))
_flow_msg = ""
try:
    flow(col["class"])
except GogError as _e:
    _flow_msg = str(_e)
assert '`col["class"]` to its `col.survived`' in _flow_msg, (
    f"the refusal must name its example's own endpoints: {_flow_msg}")
assert "col.klass" not in _flow_msg, (
    f"the refusal must not send a reader to a column no table has: {_flow_msg}")
assert first == alluvial(), "one flow sentence is one picture, every run"
ok("`ribbon * flow` bands, `zone * flow` slots, `text * flow` names")

refuses("flow with one stage", lambda: flow(col.klass))
refuses(
    "a mark with no flow reading",
    lambda: render_svg(data(voyage, name="voyage") + y(col.n)
                       + bar * flow(col.klass, col.survived)),
)
refuses(
    "x bound under flow",
    lambda: render_svg(data(voyage, name="voyage") + x(col.n)
                       + ribbon * flow(col.klass, col.survived)),
)
refuses(
    "a band colored by a column that is not a stage",
    lambda: render_svg(data(voyage, name="voyage") + y(col.n)
                       + ribbon * flow(col.klass, col.survived) + color(col.n)),
)

# --- network: a graph placed by its layout ----------------------------------
trade = {
    "exporter": ["Korea", "Korea", "Japan", "China"],
    "importer": ["Japan", "China", "China", "India"],
    "tons": [3.0, 4.0, 3.0, 2.0],
}


def web():
    return render_svg(
        data(trade, name="trade")
        + edge * layout(col.exporter, col.importer) + opacity(col.tons)
        + point * layout(col.exporter, col.importer) + size(col.degree)
        + text * layout(col.exporter, col.importer) * repel + label(col.name)
        + network()
    )


first = web()
assert "<line" in first, "a network draws its edges as strokes"
assert ">Korea<" in first, "`label(col.name)` names each node"
assert "tick" not in first, "the graph-theoretic space draws no ticks"
assert first == web(), "one network sentence is one picture, every run"
cube = render_svg(data(trade, name="trade")
                  + edge * layout(col.exporter, col.importer)
                  + point * layout(col.exporter, col.importer)
                  + network(turn=40, tilt=20))
assert first != cube, "a stated angle changes the picture"
ok("`edge`, `point` and `text` read one layout in `network()`")

refuses(
    "a layout outside the network",
    lambda: render_svg(data(trade, name="trade")
                       + edge * layout(col.exporter, col.importer)),
)
refuses(
    "a network with no layout in it",
    lambda: render_svg(data(trade, name="trade") + point + network()),
)

# --- cluster: the tree of merges, and the seriated tile plot -----------------
pantry = {
    "food": ["rice", "rice", "lentils", "lentils",
             "chicken", "chicken", "oats", "oats"],
    "nutrient": ["protein", "iron"] * 4,
    "amount": [2.7, 0.8, 9.0, 3.3, 25.0, 2.6, 3.4, 1.8],
}


def dendro():
    return render_svg(data(pantry, name="pantry")
                      + path * cluster(col.amount, over=col.nutrient)
                      + x(col.food))


first = dendro()
assert ">Distance<" in first, "the unbound axis names itself Distance"
assert first.count("<polyline") == 3, "three merges are three elbow strokes"
assert first == dendro(), "one cluster sentence is one picture, every run"
sideways = render_svg(data(pantry, name="pantry")
                      + path * cluster(col.amount, over=col.nutrient)
                      + y(col.food))
assert ">Distance<" in sideways, "the sideways tree titles its distance axis"
plain = render_svg(data(pantry, name="pantry") + zone
                   + x(col.food) + y(col.nutrient) + color(col.amount))
sorted_tiles = render_svg(data(pantry, name="pantry")
                          + zone * cluster(over=col.nutrient)
                          + x(col.food) + y(col.nutrient) + color(col.amount))
assert plain != sorted_tiles, "the reorder reading must visibly reorder"
ok("`cluster` draws the tree, lies down, and reorders the tiles")

refuses(
    "a mark with no cluster reading",
    lambda: render_svg(data(pantry, name="pantry")
                       + bar * cluster(col.amount, over=col.nutrient)
                       + x(col.food)),
)
refuses(
    "a cluster with nothing to measure distance on",
    lambda: render_svg(data(pantry, name="pantry")
                       + path * cluster(over=col.nutrient) + x(col.food)),
)
refuses(
    "a bound distance axis",
    lambda: render_svg(data(pantry, name="pantry")
                       + path * cluster(col.amount, over=col.nutrient)
                       + x(col.food) + y(col.amount)),
)
mixed = {"group": ["A", "A"], "item": [None, "p"], "amount": [5.0, 5.0]}
refuses(
    "an interior node with a value of its own",
    lambda: render_svg(data(mixed, name="mixed")
                       + zone * partition(col.group, col.item) + x(col.amount)),
)

# --- partition(cross=True): the mosaic ---------------------------------------
# One parameter apart from the icicle, and it buys the whole plot: the levels
# turn across each other instead of running down one axis. The engine pins the
# arithmetic; here that the sentence draws and that crossing is visible in the
# output rather than silently ignored.
counts = {
    "decade": ["1950s", "1950s", "1960s", "1960s"],
    "theme": ["Heartbreak", "Love", "Heartbreak", "Love"],
    "n": [10.0, 10.0, 30.0, 40.0],
}
mosaic = render_svg(data(counts, name="counts") + x(col.n)
                    + zone * partition(col.decade, col.theme, cross=True)
                    + color(col.theme))
nested = render_svg(data(counts, name="counts") + x(col.n)
                    + zone * partition(col.decade, col.theme) + color(col.theme))
assert "<rect" in mosaic, "a crossed partition draws its cells"
assert mosaic != nested, "`cross=True` must change the picture"
assert "Share of column" in mosaic, "the second axis names what it carries"
ok("`partition(cross=True)` is the mosaic")

labeled = render_svg(data(counts, name="counts") + x(col.n)
                      + zone * partition(col.decade, col.theme, cross=True)
                      + color(col.theme)
                      + text * partition(col.decade, cross=True) + label(col.name))
assert ">1960s<" in labeled, "a shallower crossed partition names the columns"
ok("a shallower crossed partition labels the columns")

refuses("cross given something that is not a bool",
        lambda: partition(col.decade, col.theme, cross="yes"))

# --- a zone takes a border (the closed-glyph fills, spec §4) -----------------
# The settable rule spans a setting across its geometry class, and `zone` joined
# the fills on 2026-07-27 because a mosaic without cell edges is one blob
# wherever two neighbors share a color. Refused until that day, so this is the
# ruling rather than a feature test.
edged = render_svg(data(counts, name="counts") + x(col.n)
                   + zone * partition(col.decade, col.theme, cross=True)
                   + color(col.theme)
                   + style(border_color="white", border_size=2))
assert 'stroke="white"' in edged, "a zone draws the border it was given"
assert 'stroke="white"' not in mosaic, "an unasked-for border must not appear"
ok("a `zone` carries `style(border_color=, border_size=)`")


# ---------------------------------------------------------------------------
# query() — the table that is not in memory
#
# The guard is the one that matters and it is the same in all four bindings: the
# *same sentence*, over a materialized frame and over a query returning the same
# rows, must render byte-identical SVG. If those ever diverge, `query()` has
# stopped being a way of naming rows and become a second way of drawing them.
# ---------------------------------------------------------------------------

import sqlite3

from gog import query
from gog.render import Query

_ROWS = [("open", 120.0), ("shipped", 240.5), ("shipped", 95.25),
         ("closed", 310.75), ("open", 60.0), ("refunded", 45.0)]
_FRAME = {"status": [s for s, _ in _ROWS], "revenue": [r for _, r in _ROWS]}

_con = sqlite3.connect(":memory:")
_con.execute("CREATE TABLE orders (status TEXT, revenue REAL)")
_con.executemany("INSERT INTO orders VALUES (?, ?)", _ROWS)
_SQL = "SELECT status, revenue FROM orders"

for _label, _sentence in (
    ("point with two positions",
     lambda t: t + point + x(col.revenue) + y(col.status)),
    ("bar * count",
     lambda t: t + bar * count + x(col.status)),
    ("bar with a mapped color",
     lambda t: t + bar + x(col.status) + y(col.revenue) + color(col.status)),
):
    _a = render_svg(_sentence(data(_FRAME, name="orders")))
    _b = render_svg(_sentence(query(_con, _SQL, name="orders")))
    assert _a == _b, f"query() and data() disagree on {_label}"
    ok(f"query() draws {_label} byte-identically to data()")

# The query does not run when the sentence is written. An eager query would
# foreclose pushing the transform down, since the planner has to see the whole
# sentence before it can know what to ask the database for.
_lazy = query(_con, "SELECT nonsense FROM nowhere", name="orders")
assert isinstance(_lazy.frames["orders"], Query)
ok("query() holds the SQL rather than running it when the sentence is built")

refuses(
    "query('SELECT ...') with no connection",
    lambda: query(_SQL),
)
refuses(
    "query() given a query that is not text",
    lambda: query(_con, 123),
)
refuses(
    "query() on an object that is neither PEP 249 nor Spark",
    lambda: render_svg(query(object(), _SQL) + bar * count + x(col.status)),
)

# The same guard against other engines. SQLite above is the one that always
# runs, being standard library; these two are skipped when absent rather than
# quietly not run. DuckDB needs no server. Postgres does, so it is reached only
# when `GOG_TEST_POSTGRES` names one — a connection string like
# `postgresql://user:pass@localhost/db`.
#
# What is being checked is not the database. It is that `to_wire` reads whatever
# types that driver returns and still lands every column in the same wire bucket
# SQLite did — which is where a new engine would actually break, since a driver
# is free to hand back Decimal, memoryview, or its own date class.

def _guard_over(label: str, connect, ddl: str) -> None:
    """Assert query() and data() agree byte for byte on one more engine."""
    try:
        con = connect()
    except Exception as exc:                       # driver missing, or no server
        print(f"SKIP: {label} — {type(exc).__name__}: {str(exc)[:60]}")
        return
    try:
        cur = con.cursor()
        cur.execute("DROP TABLE IF EXISTS gog_orders")
        cur.execute(ddl)
        for status, revenue in _ROWS:
            cur.execute(f"INSERT INTO gog_orders VALUES ('{status}', {revenue})")
        if hasattr(con, "commit"):
            con.commit()
        sql = "SELECT status, revenue FROM gog_orders"
        sentence = lambda t: t + bar * count + x(col.status)
        a = render_svg(sentence(data(_FRAME, name="orders")))
        b = render_svg(sentence(query(con, sql, name="orders")))
        assert a == b, f"query() and data() disagree over {label}"
        ok(f"query() over {label} draws byte-identically to data()")
    finally:
        con.close()


def _duckdb():
    import duckdb
    return duckdb.connect()


def _postgres():
    import os
    url = os.environ.get("GOG_TEST_POSTGRES")
    if not url:
        raise RuntimeError("set GOG_TEST_POSTGRES to a connection string to run this")
    try:
        import psycopg
        return psycopg.connect(url)
    except ImportError:
        import psycopg2
        return psycopg2.connect(url)


_guard_over("DuckDB", _duckdb,
            "CREATE TABLE gog_orders (status VARCHAR, revenue DOUBLE)")
_guard_over("PostgreSQL", _postgres,
            "CREATE TABLE gog_orders (status TEXT, revenue DOUBLE PRECISION)")

# --- gog_table(): the manual's tables, without a CSV reader to copy ----------
# Binding plumbing rather than a word of the grammar, which is why
# `book/check_vocabulary.R` excludes it from the kernel block beside
# `render_svg`. The offline checks always run; the fetch is guarded, because a
# suite has to pass on a laptop with no network.
from gog.tables import (  # noqa: E402
    BOOK_DATA_CHAPTER, BOOK_DATA_URL, _columns, _nearest_table, _unknown_table,
)

assert BOOK_DATA_URL == "https://psychometrician.github.io/gog-book/data/"

_rows = [{"n": "1", "s": "01", "w": "x"}, {"n": "2", "s": "02", "w": "y"}]
_typed = _columns(_rows, text=("s",))
assert _typed["n"] == [1.0, 2.0], _typed["n"]
assert _typed["s"] == ["01", "02"], _typed["s"]
assert _typed["w"] == ["x", "y"], _typed["w"]
ok("gog_table() types a column only when every value in it is a number")

# `GogError`, not `TypeError`: every refusal in this package is one class, so
# `except GogError` around a session catches the lot — `errors.py` records why.
refuses("`gog_table()` with something that is not a name", lambda: gog_table(42))

# The near-miss rule, tested where it is deterministic. The list is written here
# rather than fetched, so this says what the rule does and not what the site
# currently holds — and it runs on a laptop with no network.
#
# The rule is the engine's `nearest_color`: within two edits, and fewer edits
# than the candidate has letters. The last two probes are the ones that matter,
# because a suggestion rule is judged by what it declines to say. `penguins` is
# nothing like any of these, and `gm` is short enough that a loose rule would
# match half the list.
_known = ["gapminder_2007", "gapminder_asia", "gm_all", "winds", "medals"]
assert _nearest_table("gapminder2007", _known) == "gapminder_2007"
assert _nearest_table("Gapminder_2007", _known) == "gapminder_2007"
assert _nearest_table("gapmidner_2007", _known) == "gapminder_2007"
assert _nearest_table("wind", _known) == "winds"
assert _nearest_table("penguins", _known) is None
assert _nearest_table("gm", _known) is None
# No list to read from is the offline case, and it must not be an error.
assert _nearest_table("gapminder2007", []) is None
ok("nearest_table() suggests a near miss and declines a far one")

# The two sentences, in full. All four bindings print these words exactly, so a
# change here is a change every reader of the manual sees four times.
assert _unknown_table("gapminder2007", _known) == (
    'gog: there is no table called "gapminder2007". '
    'Did you mean "gapminder_2007"?'
), _unknown_table("gapminder2007", _known)
assert _unknown_table("penguins", _known) == (
    'gog: there is no table called "penguins". The table names are listed in '
    f"the book's data chapter: {BOOK_DATA_CHAPTER}"
), _unknown_table("penguins", _known)
ok("gog_table() names the table it could not find")

# The old name is gone rather than deprecated, and this is the assertion that
# keeps it gone. `from gog import *` is how the book writes every example, so a
# name left behind in `__all__` or in the module would be back in the
# vocabulary quietly, as two spellings of one function.
import gog as _gog  # noqa: E402

assert "book_table" not in _gog.__all__, _gog.__all__
assert not hasattr(_gog, "book_table"), "book_table survived on the package"
assert not hasattr(_gog.tables, "book_table"), "book_table survived in tables.py"
ok("book_table() is gone, not deprecated")

# `map(preserve=)` was the one refusal in the package raised as a `ValueError`,
# which `except GogError` missed; now it is the same class as the other hundred.
refuses("`map(preserve=)` beyond its two words", lambda: map(preserve="upside"))

# The viewing angles are numbers and a label is one string — checked at the
# line the caller wrote, in every binding. `float("left")` used to raise a
# bare `ValueError` here.
refuses("a string viewing angle", lambda: space(turn="left"))
refuses("a string polar start", lambda: polar(start="top"))
refuses("`x_label()` with a number", lambda: x_label(42))
refuses("`title()` with a number", lambda: title(42))

# An elevation has ends and a bearing does not, and the pair is the test. The drag
# has clamped tilt to plus or minus 90 since 2026-08-06, so a reader turning the
# cube could not reach a turned-over view while a reader *writing* one could:
# `space(tilt=-400)` drew upside-down nonsense without a word. `turn` must stay
# silent, because a bearing genuinely wraps — refusing both alike would teach a cap
# the grammar does not have.
_cube = {"x": [1.0, 2.0, 3.0], "y": [2.0, 1.0, 3.0], "z": [3.0, 2.0, 1.0]}


def _turned(**kw):
    return (data(_cube, name="cube") + point + x(col.x) + y(col.y) + z(col.z)
            + space(**kw))


for _t in (95, 180, -400):
    try:
        render_svg(_turned(tilt=_t))
        raise AssertionError(f"FAIL: space(tilt={_t}) should be refused")
    except GogError as _e:
        assert "-90 to 90" in str(_e), str(_e)
        # The refusal must offer the bearing, or it reads as a cap on both angles.
        assert "space(turn = )" in str(_e), str(_e)
for _t in (90, -90, 0, 25):
    assert "<svg" in render_svg(_turned(tilt=_t)), f"tilt {_t} is in range"
# Equal bearings draw the same bytes, not merely a similar picture. `turn=-360`
# used to lose two of eighteen tick labels: every mark in place, nothing on stderr.
_canonical = render_svg(_turned(turn=30))
for _t in (390, 750, -330, -690):
    assert render_svg(_turned(turn=_t)) == _canonical, f"turn {_t} is turn 30"
ok("tilt has ends, turn wraps, and equal bearings draw alike")

# A fit needs rows, and it needs them in the cell the fit runs in. The engine had
# the minimum already — below three rows the transform returns the frame unchanged
# — but no gate, so the raw rows reached the page *as* the fitted curve. The split
# is the half that hid: six rows pass any whole-frame count while all three of
# their groups fail.
for _n, _rows in ((1, [1.0]), (2, [1.0, 2.0])):
    # Written out rather than built with `range`, which is gog's transform here —
    # the shadowing this package warns about on import, and its refusal names the
    # fix (`builtins.range`) rather than letting a list comprehension fail oddly.
    _thin = {"x": list(_rows), "y": list(_rows)}
    try:
        render_svg(data(_thin, name="thin") + line * smooth + x(col.x) + y(col.y))
        raise AssertionError(f"FAIL: smooth on {_n} row(s) should be refused")
    except GogError as _e:
        assert "at least 3" in str(_e), str(_e)
        assert f"has {_n}." in str(_e), str(_e)
        assert "Drop the split" not in str(_e), str(_e)
_three = {"x": [1.0, 2.0, 3.0], "y": [1.0, 2.0, 3.0]}
assert "<svg" in render_svg(data(_three, name="three") + line * smooth + x(col.x) + y(col.y))

_split = {"g": ["a", "a", "b", "b", "c", "c"],
          "x": [1.0, 2.0, 1.0, 2.0, 1.0, 2.0], "y": [1.0, 2.0, 2.0, 1.0, 1.0, 3.0]}
try:
    render_svg(data(_split, name="split") + line * smooth + x(col.x) + y(col.y)
               + group(col.g))
    raise AssertionError("FAIL: groups of two should be refused")
except GogError as _e:
    assert "at least 3" in str(_e) and "`g`" in str(_e), str(_e)
    assert "Drop the split" in str(_e), str(_e)
ok("smooth needs three rows in every cell it fits")

try:
    _gm = gog_table("gapminder_2007")
except Exception as _error:
    print(f"SKIP: gog_table() live fetch - {type(_error).__name__}: {str(_error)[:50]}")
else:
    assert len(_gm["country"]) == 142, len(_gm["country"])
    assert isinstance(_gm["gdp"][0], float), _gm["gdp"][0]
    assert isinstance(_gm["continent"][0], str), _gm["continent"][0]
    ok("gog_table('gapminder_2007') is 142 typed rows")

    # A name the site does not have. Guarded with the fetch above, because it
    # takes the same network — and it is the assertion the whole refusal exists
    # for: before it, Python raised `HTTPError`, which names neither the table
    # nor the fix and which `except GogError` does not catch. Only reached when
    # the network is up, so the rule itself is checked offline above.
    try:
        gog_table("gapminder2007")
    except GogError as _refusal:
        assert "gapminder2007" in str(_refusal), _refusal
        ok(f"gog_table() refused an unknown name — {_refusal}")
    else:
        raise AssertionError("FAIL: gog_table('gapminder2007') returned a table")


# ---------------------------------------------------------------------------
# brush — the selection
#
# Four claims, and the second is the one the whole feature rests on: a plot that
# names no brush must be exactly the plot it was before selection existed.
# ---------------------------------------------------------------------------

_brush_df = {"v": [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
             "w": [2.0, 4.0, 1.0, 5.0, 3.0, 6.0],
             "kind": ["a", "a", "b", "b", "c", "c"]}
_DIM = '<g opacity="0.150">'

_svg = render_svg(data(_brush_df, name="bt") + point + x(col.v) + y(col.w)
                  + brush(col.v, at=(2.5, 4.5)))
assert _DIM in _svg, "brush() drew no dimmed group"
# A brush highlights; it never removes rows. That is what separates it from
# `limits`, and it is the claim a reader is most likely to test.
assert _svg.count("<circle") == 6, "brush() dropped rows — it must dim, not filter"
ok("brush() dims the rows outside the bound and drops none")

_plain = render_svg(data(_brush_df, name="bt") + point + x(col.v) + y(col.w))
assert "data-gog-panel" not in _plain and "<g opacity=" not in _plain
ok("a plot with no brush is untouched by selection")

_cat = render_svg(data(_brush_df, name="bt") + point + x(col.v) + y(col.w)
                  + brush(col.kind, at="b"))
assert _DIM in _cat, "brush() on a column of categories selected no slots"
ok("brush() on a category column selects slots")

try:
    render_svg(data(_brush_df, name="bt") + line + x(col.v) + y(col.w)
               + brush(col.v, at=(2.0, 4.0)))
    raise AssertionError("a brushed line should refuse")
except Exception as _e:
    _t = str(_e)
    # Every mark that draws one row per shape is named, and `group()` is not: a
    # grouped line is refused the same way, so the advice could not be followed.
    assert "one shape through many rows" in _t and "`point`, `text`, `rule` and `zone`" in _t \
        and "group()" not in _t, _t
try:
    render_svg(data(_brush_df, name="bt") + area + x(col.v) + y(col.w)
               + brush(col.v, at=(2.0, 4.0)))
    raise AssertionError("a brushed area should refuse")
except GogError as _e:
    assert str(_e).startswith("gog: an `area` draws"), str(_e)
ok("refused — a line has no single row to select")

refuses("`at` is two numbers or a set of names",
        lambda: brush(col.v, at=(1, 2, 3)))



_con.close()

# --- a composed page of cubes carries the engine ------------------------------
#
# A `Page` writes its list as `cells`, and this check read only `plots`, so it
# answered False for every composition of 3-D plots and shipped no engine. The
# page drew perfectly and would not turn, which is the failure that hides: a
# picture with a gesture missing still looks like a picture. Julia had the same
# gap; R read both spellings and JavaScript reads both, which is what made two
# bindings look right while two were not.
from gog import render as _R
_c = {"a": [1.0, 2.0], "b": [1.0, 2.0], "c": [1.0, 2.0]}
_cube = lambda n: data(_c, n) + point + x(col.a) + y(col.b) + z(col.c) + space()
_page = _cube("t") | _cube("u")
assert _R._needs_engine({"arrange": _page.arrange, "cells": _page.cells}), \
    "a page holding a cube has an angle to drag"
assert _R._needs_engine(_page.cells[0]), "and so does the cube on its own"
assert not _R._needs_engine({"arrange": "beside", "cells": [{"layers": []}]}), \
    "a page of flat plots still pays nothing"
ok("a composed page of cubes carries the engine")

# The globe carries the engine for the cube's own reason: an angle worth
# dragging. Its gate missed this on the day the space shipped.
_globe_page = (data(_c, "t") + point + x(col.a) + y(col.b) + globe()) | (
    data(_c, "u") + point + x(col.a) + y(col.b) + globe()
)
assert _R._needs_engine(_globe_page.cells[0]), \
    "a globe has an angle to drag, exactly as the cube does"
ok("a globe carries the engine")

# --- the interactive block must reach the browser intact ---------------------
#
# Two defects lived here and neither was reachable by comparing SVG, because both
# bypass `gog-cli` entirely: the static picture is the CLI's and is perfect,
# while the *browser* gets a separate payload that nothing checked.
#
# (1) The data was never converted. `_wire()` returns raw frames; `render_svg`
#     passes them through `to_wire` and this block did not, so the engine read
#     each column *name* where a type group belongs and refused every column.
#     3-D, `brush` and `play` were broken for every Python user.
# (2) The module was imported from a `data:` URL, which a content-security policy
#     refuses — silently, since a blocked module import throws nothing.
from gog import render as _R
_t = {"gdp": [1000.0, 20000.0, 40000.0], "life": [50.0, 70.0, 80.0]}
_p = (data(_t, "t") + point + x(col.gdp) + y(col.life)
      + brush(col.gdp, at=[2000, 30000]))
_block = _R.svg_block(render_svg(_p), _p)

# No script means the browser engine was never built, which is the normal state
# in CI. There is nothing to assert about a block that does not exist.
if "<script" not in _block:
    print("SKIP: browser engine not built, so the interactive block cannot be checked")
else:
    assert "data:text/javascript" not in _block
    assert "data:application/wasm" not in _block
    assert 'from "./view.js"' not in _block
    assert "function mountView" in _block
    assert "atob(" in _block
    ok("the interactive block names no URL a policy can refuse")

    _sent = json.loads(re.search(r'mount\("[^"]+", (\{.*?\}), \{ wasm:',
                                 _block, re.S).group(1))["data"]
    _spec, _frames = _p._wire()
    assert _sent == {n: _R.to_wire(f, n) for n, f in _frames.items()}
    ok("the browser gets the same wire tables the engine does")

# --- the engine beside the package is the package's own ----------------------
# Eight files agreeing on a version number says nothing about the binary that
# draws. They are separate artifacts and they went out of step exactly once it
# mattered: a source tarball carried an engine a whole release behind its own
# manifest, and nothing in this repository could see it. Not the version guard,
# which reads files; not the parity harness, which drew all 740 sentences of the
# manual through both engines and found them identical, because two builds a
# patch apart agree on every sentence that did not change between them.
#
# Bytes cannot answer it either. An engine compiled inside an installed package
# hashes differently from the same sources built in a checkout, because the
# build path travels in the binary. Asking is the only question with an answer.
#
# `stdin` must be closed. An engine older than the flag does not reject
# `--version`; it ignores the argument and blocks reading stdin forever, since
# stdin is how a plot arrives. The obvious spelling of this check hangs on
# exactly the engine it exists to catch.
from gog import __version__ as _declared

_engine = _R.find_gog_cli()
_reported = subprocess.run(
    [_engine, "--version"], capture_output=True, text=True,
    stdin=subprocess.DEVNULL, timeout=30,
).stdout.strip()
assert re.match(r"^\d+\.\d+\.\d+", _reported), (
    f"the engine at {_engine} cannot say which version it is; it answered "
    f"{_reported!r}. An engine without `--version` predates this check, so it "
    f"is older than the package beside it. Rebuild: cargo build --release -p gog-cli"
)
assert _reported == _declared, (
    f"the package says {_declared} and its engine says {_reported}. "
    f"Engine: {_engine}. A plot drawn now is drawn by the wrong release."
)
ok(f"the engine reports {_reported}, the same as the package")

# --- a page of tables the binding had to name itself --------------------------
# Neither plot can read a name off the caller, so the binding invents `data` for
# both. That name is its own and means nothing to the author, so the second one
# gives way rather than colliding — the same rule a plot of two tables already
# follows. A name the author *wrote* still cannot be moved.
_left = {"x": [1.0, 2.0], "y": [3.0, 4.0]}
_right = {"x": [3.0, 4.0], "y": [5.0, 6.0]}
with warnings.catch_warnings():
    warnings.simplefilter("ignore")
    _bare = ((data(dict(_left)) + point + x(col.x) + y(col.y))
             | (data(dict(_right)) + point + x(col.x) + y(col.y)))
_named = ((data(_left, name="one") + point + x(col.x) + y(col.y))
          | (data(_right, name="two") + point + x(col.x) + y(col.y)))
assert len(_bare.frames) == 2, _bare.frames
# The picture is the test: a rename that pointed both cells at one table would
# draw too, and only this catches that.
assert render_svg(_bare) == render_svg(_named)
ok("a page of two anonymous tables draws what naming them would draw")

refuses("two different tables on one page under a name the author wrote",
        lambda: (data(_left, name="s") + point + x(col.x) + y(col.y))
        | (data(_right, name="s") + point + x(col.x) + y(col.y)))

# --- a refusal must cost nothing that was already on disk ---------------------
# Julia's `save()` opened the destination before it knew the render had
# succeeded, and opening for writing truncates, so a refused plot emptied
# whatever was there. Python's `plot.save()` renders before it opens and so
# cannot, and this holds it to that: the ordering is easy to reverse while
# tidying, and nothing else would notice.
_savedir = tempfile.mkdtemp()
_savepath = os.path.join(_savedir, "plot.svg")
_good = data(_left, name="one") + point + x(col.x) + y(col.y)
_bad = data(_left, name="one") + point + x(col.x) + y(col.y) + palette("okabe")
_good.save(_savepath)
with open(_savepath, encoding="utf-8") as _h:
    _before = _h.read()
assert _before, "save() wrote nothing"
try:
    _bad.save(_savepath)
    raise AssertionError("FAIL: a plot that maps no color should have been refused")
except GogError:
    pass
with open(_savepath, encoding="utf-8") as _h:
    assert _h.read() == _before, "a refused save() destroyed the file already there"
ok("a refused save() leaves an existing file alone")

# --- a refusal in a notebook cell reads as the message, not as a crash --------
# Raised into a display host, a refusal arrives as frames through this package
# and IPython's internals, none of which is anywhere the author can act. The
# display hook shows the message instead; `render_svg` still raises, so every
# check that reads an exit code is unaffected.
_frame = {"gdp": [1.0, 2.0], "life": [3.0, 4.0]}
_refused = data(_frame, name="f") + point + x(col.gdp) + y(col.life) + palette("okabe")
_drawn = data(_frame, name="f") + point + x(col.gdp) + y(col.life)

_shown = _refused._repr_html_()
assert "palette()" in _shown, "the refusal message did not reach the cell"
assert "<div" not in _shown, "a refusal displayed as a plot"
# The message contains `color(<column>)`, so an unescaped `<` would be eaten as
# a tag and the reader would lose the half of the sentence naming the fix.
assert "&lt;column&gt;" in _shown, "the message reached the cell unescaped"
ok("a refused plot shows its message in a cell")

assert _drawn._repr_html_().lstrip().startswith("<div"), "a good plot stopped drawing"
try:
    render_svg(_refused)
    raise AssertionError("FAIL: render_svg() stopped raising on a refusal")
except GogError:
    pass
ok("drawing still draws and render_svg() still raises")

# ---------------------------------------------------------------------------
# range() — the band's two ends, as quantile probabilities
# ---------------------------------------------------------------------------

_band_table = {"g": ["a"] * 10, "v": [float(i) for i in builtins.range(1, 11)]}
_band = render_svg(
    data(_band_table, name="b") + interval * range(0.25, 0.75) + x(col.g) + y(col.v)
)
_whole = render_svg(data(_band_table, name="b") + interval * range + x(col.g) + y(col.v))
assert _band != _whole, "range(0.25, 0.75) drew what bare `range` draws"
# 1..10 by type 7: Q1 = 3.25 and Q3 = 7.75, the numbers `quantile()` returns.
assert ">4</text>" in _band and ">10</text>" not in _band, (
    "the interquartile band should span 3.25..7.75, not the extremes"
)
assert ">10</text>" in _whole, "bare `range` should still reach the maximum"
ok("range() takes a quantile band, bare stays the extremes")

refuses("a band end above 1", lambda: range(0.5, 1.5))
refuses("a band end below 0", lambda: range(-0.1))
refuses("a band end that is not a number", lambda: range("a"))
refuses(
    "a band that runs downward",
    lambda: render_svg(
        data(_band_table, name="b") + interval * range(0.75, 0.25) + x(col.g) + y(col.v)
    ),
)

# ---------------------------------------------------------------------------
# deviation and quantile — the family's two newest members
# ---------------------------------------------------------------------------

_spread = {"g": ["a"] * 8, "v": [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]}
_one_sd = render_svg(data(_spread, name="s") + interval * deviation + x(col.g) + y(col.v))
_two_sd = render_svg(data(_spread, name="s") + interval * deviation(2) + x(col.g) + y(col.v))
assert _one_sd != _two_sd, "deviation(2) drew what bare `deviation` draws"
assert _one_sd != render_svg(
    data(_spread, name="s") + interval * confidence + x(col.g) + y(col.v)
), "a spread band drew the mean's confidence interval"
refuses("a deviation of zero", lambda: deviation(0))
ok("deviation() bands the spread, and is not confidence()")

# The whisker rule is one of two words, and the refusal naming them went
# untested in all four suites until 0.0.4 -- which is how this message shipped
# for several releases with R writing `--` where the other three wrote a dash.
# The bed compares the refusals it has a sentence for, and nobody had written
# this one, so a message nothing triggers was a message nothing checked.
refuses("a whisker rule that is neither word", lambda: box("middle"))
try:
    box("middle")
except Exception as _e:
    assert "is either" in str(_e), str(_e)
ok("box() names the two whisker rules it takes")

# `GOG_STRICT=0` does not reach a refusal raised while the atom is built, and the
# manual says so, so the claim is pinned here rather than left to reasoning. The
# switch trades a refusal for a picture; there is no picture on offer when the
# atom was never built, and downgrading could only invent a value gog was not
# given. What would break this is moving such a check into the engine, where the
# switch does reach it — which is exactly the refactor the ruling declines.
_before_strict = os.environ.get("GOG_STRICT")
os.environ["GOG_STRICT"] = "0"
try:
    for _probe, _what in ((lambda: box("middle"), 'box("middle")'),
                          (lambda: deviation(-1), "deviation(-1)"),
                          (lambda: color("red"), 'color("red")')):
        try:
            _probe()
        except GogError as _refusal:
            assert "gog:" in str(_refusal), str(_refusal)
        else:
            raise AssertionError(f"FAIL: GOG_STRICT=0 reached {_what}")
finally:
    if _before_strict is None:
        del os.environ["GOG_STRICT"]
    else:
        os.environ["GOG_STRICT"] = _before_strict
ok("GOG_STRICT=0 does not reach a refusal raised as the atom is built")

_q90 = render_svg(data(_spread, name="s") + bar * quantile(0.9) + x(col.g) + y(col.v))
assert _q90 != render_svg(
    data(_spread, name="s") + bar * median + x(col.g) + y(col.v)
), "quantile(0.9) drew the median"
refuses(
    "a bare quantile",
    lambda: render_svg(data(_spread, name="s") + bar * quantile + x(col.g) + y(col.v)),
)
refuses("a quantile above 1", lambda: quantile(1.5))
refuses("a quantile below 0", lambda: quantile(-0.1))
ok("quantile() needs its probability")

print(f"\nAll {passed} checks passed.")

# ---------------------------------------------------------------------------
# The globe: the sphere itself, viewed. The same marks as a map stand on the
# facing hemisphere; the far half is hidden behind the sphere.
# ---------------------------------------------------------------------------
_places = {"lon": [178.44, 139.69, -0.13], "lat": [-18.14, 35.69, 51.51]}
_gsvg = render_svg(
    data(_places) + point + x(col.lon) + y(col.lat) + globe(turn=178, tilt=-18)
)
assert "<circle" in _gsvg, "the globe drew no disk"
assert "<polyline" in _gsvg, "the globe drew no graticule"
assert "<text" not in _gsvg, "a globe grew an axis label"
ok("the globe draws its disk and graticule, and no axes")
refuses(
    "a bar on the globe with no measure",
    lambda: render_svg(data(_places) + bar + x(col.lon) + y(col.lat) + globe()),
)
refuses(
    "a tilt past the pole",
    lambda: render_svg(
        data(_places) + point + x(col.lon) + y(col.lat) + globe(tilt=100)
    ),
)
# With its measure named, the bar is the spike.
_spiky = dict(_places, v=[3.0, 9.0, 5.0])
_spike_svg = render_svg(
    data(_spiky) + bar + x(col.lon) + y(col.lat) + z(col.v)
    + globe(turn=178, tilt=-18)
)
assert "<line " in _spike_svg, "a spike drew no stroke"
ok("a bar with z is a spike on the globe")


# --- the grown *which one?* vocabularies -------------------------------------
# Python keeps its own copy of `pattern`'s values, so a vocabulary grown in the
# engine and not here fails silently. Each value is drawn rather than asserted
# against a second list.
_vocab = dict(a=[1.0, 2.0, 3.0], b=[4.0, 5.0, 6.0])
for _s in ("circle", "square", "triangle", "diamond", "cross", "star", "wye"):
    render_svg(data(_vocab) + point + x(col.a) + y(col.b) + style(shape=_s))
ok("Python draws all seven glyphs")
for _d in ("solid", "dashed", "dotted", "dotdash", "longdash"):
    render_svg(data(_vocab) + line + x(col.a) + y(col.b) + style(pattern=_d))
ok("Python draws all five stroke dashes")
for _f in ("solid", "hatch", "crosshatch", "stripes", "grid", "dots"):
    render_svg(data(_vocab) + bar + x(col.a) + y(col.b) + style(pattern=_f))
ok("Python draws all six fill textures")


# --- `theme(axis_label=)`: one convention for both axis names ----------------
_al = dict(a=[1.0, 2.0, 3.0], b=[4.0, 5.0, 6.0])
_base = data(_al) + point + x(col.a) + y(col.b) + x_label("A") + y_label("B")
assert "rotate(-90" in render_svg(_base), "the default should turn the y name"
assert "rotate(-90" not in render_svg(_base + theme(axis_label="end")), \
    "`end` should leave every name horizontal"
ok("Python places both axis names by one rule")
refuses("an axis_label placement that does not exist",
        lambda: theme(axis_label="sideways"))


# --- two plots in one document may not share an id ---------------------------
import re as _re
_p1 = render_svg(data(dict(g=["a", "b", "c"], v=[1.0, 2.0, 3.0]))
                 + bar + x(col.g) + y(col.v) + style(pattern="hatch"))
_p2 = render_svg(data(dict(g=["a", "b", "c"], v=[3.0, 1.0, 2.0]))
                 + bar + x(col.g) + y(col.v) + style(pattern="hatch"))
_ids = lambda s: set(_re.findall(r'id="([^"]+)"', s))
assert _ids(_p1) and _ids(_p2), "both plots must mint ids to compare"
assert not (_ids(_p1) & _ids(_p2)), \
    f"two different plots share an id: {_ids(_p1) & _ids(_p2)}"
for _s in (_p1, _p2):
    assert set(_re.findall(r"url\(#([^)]+)\)", _s)) <= _ids(_s), \
        "a reference points outside its own plot"
ok("two plots in one document mint no id in common")


# --- accepted and dropped: each drew something other than it said -----------
# The same block runs in all four bindings, on the same table.
_strip = dict(g=["A"] * 4 + ["B"] * 4, k=["p", "q"] * 4,
              n=[3.0, 1.0, 4.0, 2.0, 3.0, 1.0, 4.0, 2.0],
              v=[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0])


def _refusal(thunk) -> str:
    try:
        thunk()
    except GogError as error:
        return str(error)
    raise AssertionError("FAIL: the sentence was accepted, and should have been refused")


for _what, _thunk, _fragment in [
    ("jitter past the edge of its slot",
     lambda: render_svg(data(_strip) + point * jitter(2) + x(col.g) + y(col.v)),
     "`jitter(1.25)`"),
    ("order() by a column of text",
     lambda: render_svg(data(_strip) + bar * sum + x(col.g) + y(col.v) + order(col.k)),
     "holds text"),
    ("a rule naming both positions",
     lambda: render_svg(data(_strip) + point + x(col.n) + y(col.v)
                        + rule + x(col.n) + y(col.v)),
     "`rule + x(n) + rule + y(v)`"),
    ("opacity on a point with no fill",
     lambda: render_svg(data(_strip) + point + x(col.n) + y(col.v)
                        + style(color="none", border_color="steelblue", opacity=0.3)),
     "#4682b44d"),
    ("zone * bin over two categories",
     lambda: render_svg(data(_strip) + zone * bin + x(col.g) + y(col.k)),
     "are both categorical"),
]:
    _text = _refusal(_thunk)
    assert _fragment in _text, f"{_what}: wanted {_fragment!r} in {_text}"
    ok(f"{_what} refused")
render_svg(data(_strip) + point * jitter(1.25) + x(col.g) + y(col.v))
for _thunk in (lambda: render_svg(data(_strip) + path * mean + x(col.g) + y(col.v)),
               lambda: render_svg(data(_strip) + zone * bin + x(col.g) + y(col.k))):
    assert _refusal(_thunk).count("gog: `") == 1, "a mistake should be refused once"
ok("the refusals of 2026-09-23 refuse once, with direction")

_summed = render_svg(data(_strip) + zone * sum + x(col.g) + y(col.k) + color(col.v))
_shared = render_svg(data(_strip) + zone * sum * proportion + x(col.g) + y(col.k)
                     + color(col.v))
assert _summed != _shared, "`proportion` was dropped on a zone"
_lines = render_svg(data(_strip) + line * mean + x(col.n) + y(col.v)
                    + color(col.g) + group(col.k))
assert _lines.count("<polyline") == 4, "color and group should split a mean into four lines"
assert render_svg(data(_strip) + interval * range + x(col.g) + y(col.v) + pattern(col.k)) \
    == render_svg(data(_strip) + interval * range + x(col.g) + y(col.v) + pattern(col.k)
                  + group(col.k)), "`pattern` alone should split a statistic"
ok("a statistic splits by every channel, and a zone takes a share")

_numbers = dict(year=ordered([2019, 2020, 2021], [2019, 2020, 2021]), sales=[3.0, 5.0, 4.0])
_text_years = dict(year=ordered(["2019", "2020", "2021"], ["2019", "2020", "2021"]),
                   sales=[3.0, 5.0, 4.0])
assert render_svg(data(_numbers, name="as_numbers") + bar + x(col.sales) + y(col.year)) \
    == render_svg(data(_text_years, name="as_numbers") + bar + x(col.sales) + y(col.year)), \
    "a declared order over numbers should draw as categories"
ok("a declared order over numbers draws as categories")


# --- a tally's axis names no column; a box's refusal is said once -------------
_text = _refusal(lambda: render_svg(data(_strip) + bar * count + x(col.g) + y(col.v)))
assert "names a column it never reads" in _text, _text
_text = _refusal(lambda: render_svg(data(_strip) + point * bin * stack + x(col.n) + y(col.g)))
assert "`color(g)`" in _text, _text
render_svg(data(_strip) + bar * count + x(col.g) + y(col.count))
assert _refusal(lambda: render_svg(data(_strip) + box * mean + x(col.g) + y(col.v))) \
    .count("gog: `") == 1, "`box * mean` should be refused once"
ok("a tally's axis names no column, and a box's refusal is said once")


# --- each of these drew something other than the sentence said ------------------
# A count a log, calendar or map axis never read, a date on `z` in epoch seconds,
# a page coarsening its plots' ticks, two keys for one column, a transparent
# figure painted white, a name far from its axis, a dash an `edge` refused, a
# label the layout had nowhere to read from.
from datetime import timedelta as _timedelta  # noqa: E402


def _labels(svg: str) -> list:
    return re.findall(r">([^<>]*)</text>", svg)


_big = {"i": list(builtins.range(25)), "v": [10.0 ** k for k in builtins.range(25)]}
_lab = _labels(render_svg(data(_big) + point + x(col.i) + y(col.v, scale="log")))
assert "10¹²" in _lab and not any("000B" in l for l in _lab), _lab

_wide = {"gdp": [277.55, 2000.0, 12000.0, 49357.19], "life": [40.0, 55.0, 70.0, 82.0]}


def _logged(**kw):
    return _labels(render_svg(data(_wide, name="wide") + point
                              + x(col.gdp, scale="log", **kw) + y(col.life)))


assert "2K" not in _logged() and {"2K", "5K"} <= set(_logged(tick_count=12)), _logged(tick_count=12)
_days = {"day": [date(2024, 3, 1) + _timedelta(days=i) for i in builtins.range(42)],
         "orders": [20.0, 22.0, 24.0, 26.0, 28.0, 30.0, 32.0] * 6}


def _dated(width=800, **kw):
    return builtins.sum(bool(re.match(r"(Feb|Mar|Apr) ", l)) for l in _labels(
        render_svg(data(_days, name="days") + line + x(col.day, **kw) + y(col.orders)
                   + theme(width=width))))


# Twenty asks the calendar for 22 dates: at 800 pixels they do not fit side by
# side, so one in every two is drawn; at 1600 all 22 are.
assert (_dated(), _dated(tick_count=3), _dated(tick_count=20),
        _dated(tick_count=20, width=1600)) == (6, 3, 11, 22), \
    (_dated(), _dated(tick_count=3), _dated(tick_count=20))
assert "Mar 4" in _labels(render_svg(data(_days, name="days") + point + x(col.orders)
                                     + y(col.orders) + z(col.day)))

_quakes = {"east": [165.0, 170.0, 175.0, 180.0, 185.0], "north": [-35.0, -30.0, -25.0, -20.0, -15.0]}


def _degrees(**kw):
    return builtins.sum(l.endswith("°") for l in _labels(render_svg(
        data(_quakes, name="quakes") + point + x(col.east, **kw) + y(col.north) + map())))


assert _degrees(tick_count=20) > _degrees() + 5, (_degrees(tick_count=20), _degrees())

_page = ((data(_wide, name="wide") + point + x(col.gdp, tick_count=3) + y(col.life))
         | (data(_wide, name="wide") + point + x(col.gdp, tick_count=12) + y(col.life)))
assert {"5K", "15K", "45K"} <= set(_labels(render_svg(_page))), "a page coarsened its ticks"

_kinds = {"x": [1.0, 2.0, 3.0], "y": [1.0, 2.0, 3.0], "kind": ["a", "b", "c"]}
_merged = render_svg(data(_kinds, name="kinds") + point + x(col.x) + y(col.y)
                     + color(col.kind) + shape(col.kind))
assert _labels(_merged).count("Kind") == 1, "color and shape on one column drew two keys"
assert "`shape(<column>)`" in _refusal(lambda: render_svg(
    data(_kinds, name="kinds") + point + x(col.x) + y(col.y) + color(col.kind) + palette("gray")))
_clear = render_svg(data(_kinds, name="kinds") + point + x(col.x) + y(col.y)
                    + theme(background="transparent"))
assert '<rect width="800" height="600" fill="white"/>' not in _clear, "a transparent figure is white"
_squared = render_svg(data(_kinds, name="kinds") + point + x(col.x) + y(col.y) + theme(ratio=1))
assert float(re.search(r"rotate\(-90 ([0-9.]+) ", _squared).group(1)) > 50, \
    "the y name stayed at the image's edge"

assert "a `cross` is two strokes with no fill" in _refusal(lambda: render_svg(
    data(_kinds, name="kinds") + point + x(col.x) + y(col.y)
    + style(shape="cross", border_color="red", border_size=2)))

_links = {"src": ["a", "a", "b"], "dst": ["b", "c", "c"]}
assert "`label(name)`" in _refusal(lambda: render_svg(
    data(_links, name="links") + text * layout(col.src, col.dst) + label(col.src) + network()))
assert "stroke-dasharray" in render_svg(
    data(_links, name="links") + edge * layout(col.src, col.dst) + pattern(col.src) + network())
_boxes = {"g": ["a"] * 5 + ["b"] * 5, "v": [1.0, 2.0, 3.0, 4.0, 5.0, 2.0, 4.0, 6.0, 8.0, 10.0]}


def _caps(on):
    return render_svg(data(_boxes, name="boxes") + box + x(col.g) + y(col.v)
                      + style(caps=on)).count('stroke-linecap="round"/>')


assert (_caps(True), _caps(False)) == (4, 0), "style(caps=False) should leave a box's whiskers bare"
assert render_svg(data(_links, name="links") + edge * layout(col.src, col.dst)
                  + style(arrow="end") + network()).count("<polygon") == 3, \
    "style(arrow='end') should put a head on each edge"
# Two rows at every x, which is when a stroke zigzags and the note is said.
_ramp = {"x": [0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0],
         "y": [1.0, 3.0, 2.0, 4.0, 3.0, 5.0, 4.0, 6.0],
         "v": [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]}
with contextlib.redirect_stderr(io.StringIO()) as _said:
    render_svg(data(_ramp, name="ramp") + line + x(col.x) + y(col.y) + color(col.v))
assert "`color(v)` holds numbers" in _said.getvalue(), _said.getvalue()
ok("counts on every axis, calendar z, shared ticks, one key, transparent, names beside "
   "the panel, dashed edges, node labels")


# --- a partition read as proportion, the spine plot's axis, the filled pile ------
# A partition read as proportion divides its measure axis by the total and keeps
# its other axis's name; a spine plot's share axis runs 0 to 1 instead of -0 to 2;
# the refusal of a proportion beside a filled pile names `stack(share = TRUE)` and a
# way out that draws; and `box * jitter` is refused toward `dodge`.
_trips = {"city": ["A", "A", "B", "B"], "mode": ["car", "bus", "car", "bus"],
          "people": [30.0, 10.0, 20.0, 40.0]}
_lab = _labels(render_svg(data(_trips, name="trips") + x(col.people)
                          + zone * partition(col.city, col.mode, cross=True) * proportion
                          + color(col.mode)))
_nums = [float(l) for l in _lab if re.fullmatch(r"-?[0-9.]+", l)]
assert "Share of column" in _lab and "Proportion" not in _lab and _nums \
    and builtins.max(_nums) <= 1.0, f"partition * proportion should tick in shares: {_lab}"
_lab = _labels(render_svg(data(_trips, name="trips") + x(col.people)
                          + text * partition(col.city, col.mode) * proportion + label(col.name)))
assert {"A", "car"} <= set(_lab), f"text * partition * proportion should name the nodes: {_lab}"
_spine = _labels(render_svg(data(_trips, name="trips") + zone * partition(col.city, cross=True)
                            + x(col.people) + color(col.city)))
assert "-0" not in _spine and {"0.2", "1.0"} <= set(_spine), \
    f"a spine plot's share axis should run 0 to 1: {_spine}"
for _what, _thunk, _fragment in [
    ("proportion beside a filled pile",
     lambda: render_svg(data(_trips, name="trips") + bar * stack(share=True) * proportion
                        + x(col.city) + color(col.mode)),
     "`bar * count * stack(share = TRUE)` for shares within each pile"),
    ("jitter on a box",
     lambda: render_svg(data(_trips, name="trips") + box * jitter + x(col.city) + y(col.people)),
     "`dodge` sets them side by side"),
]:
    assert _fragment in _refusal(_thunk), f"{_what}: {_refusal(_thunk)}"
ok("partition * proportion in shares, the spine plot's axis, the filled-pile refusal, "
   "and box * jitter refused")


# --- a written name on an axis with no numbers, jitter(0), bounds on a bar -------
# A name written with `y_label()` is drawn where an axis draws no numbers (a moved
# pile, a partition's ring), where it used to be dropped in silence; `jitter(0)`
# draws with a note that it moved nothing; and the `bounds` refusal names `zone`,
# the fifth mark that takes the pair.
_weeks = {"week": [1.0, 2.0, 3.0, 4.0] * 2, "plays": [3.0, 4.0, 5.0, 4.0, 2.0, 3.0, 2.0, 4.0],
          "genre": ["folk"] * 4 + ["jazz"] * 4}
_lab = _labels(render_svg(data(_weeks, name="weeks") + area * stack(baseline="wiggle")
                          + x(col.week) + y(col.plays) + color(col.genre)
                          + y_label("Plays per week")))
assert "Plays per week" in _lab, f"a written y_label should be drawn on a moved pile: {_lab}"
_strip3 = {"g": ["a", "a", "a", "b", "b", "b"], "v": [1.0, 2.0, 3.0, 2.0, 3.0, 4.0]}
with contextlib.redirect_stderr(io.StringIO()) as _said:
    render_svg(data(_strip3, name="strip3") + point * jitter(0) + x(col.g) + y(col.v))
assert "`jitter(0)` moves no point" in _said.getvalue(), _said.getvalue()
assert "`zone` shades the region between them" in _refusal(
    lambda: render_svg(data(_strip3, name="strip3") + bar * bounds(col.v, col.v) + x(col.g)))
ok("a written name on an axis with no numbers, jitter(0)'s note, and zone in the bounds refusal")


# --- a partition's axes run from 0 ------------------------------------------------
# They are ticked over the cells, not over the cells' centers, so a mosaic's share
# axis reads 0.0 to 1.0, and a sunburst's angle labels 0 once, at the top, where its
# total would share the spoke.
_lab = _labels(render_svg(data(_trips, name="trips") + zone * partition(col.city, col.mode, cross=True)
                          + x(col.people) + color(col.mode)))
assert {"0", "0.0", "1.0"} <= set(_lab), f"a mosaic's axes should run from 0: {_lab}"
_lab = _labels(render_svg(data(_trips, name="trips") + zone * partition(col.city, col.mode)
                          + x(col.people) + color(col.city) + polar()))
assert _lab.count("0") == 1 and "100" not in _lab, f"a sunburst labels 0 once: {_lab}"
ok("a partition's axes are ticked from 0 over its cells")


# --- a summary of one-row groups says so ------------------------------------------
_spread = {"gdp": [1.5, 2.5, 3.5, 4.5], "life": [50.0, 60.0, 70.0, 80.0]}
with contextlib.redirect_stderr(io.StringIO()) as _said:
    render_svg(data(_spread, name="spread") + bar * mean + x(col.gdp) + y(col.life))
assert "every group holds a single row" in _said.getvalue() \
    and "`bar * bin * mean`" in _said.getvalue(), _said.getvalue()
_twice = {"gdp": [1.0, 1.0, 2.0, 2.0], "life": [50.0, 60.0, 70.0, 80.0]}
with contextlib.redirect_stderr(io.StringIO()) as _said:
    render_svg(data(_twice, name="twice") + bar * mean + x(col.gdp) + y(col.life))
assert "every group holds a single row" not in _said.getvalue(), _said.getvalue()
ok("a summary of one-row groups says so, and a real summary does not")


# --- every plot is a named image ----------------------------------------------------
_pts = {"gdp": [1.0, 2.0, 3.0], "life": [50.0, 60.0, 70.0]}
_svg = render_svg(data(_pts, name="pts") + point + x(col.gdp) + y(col.life))
assert 'role="img" aria-label="Points, x is gdp, y is life"' in _svg, _svg[:200]
assert 'aria-label="Longer lives"' in render_svg(
    data(_pts, name="pts") + point + x(col.gdp) + y(col.life) + title("Longer lives"))
ok("every plot carries an accessible name")


# --- a color name means one color -------------------------------------------------
_d5 = {"a": [1.0, 2.0, 3.0, 4.0, 5.0], "b": [1.0, 2.0, 3.0, 4.0, 5.0], "v": [1.0, 2.0, 3.0, 4.0, 5.0]}
_svg = render_svg(data(_d5, name="d5") + point + x(col.a) + y(col.b) + color(col.v)
                  + palette(["white", "green"]))
assert "#008000" in _svg and "#00ff00" not in _svg.lower(), "a ramp through green ends at CSS green"
ok("a palette's green is CSS's green")


# --- a page carries only the columns its plot names --------------------------------
_people = {"gdp": [1000.0, 20000.0, 40000.0], "life": [50.0, 70.0, 80.0],
           "email": ["a@x.org", "b@x.org", "c@x.org"]}
_bp = (data(_people, name="people") + point + x(col.gdp) + y(col.life)
       + brush(col.gdp, at=(2000, 30000)))
_block = _R.svg_block(render_svg(_bp), _bp)
if "<script" in _block:
    assert "a@x.org" not in _block and '"email"' not in _block, "an unmapped column was published"
    assert '"gdp"' in _block and '"life"' in _block, "a mapped column was dropped"
    ok("a page carries only the columns its plot names")
else:
    print("SKIP: browser engine not built, so the payload cannot be checked")


# --- smooth_band: the band around a smooth line ------------------------------------
_pts = {"g": [float(i) for i in builtins.range(1, 31)],
        "v": [__import__("math").sin(i / 5) * 3 + (i % 4) for i in builtins.range(1, 31)]}


def _band(*args):
    return render_svg(data(_pts, name="pts") + ribbon * smooth_band(*args) + x(col.g) + y(col.v)
                      + line * smooth + x(col.g) + y(col.v))


assert "<polygon" in _band() and "Ribbons derived by smooth_band" in _band(), "the band did not draw"
assert _band(0.5) != _band(0.99), "smooth_band's level did not reach the engine"
refuses("a smooth_band level outside (0, 1)", lambda: smooth_band(1.5))
# The band needs five rows where its curve needs three: below five every local fit
# passes through its own points and leaves nothing to measure a width from.
try:
    render_svg(data({"g": _pts["g"][:4], "v": _pts["v"][:4]}, name="four")
               + ribbon * smooth_band + x(col.g) + y(col.v))
    raise AssertionError("FAIL: smooth_band on four rows should be refused")
except GogError as _e:
    assert "`smooth_band`" in str(_e) and "at least 5" in str(_e), str(_e)
    assert "`line * smooth`" in str(_e), str(_e)
ok("smooth_band draws the band around smooth, at its level")


# --- the parentheses refusal, and a bar split by what it takes ---------------------
_a = {"g": [1.0, 2.0, 3.0], "v": [1.0, 2.0, 3.0]}
_lines = {"at": [1.5, 2.5]}
try:
    data(_a, name="a") + x(col.g) + y(col.v) + point + (data(_lines, name="lines") + rule + x(col.at))
    raise AssertionError("FAIL: marks in parentheses were accepted")
except GogError as _e:
    assert "`+ data(lines) + rule`" in str(_e) and "area" not in str(_e), str(_e)
try:
    data(_a, name="a") + x(col.g) + y(col.v) + point + (data(_lines, name="lines") + x(col.at))
    raise AssertionError("FAIL: a group with no mark was accepted")
except GogError as _e:
    assert "do not group the parts of a plot" in str(_e), str(_e)
_eras = {"continent": ["Asia", "Asia", "Europe", "Europe"], "era": ["1957", "2007"] * 2,
         "life": [50.0, 60.0, 65.0, 75.0]}
assert "Add `color(<field>)` or `pattern(<field>)`" in _refusal(
    lambda: render_svg(data(_eras, name="eras") + bar * mean * dodge + x(col.continent) + y(col.life)))
assert "<rect" in render_svg(data(_eras, name="eras") + bar * count * stack + pattern(col.era))
ok("the parentheses refusal names the reader's marks; a bar is split by what it takes")


# --- Law 7's second half: positions with no mark ----------------------------
# Refused, as a mark with no positions always was, and told which mark to add.
# Each drew an empty panel with made-up axes. The same block runs in all four.
_quiet = {"gdp": [1.0, 2.0, 3.0], "life": [4.0, 5.0, 6.0],
          "continent": ["Asia", "Europe", "Africa"]}
for _what, _thunk, _fragment in [
    ("two positions and no mark",
     lambda: render_svg(data(_quiet) + x(col.gdp) + y(col.life)),
     "`point` draws a dot for each row"),
    ("a number and no mark", lambda: render_svg(data(_quiet) + x(col.gdp)),
     "`bar * bin` shows how the values of `gdp` are spread"),
    ("a category and no mark", lambda: render_svg(data(_quiet) + y(col.continent)),
     "`bar * count` counts the rows for each value of `continent`"),
    ("a table and no mark", lambda: render_svg(data(_quiet)), "this plot has no mark"),
]:
    _text = _refusal(_thunk)
    assert _fragment in _text, f"{_what}: wanted {_fragment!r} in {_text}"
render_svg(data(_quiet) + bar * bin + x(col.gdp))
ok("a plot with no mark is refused, with the mark to add")


# --- an edge with no layout has nothing to draw ------------------------------
# Its geometry is the two nodes a layout places. It drew an empty panel with
# made-up axes, in silence. The same block runs in all four bindings.
_links = {"from": ["a", "b", "c"], "to": ["b", "c", "a"]}
assert "an `edge` with no `layout` has nothing to draw" in _refusal(
    lambda: render_svg(data(_links) + edge))
assert "`edge * layout(<from>, <to>) + network()`" in _refusal(
    lambda: render_svg(data(_links) + edge + color(col["from"])))
render_svg(data(_links) + edge * layout(col["from"], col.to) + network())
ok("an edge with no layout is refused, with the sentence that draws it")


# --- a bare space() over a transform in the plane is drawn flat --------------
# `bounds`, `partition` and a cluster tree place things in the plane, and each
# stood a bare `space()` in an empty cube. The same block runs in all four.
import warnings as _warnings  # noqa: E402
_spans = {"t": [1.0, 2.0, 3.0, 4.0], "lo": [1.0, 2.0, 3.0, 4.0], "hi": [2.0, 3.0, 5.0, 6.0]}
with _warnings.catch_warnings():
    _warnings.simplefilter("ignore")
    _cube = render_svg(data(_spans) + zone * bounds(col.lo, col.hi) + x(col.t) + space())
assert _cube == render_svg(data(_spans) + zone * bounds(col.lo, col.hi) + x(col.t)), \
    "a bare space() over bounds should draw the flat plot"
ok("a bare space() over a transform in the plane is drawn flat")


# --- a smoothed point in the cube --------------------------------------------
# It lost its third position and drew an empty cube, in silence. It is refused
# as every smooth in the cube is. The same block runs in all four bindings.
_drift = {"g": [1.5, 2.5, 3.1, 4.2, 5.3, 6.1], "h": [3.0, 1.0, 4.0, 1.0, 5.0, 9.0],
          "n": [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]}
assert "`bar * mean + x(<a>) + y(<b>) + z(<column>) + space()`" in _refusal(
    lambda: render_svg(data(_drift) + point * smooth + x(col.g) + y(col.h) + z(col.n)))
render_svg(data(_drift) + point * smooth + x(col.g) + y(col.h))
ok("a smoothed point in the cube is refused toward the plane")


# --- a flow stage named twice ------------------------------------------------
# Stages are placed by their column's name, so a second `class` fell on the
# first, with no message. The same block runs in all four bindings.
_voyage = {"class": ["1st", "2nd", "3rd", "1st"], "fate": ["lived", "died", "died", "lived"]}
assert "`class` is named twice in `flow()`" in _refusal(
    lambda: render_svg(data(_voyage) + zone * flow(col["class"], col["class"])))
assert "Name each column once" in _refusal(
    lambda: render_svg(data(_voyage) + ribbon * flow(col["class"], col.fate, col["class"])))
render_svg(data(_voyage) + zone * flow(col["class"], col.fate))
ok("a flow stage named twice is refused")


# --- a split tally with a key on y lies on its side ---------------------------
# A tally's `y`, split and stacked with no `x`, is the key of a bar on its side, as
# it is unsplit. The one-pile rule swallowed it, and the counts were drawn in one
# pile under an axis titled after the column. The same block runs in all four.
_winds = {"dir": ["N", "S", "N", "S"], "season": ["win", "win", "sum", "sum"],
          "speed": [3.0, 5.0, 2.0, 8.0]}
_sideways = render_svg(data(_winds) + bar * count * stack + y(col.season) + color(col.dir))
assert ">win<" in _sideways and ">Count<" in _sideways, \
    "a stacked tally with a category on y should lie on its side"
render_svg(data(_winds) + bar * sum * stack + y(col.speed) + color(col.dir))
ok("a split tally with a key on y lies on its side")


# --- a network row left out adds no node -------------------------------------
# A row with a missing end is left out, and said so, but its named end was still
# added as a node, standing alone. The same block runs in all four bindings.
_links = {"from": ["a", "a", "b", "c"], "to": ["b", "c", "c", "d"]}
_extra = {"from": ["a", "a", "b", "c", "e"], "to": ["b", "c", "c", "d", None]}


def _network(t):
    with _warnings.catch_warnings():
        _warnings.simplefilter("ignore")
        return render_svg(data(t, name="links") + edge * layout(col["from"], col.to)
                          + point * layout(col["from"], col.to) + network())


assert _network(_links) == _network(_extra), "a row left out should add no node"
ok("a network row left out adds no node")


# --- a second table's points after a bar are drawn inside the panel ---------
# They were placed beyond the panel and clipped away, in silence. The same block
# runs in all four bindings.
import re as _re2  # noqa: E402
_actuals = {"year": [2019.0, 2020.0, 2021.0, 2022.0, 2023.0], "sales": [3.0, 4.0, 5.0, 4.0, 6.0]}
_forecast = {"year": [2024.0, 2025.0, 2026.0], "sales": [6.5, 7.0, 7.4]}
_svg = render_svg(data(_actuals, name="actuals") + x(col.year) + y(col.sales) + bar
                  + data(_forecast, name="forecast") + point)
_cx = [float(v) for v in _re2.findall(r'cx="([0-9.]+)', _svg)]
assert len(_cx) == 3 and all(v <= 800 for v in _cx), f"a forecast point is off the canvas: {_cx}"
assert ">2026<" in _svg, "the axis should reach 2026"
ok("a second table's points after a bar are drawn inside the panel")


# --- a one-column rule on a map is projected ---------------------------------
# It kept its raw degrees, and a parallel at 45 collapsed the map to a sliver. It
# is placed now exactly as the same rule with both columns. The same block runs
# in all four bindings.
_pts = {"lon": [-10.0, 0.0, 10.0, 5.0], "lat": [30.0, 50.0, 60.0, 40.0]}
_par = lambda t: render_svg(data(_pts, name="pts") + point + x(col.lon) + y(col.lat)  # noqa: E731
                            + data(t, name="par") + rule + y(col.lat) + map())
assert _par({"lat": [45.0]}) == _par({"lon": [0.0], "lat": [45.0]}), \
    "a parallel from one column should be placed as from two"
ok("a one-column rule on a map is projected")


# --- a second facet in one direction, and facets with no plot ----------------
# A plot splits once across and once down, so a second facet in one direction
# replaced the first, in silence; and a pair of facets with no plot raised an
# AttributeError. The same block runs in all four bindings.
_split = {"a": [1.0, 2.0, 3.0, 4.0], "b": [1.0, 2.0, 3.0, 4.0],
          "g": ["p", "q", "p", "q"], "h": ["u", "u", "v", "v"]}
assert "already split into panel columns by `g`" in _refusal(
    lambda: data(_split) + point + x(col.a) + y(col.b) | facet(col.g) | facet(col.h))
assert "already split into panel rows by `g`" in _refusal(
    lambda: data(_split) + point + x(col.a) + y(col.b) / facet(col.g) / facet(col.h))
assert "`facet()` with no plot to split" in _refusal(
    lambda: render_svg(facet(col.g) | facet(col.g)))
render_svg(data(_split) + point + x(col.a) + y(col.b) | facet(col.g) / facet(col.h))
ok("a second facet in one direction, and facets with no plot, are refused")


# --- a second coordinate space -------------------------------------------------
# Each space atom replaced the one before it, so the engine saw only the last:
# `space() + polar()` drew polar, in silence. A second, different space is refused
# now; restating one re-angles it. The same block runs in all four bindings.
_view = {"a": [1.0, 2.0, 3.0, 4.0], "b": [2.0, 3.0, 1.0, 4.0], "c": [1.0, 2.0, 3.0, 4.0]}
assert "already drawn in `space()`, and `polar()` asks for a second space" in _refusal(
    lambda: data(_view) + point + x(col.a) + y(col.b) + space() + polar())
assert "already drawn in `polar()`, and `space()` asks for a second space" in _refusal(
    lambda: data(_view) + point + x(col.a) + y(col.b) + polar() + space() + z(col.c))
assert "already drawn in `map()`" in _refusal(
    lambda: data(_view) + point + x(col.a) + y(col.b) + map() + polar())
render_svg(data(_view) + point + x(col.a) + y(col.b) + z(col.c) + space() + space(turn=60))
ok("a second coordinate space is refused, and a restated one re-angles")


# --- a page refuses one column read through two scales ------------------------
# A page shares an axis by column, and it merged two plots that read the column
# through different scales, in silence. The same block runs in all four bindings.
_wealth = {"gdp": [1000.0, 2000.0, 5000.0, 20000.0, 40000.0], "life": [50.0, 60.0, 65.0, 72.0, 80.0]}
assert "`gdp` is on the x axis of two plots on this page" in _refusal(
    lambda: render_svg((data(_wealth) + bar * bin + x(col.gdp))
                       / (data(_wealth) + point + x(col.gdp, scale="log") + y(col.life))))
render_svg((data(_wealth) + bar * bin + x(col.gdp, scale="log"))
           / (data(_wealth) + point + x(col.gdp, scale="log") + y(col.life)))
ok("a page refuses one column read through two scales")


# --- a tree under a bare space() is drawn flat ---------------------------------
# As every plot with no third dimension is; the note says to drop `space()` rather
# than to add a `z` the tree would refuse. The same block runs in all four bindings.
_menu = {"food": ["a", "a", "b", "b", "c", "c"], "nutrient": ["p", "q"] * 3,
         "amount": [1.0, 2.0, 1.5, 2.5, 5.0, 1.0]}
with _warnings.catch_warnings():
    _warnings.simplefilter("ignore")
    _cube = render_svg(data(_menu) + path * cluster(col.amount, over=col.nutrient)
                       + x(col.food) + space())
assert _cube == render_svg(data(_menu) + path * cluster(col.amount, over=col.nutrient)
                           + x(col.food)), "a tree under a bare space() should draw the flat tree"
ok("a tree under a bare space() is drawn flat")

# A border on `text` is a halo under its letters: the glyph's outline stroked
# beneath its fill, so a name written over a line reads clearly. It was refused
# until 2026-09-26 ("a `text` mark draws glyphs, not a filled shape").
_halo_t = {"x": [1.0, 2.0, 3.0], "y": [1.0, 3.0, 2.0], "name": ["a", "b", "c"]}
_haloed = render_svg(data(_halo_t, name="halo_t") + x(col.x) + y(col.y) + line + text
                     + label(col.name) + style(border_color="white", border_size=5))
assert 'fill="none" stroke="white" stroke-width="5"' in _haloed, \
    "style(border_color=, border_size=) on text should draw a halo"
assert 'text-anchor="middle" fill="none" stroke="' not in render_svg(data(_halo_t, name="halo_t") + x(col.x) + y(col.y)
                                       + line + text + label(col.name)), \
    "a label with no border must have no halo"
ok("a border on `text` is a halo under its letters")

# One density layer is smoothed by one bandwidth: the mean of the bandwidths its
# groups would each choose alone. So two groups of one shape draw one violin,
# whatever their row counts. Until 2026-09-26 each group took its own, and a
# group with every row written twice was drawn sharper than its twin.
_v = [1.0, 2.0, 2.5, 4.0, 4.2, 5.0, 7.0, 7.5, 9.0, 12.0]
_same_shape = {"g": ["once"] * 10 + ["twice"] * 20, "v": _v * 3}
_svg = render_svg(data(_same_shape, name="same_shape") + ribbon * density(compare="shape")
                  + x(col.g) + y(col.v))
_heights = [[p.split(",")[1] for p in pts.split()]
            for pts in re.findall(r'<polygon points="([^"]*)"', _svg)]
assert len(_heights) == 2 and _heights[0] == _heights[1], \
    "two groups of one shape should draw one violin, whatever their row counts"
ok("a density layer smooths every group by one bandwidth")

# `point * dodge` is the beeswarm: each point in a category moves across its slot
# only as far as it must to clear the others, and never along the measure axis.
# Six rows at one value must land at six places. It was refused toward `jitter`
# until 2026-09-26.
_tied = {"g": ["a"] * 6, "v": [1.0] * 6}
_svg = render_svg(data(_tied, name="tied") + point * dodge + x(col.g) + y(col.v))
_dots = re.findall(r'<circle cx="([0-9.]+)" cy="([0-9.]+)"', _svg)
assert len(_dots) == 6 and len({cx for cx, _ in _dots}) == 6, \
    "point * dodge should set six tied points at six places"
assert len({cy for _, cy in _dots}) == 1, "point * dodge must not move a point along the measure axis"
ok("point * dodge sets tied points apart across the slot only")

# A network draws its separate parts at one scale: a pair's one edge is about as
# long as the edges of a large ring beside it. Until 2026-09-26 each part was
# stretched to fill a cell sized by its node count, which drew a pair as a long
# stroke beside short ring edges.
_ring_names = [f"r{i:02d}" for i in builtins.range(24)]
_parts = {"a": _ring_names + ["a0", "a1", "a2", "a3"],
          "b": _ring_names[1:] + _ring_names[:1] + ["b0", "b1", "b2", "b3"]}
_svg = render_svg(data(_parts, name="parts") + edge * layout(col.a, col.b) + network())
_ends = [tuple(builtins.map(float, m)) for m in
         re.findall(r'<line x1="([0-9.]+)" y1="([0-9.]+)" x2="([0-9.]+)" y2="([0-9.]+)"', _svg)]
_len = [math.hypot(x2 - x1, y2 - y1) for x1, y1, x2, y2 in _ends]
_ring = builtins.sum(_len[:24]) / 24
assert len(_len) == 28 and all(0.5 < l / _ring < 2 for l in _len[24:]), \
    "a pair's edge should be drawn about as long as the ring's edges"
ok("a network draws its separate parts at one scale")

# A played column's frames are held in proportion to the gap to the next value:
# 2002 stands for 18 years before 2020 and is held four times the usual 0.8s,
# the longest a frame is held. Until 2026-09-26 every frame held 0.8s.
_uneven = {"a": [1.0, 2.0, 3.0, 4.0], "b": [4.0, 3.0, 2.0, 1.0], "t": [2000.0, 2001.0, 2002.0, 2020.0]}
_svg = render_svg(data(_uneven, name="uneven") + point + x(col.a) + y(col.b) + play(col.t))
_begins = sorted({float(b) for b in re.findall(r'begin="([0-9.]+)s"', _svg)})
assert _begins == [0.0, 0.8, 1.6, 4.8], f"frames of an uneven column should begin at 0, 0.8, 1.6, 4.8: {_begins}"
ok("a played column's frames are held in proportion to its gaps")

# Category names that do not fit side by side are turned to read upward, and when
# even turned they are too close, one in every few is drawn and the engine says
# how many. Until 2026-09-26 forty names printed over each other, and so did
# three hundred.
def _crowd(n):
    return {"g": [f"name {i:03d}" for i in builtins.range(1, n + 1)],
            "v": [float(i) for i in builtins.range(1, n + 1)]}
with contextlib.redirect_stderr(io.StringIO()) as _said:
    _svg = render_svg(data(_crowd(40), name="forty") + bar + x(col.g) + y(col.v))
assert _svg.count("rotate(-90.00 ") == 40 and _said.getvalue() == "", \
    "forty crowded names should all be drawn, turned to read upward, in silence"
with contextlib.redirect_stderr(io.StringIO()) as _said:
    _svg = render_svg(data(_crowd(300), name="many") + bar + x(col.g) + y(col.v))
_drawn = len(re.findall(r">name [0-9]{3}</text>", _svg))
assert _drawn < 300 and f"({_drawn} of 300)" in _said.getvalue(), \
    "three hundred names should be thinned, and the message should count the ones drawn"
ok("crowded category names turn, then thin, and say so")


# --- A treemap's label report ------------------------------------------------
# It says "do not fit", since a name can fail on height as well as width; it
# agrees in number; and it calls the packing whole only when every share has a
# region. The same block runs in all four bindings.
_one = {"g": ["roomy", "cramped"], "v": [240.0, 1.0]}
_tiny = {"g": ["big", "mid", "gone"], "v": [1e9, 5e8, 1.0]}
_said = io.StringIO()
with contextlib.redirect_stderr(_said):
    render_svg(data(_one) + bar + y(col.v) + color(col.g) + text + label(col.g) + nest())
assert "1 of 2 labels are drawn — one does not fit inside the region it names" in _said.getvalue(), \
    _said.getvalue()
_said = io.StringIO()
with contextlib.redirect_stderr(_said):
    render_svg(data(_tiny) + bar + y(col.v) + color(col.g) + text + label(col.g) + nest())
assert "one share is too small to have a region at all" in _said.getvalue(), _said.getvalue()
assert "drew every share" not in _said.getvalue(), _said.getvalue()
ok("a treemap's label report agrees in number, and calls the packing whole only when it is")


# --- An axis holds one kind of value -----------------------------------------
# Two tables that give one position column two kinds are refused, and the
# message names each table. Each drew its numbers as categories' places, off the
# plot, with nothing said. The same block runs in all four bindings.
_cats = {"g": ["a", "b", "c"], "v": [1.0, 2.0, 3.0]}
_nums = {"g": [10.0], "v": [2.0], "t": ["ten"]}
_pts = {"g": [1.0, 2.0, 3.0], "v": [1.0, 2.0, 3.0]}
_lab = {"g": ["b"], "v": [2.0], "t": ["bee"]}
_text = _refusal(lambda: render_svg(data(_cats, name="cats") + bar + x(col.g) + y(col.v)
                                    + data(_nums, name="nums") + text + label(col.t)))
assert ("`g` holds text in `cats` (read by `bar`), and `g` holds numbers in `nums` "
        "(read by `text`)") in _text, _text
_text = _refusal(lambda: render_svg(data(_pts, name="pts") + point + x(col.g) + y(col.v)
                                    + data(_lab, name="lab") + text + label(col.t)))
assert "the `x` axis is read two ways" in _text, _text
assert ">bee</text>" in render_svg(data(_cats, name="cats") + bar + x(col.g) + y(col.v)
                                   + data(_lab, name="lab") + text + label(col.t))
ok("an axis holds one kind of value, whichever table a layer reads")


# --- A map is not brushed ----------------------------------------------------
# A map projects longitude and latitude before it draws, so a range in degrees
# is no rectangle on the page, and a selection counted one set of rows while it
# dimmed another. The refusal points at a flat plot of the same columns, which
# draws. The same block runs in all four bindings.
_places = {"lon": [-10.0, 0.0, 20.0], "lat": [40.0, 50.0, 60.0]}
assert "`brush` cannot select on a `map()`" in _refusal(
    lambda: render_svg(data(_places, name="places") + point + x(col.lon) + y(col.lat) + map()
                       + brush(col.lon, at=(-5, 25))))
assert "`point + x(lon) + y(lat) + brush(lon)`" in _refusal(
    lambda: render_svg(data(_places, name="places") + point + x(col.lon) + y(col.lat) + map()
                       + brush))
assert "<circle" in render_svg(data(_places, name="places") + point + x(col.lon) + y(col.lat)
                               + brush(col.lon, at=(-5, 25)))
ok("a map is not brushed, and the flat plot it points to draws")


# --- One color column per plot -----------------------------------------------
# A plot draws one color legend, so its layers map `color` from one column. A
# second column's colors reached the reader with no key; it is refused, with a
# channel of its own that draws a key. The same column on every layer, and a
# legend turned off on purpose, still draw. The same block runs in all four.
_asia = {"year": [2000.0, 2001.0, 2000.0, 2001.0], "life": [60.0, 61.0, 70.0, 71.0],
         "country": ["A", "A", "B", "B"], "continent": ["Asia"] * 4}
assert "`pattern(continent)` on the `line`" in _refusal(
    lambda: render_svg(data(_asia, name="asia") + x(col.year) + y(col.life) + point
                       + color(col.country) + line + color(col.continent)))
assert "<circle" in render_svg(data(_asia, name="asia") + x(col.year) + y(col.life) + point
                               + color(col.country) + line + color(col.country))
assert "<circle" in render_svg(data(_asia, name="asia") + x(col.year) + y(col.life) + point
                               + color(col.country) + line + color(col.continent, legend=False))
ok("a second color column is refused, with a channel of its own")


# --- Crowded numbers thin as names do ----------------------------------------
# Five narrow panels printed 0K to 50K through each other, and now keep every
# other number, the ones a coarser step would choose, with nothing said. A
# `tick_count` the caller wrote that is thinned is said out loud. The same block
# runs in all four bindings.
_crowd = {"gdp": [300.0, 12000.0, 25000.0, 49000.0, 900.0, 30000.0],
          "life": [45.0, 60.0, 70.0, 80.0, 50.0, 75.0], "g": ["a", "b", "c", "d", "e", "a"]}
_lab = _labels(render_svg(data(_crowd, name="crowd") + point + x(col.gdp) + y(col.life)
                          | facet(col.g)))
assert {"20K", "40K"} <= set(_lab) and not {"10K", "30K"} & set(_lab), _lab
_said = io.StringIO()
with contextlib.redirect_stderr(_said):
    render_svg(data(_crowd, name="crowd") + point + x(col.gdp, tick_count=12) + y(col.life)
               | facet(col.g))
assert "tick_count = 12" in _said.getvalue() and "so 3 of them are drawn" in _said.getvalue(), _said.getvalue()
ok("crowded numbers thin, and a written tick_count says so")


# --- A negative weight in a partition or a flow ------------------------------
# It has no share and no thickness, so both refuse it as `nest()` does. Both
# clamped it to 0 and drew parts that no longer summed to the table, with
# nothing said. The same block runs in all four bindings.
_neg = {"a": ["p", "p", "q"], "b": ["u", "v", "u"], "w": [3.0, -2.0, 4.0]}
assert "`x(w)` weighs each branch" in _refusal(
    lambda: render_svg(data(_neg, name="neg") + zone * partition(col.a, col.b) + x(col.w)))
assert "`y(w)` weighs each path" in _refusal(
    lambda: render_svg(data(_neg, name="neg") + ribbon * flow(col.a, col.b) + y(col.w)))
ok("a negative weight is refused by a partition and a flow")


# --- A `y` under a partition --------------------------------------------------
# It was ignored as data and printed as the axis title; a partition reads its
# weight from `x`, so `y` is refused toward it. The same block runs in all four.
_trips = {"city": ["A", "A", "B"], "mode": ["car", "bus", "car"], "people": [3.0, 2.0, 4.0]}
assert "`y(people)` has no reading under `partition`" in _refusal(
    lambda: render_svg(data(_trips, name="trips") + zone * partition(col.city, col.mode)
                       + y(col.people)))
ok("a y under a partition is refused toward x")


# --- A shared axis split into panels differently on a page --------------------
# A histogram over a faceted scatter spanned every panel and lined up with none;
# refused. Both split the same way still draw. The same block runs in all four.
_g = {"gdp": [1.0, 2.0, 3.0, 4.0, 5.0, 6.0], "life": [50.0, 55.0, 60.0, 65.0, 70.0, 75.0],
      "continent": ["A", "A", "B", "B", "C", "C"]}
assert "split into panels differently" in _refusal(
    lambda: render_svg((data(_g, name="g") + bar * bin + x(col.gdp))
                       / (data(_g, name="g") + point + x(col.gdp) + y(col.life) | facet(col.continent))))
assert "<circle" in render_svg((data(_g, name="g") + bar * bin + x(col.gdp) | facet(col.continent))
                               / (data(_g, name="g") + point + x(col.gdp) + y(col.life)
                                  | facet(col.continent)))
ok("a shared axis is split into panels the same way on a page")


# --- A category on the axis `bounds` draws ------------------------------------
# Never read, so one band combed through every group; refused, with the splits
# that draw a band per group. The same block runs in all four bindings.
_rid = {"at": [1.0, 2.0, 1.0, 2.0], "zero": [0.0] * 4, "height": [1.0, 3.0, 2.0, 1.0],
        "g": ["a", "a", "b", "b"]}
assert "`ribbon * bounds` draws its band" in _refusal(
    lambda: render_svg(data(_rid, name="rid") + ribbon * bounds(col.zero, col.height)
                       + x(col.at) + y(col.g)))
ok("a category on the axis bounds draws is refused")


# --- `area` through rows at one `x` -------------------------------------------
# Joined in x order, as `line` is, so it zigzags inside the value; now said, and
# only when it happens. The same block runs in all four bindings.
_zig = {"c": ["a", "a", "a", "b", "b"], "v": [1.0, 3.0, 2.0, 4.0, 1.0]}
_said = io.StringIO()
with contextlib.redirect_stderr(_said):
    render_svg(data(_zig, name="zig") + area + x(col.c) + y(col.v))
assert "zigzags inside that value" in _said.getvalue(), _said.getvalue()
_said = io.StringIO()
with contextlib.redirect_stderr(_said):
    render_svg(data(_zig, name="zig") + area * mean + x(col.c) + y(col.v))
assert "zigzags" not in _said.getvalue(), _said.getvalue()
# `line` asks the same question, and one long series is not a zigzag.
_said = io.StringIO()
with contextlib.redirect_stderr(_said):
    render_svg(data(_zig, name="zig") + line + x(col.c) + y(col.v))
assert "zigzags inside that value" in _said.getvalue(), _said.getvalue()
_said = io.StringIO()
with contextlib.redirect_stderr(_said):
    render_svg(data({"day": [float(i) for i in builtins.range(40)],
                     "v": [float(i % 7) for i in builtins.range(40)]}, name="one")
               + line + x(col.day) + y(col.v))
assert _said.getvalue() == "", _said.getvalue()
ok("an area through rows at one x says it zigzags")


# --- A summarized point on its side ------------------------------------------
# With the category on `y` it summarizes along `x`, one dot per category, where
# it drew every row. The same block runs in all four bindings.
_side = {"life": [40.0, 50.0, 60.0, 45.0, 55.0, 65.0], "year": ["a", "a", "a", "b", "b", "b"]}
assert render_svg(data(_side, name="side") + point * median + x(col.life)
                  + y(col.year)).count("<circle") == 2
ok("a summarized point lies on its side as a bar does")


# --- A flow band draws the pattern its legend shows --------------------------
# The legend drew the hatch while every band stayed solid. The same block runs
# in all four bindings.
_fl = {"a": ["p", "p", "q", "q"], "b": ["u", "v", "u", "v"], "n": [3.0, 2.0, 4.0, 1.0]}
_svg = render_svg(data(_fl, name="fl") + ribbon * flow(col.a, col.b) + y(col.n) + pattern(col.a))
assert re.search(r'<path d="M [^"]*" fill="url\(#', _svg), "no band takes a hatch"
ok("a flow band draws the pattern its legend shows")


# --- A legend keys only the categories the plot colored ----------------------
# A flow that left a category's rows out drew no band for it, and keyed it
# anyway. The same block runs in all four bindings.
_gone = {"a": ["p", "p", "q", "gone"], "b": ["u", "v", "u", None], "n": [3.0, 2.0, 4.0, 5.0]}
with contextlib.redirect_stderr(io.StringIO()):
    _svg = render_svg(data(_gone, name="gone") + ribbon * flow(col.a, col.b) + y(col.n) + color(col.a))
assert ">gone</text>" not in _svg and ">q</text>" in _svg
ok("a legend keys only the categories the plot colored")


# --- An empty label under `repel` ---------------------------------------------
# It names nothing: no text and no leader line, where it drew a leader to
# nothing. The same block runs in all four bindings.
_emp = {"x": [float(i % 4) for i in builtins.range(12)],
        "y": [(i // 4) * 0.2 for i in builtins.range(12)],
        "lab": ["named"] + [""] * 11}
_svg = render_svg(data(_emp, name="emp") + point + x(col.x) + y(col.y) + text * repel + label(col.lab))
assert _svg.count('stroke-width="0.7"') <= 1 and "></text>" not in _svg
ok("an empty label under repel draws no text and no leader")


# --- A pattern key beside another color column --------------------------------
# It takes neutral ink, not the color map's first hue. The same block runs in
# all four bindings.
_pk = {"c": ["a", "a", "b", "b"], "sex": ["m", "f", "m", "f"], "kept": ["yes", "no", "no", "yes"],
       "n": [3.0, 2.0, 4.0, 1.0]}
_svg = render_svg(data(_pk, name="pk") + bar * sum * dodge + x(col.c) + y(col.n) + color(col.sex)
                  + pattern(col.kept))
assert "#3c3c46" in _svg.split("Kept")[1]
ok("a pattern key beside another color column takes neutral ink")


# --- A facet panel reads the whole layer's size scale ----------------------------
# The one the legend decodes, so the low panel's largest dot is not drawn at the
# largest size. The same block runs in all four bindings.
_sh = {"x": [1.0, 2.0, 3.0, 1.0, 2.0, 3.0], "y": [1.0, 2.0, 3.0, 2.0, 3.0, 4.0],
       "n": [1.0, 2.0, 3.0, 10.0, 20.0, 30.0], "g": ["low"] * 3 + ["high"] * 3}
def _radii(svg):
    return sorted(re.findall(r'<circle[^>]* r="([^"]*)"', svg))
_whole = _radii(render_svg(data(_sh, name="sh") + point + x(col.x) + y(col.y) + size(col.n)))
_faceted = _radii(render_svg(data(_sh, name="sh") + point + x(col.x) + y(col.y) + size(col.n)
                             | facet(col.g)))
assert len(_whole) == 9 and _whole == _faceted, (_whole, _faceted)
ok("a facet panel reads the whole layer's size scale")


# --- A size key under a summary reads the summaries -----------------------------
# A dot is sized by its group's mean, so the key decodes the means rather than the
# raw column. The same block runs in all four bindings.
_sk = render_svg(data({"g": ["a", "a", "b", "b"], "v": [10.0, 30.0, 50.0, 70.0]}, name="sk")
                 + point * mean + x(col.g) + y(col.v) + size(col.v))
# The key's top row and the axis's top tick both read 60; the raw column's ends,
# 10 and 70, appear nowhere.
assert _sk.count(">60</text>") == 2 and ">10</text>" not in _sk and ">70</text>" not in _sk
ok("a size key under a summary reads the summaries")


# --- A legend too wide for its plot --------------------------------------------
# Left out and said, where the panel of a thin plot went to a negative width in
# silence. The same block runs in all four bindings.
_lw = {"x": [1.0, 2.0, 3.0, 4.0], "y": [1.0, 2.0, 3.0, 4.0],
       "g": ["a long category", "another long one", "a", "b"]}
_said = io.StringIO()
with contextlib.redirect_stderr(_said):
    _svg = render_svg((data(_lw, name="lw") + point + x(col.x) + y(col.y))
                      | (data(_lw, name="lw") + point + x(col.x) + y(col.y) + color(col.g)
                         + theme(width=150)))
assert "`G` legend needs" in _said.getvalue() and ">G</text>" not in _svg, _said.getvalue()
ok("a legend too wide for its plot is left out and said")


# --- A y name follows a panel a shared column moved ------------------------------
# In `(a | b) / c` with `c` sharing a column with `b`, the page moves `c`'s panel
# to run under `b`; its name stayed at the cell's edge, 450px away. The same block
# runs in all four bindings.
_yn = {"speed": [4.0, 7.0, 8.0, 12.0, 15.0, 18.0, 20.0, 24.0],
       "dist": [2.0, 4.0, 16.0, 24.0, 36.0, 56.0, 64.0, 120.0]}
_svg = render_svg(((data(_yn, name="yn") + point + x(col.dist) + y(col.speed))
                   | (data(_yn, name="yn") + point + x(col.speed) + y(col.dist)))
                  / (data(_yn, name="yn") + bar * bin + x(col.speed)))
_at = re.search(r'rotate\(-90 ([0-9.]+) [^)]*\)[^>]*>Count</text>', _svg)
assert _at and float(_at.group(1)) > 300, _at
ok("a y name follows a panel a shared column moved")


# --- A folded facet sharing a column keeps its width ----------------------------
# Two of them stacked on a shared column were each squeezed into one column's
# width, the rest of the cell empty. The same block runs in all four bindings.
_fw = {"speed": [4.0, 7.0, 8.0, 12.0, 15.0, 18.0], "dist": [2.0, 4.0, 16.0, 24.0, 36.0, 56.0],
       "g": ["a", "b", "c", "a", "b", "c"]}
_svg = render_svg((data(_fw, name="fw") + point + x(col.speed) + y(col.dist) | facet(col.g, wrap=2))
                  / (data(_fw, name="fw") + point + x(col.speed) + y(col.speed)
                     | facet(col.g, wrap=2)))
_reach = [builtins.max(float(a) + float(w) for a, w in re.findall(
              r'<clipPath id="[^"]*"><rect x="([^"]*)" y="[^"]*" width="([^"]*)"', cell))
          for cell in _svg.split("<svg ")[2:]]
assert len(_reach) == 2 and builtins.min(_reach) > 600, _reach
ok("a folded facet sharing a column keeps its width")


# --- A trailing data() is refused --------------------------------------------------
# No mark comes after it to read its table, where the plot drew as though it had
# never been written. A description of the plot still prints. The same block runs
# in all four bindings.
_actuals = {"year": [2019.0, 2020.0, 2021.0], "sales": [1.0, 2.0, 3.0]}
_forecast = {"year": [2024.0, 2025.0], "sales": [6.0, 7.0]}
_trailing = (data(_actuals, name="actuals") + x(col.year) + y(col.sales) + line + point
             + data(_forecast, name="forecast"))
try:
    render_svg(_trailing)
    raise AssertionError("a trailing data() drew")
except GogError as refusal:
    assert "`data(forecast)` ends the sentence" in str(refusal), refusal
assert "line + point" in repr(_trailing)
ok("a trailing data() is refused")


# --- A plot-wide channel every mark overrides is refused ---------------------------
# It reaches nothing, where it drew as though it had never been written. The same
# block runs in all four bindings.
_ov = {"gdp": [1.0, 2.0, 3.0], "life": [4.0, 5.0, 6.0], "pop": [7.0, 8.0, 9.0]}
try:
    render_svg(data(_ov, name="ov") + x(col.gdp) + y(col.life) + size(col.pop) + point
               + size(col.life))
    raise AssertionError("a plot-wide channel every mark overrides drew")
except GogError as refusal:
    assert "`point` maps `size(life)`" in str(refusal) and "reaches none of them" in str(refusal), \
        refusal
ok("a plot-wide channel every mark overrides is refused")


# --- A nest refuses the settings of an axis ------------------------------------------
# It has no axes, where each setting was accepted and drew the same bytes as the
# sentence without it. The same block runs in all four bindings.
_nt = {"g": ["a", "b", "c"], "v": [3.0, 2.0, 1.0]}
def _nest_said(p):
    try:
        render_svg(p)
        return ""
    except GogError as refusal:
        return str(refusal)
assert "`theme(grid = )`" in _nest_said(
    data(_nt, name="nt") + bar * sum + y(col.v) + color(col.g) + nest() + theme(grid="both"))
assert "`y(v, tick_count = 3)`" in _nest_said(
    data(_nt, name="nt") + bar * sum + y(col.v, tick_count=3) + color(col.g) + nest())
assert "under `bar * sum`" in _nest_said(
    data(_nt, name="nt") + bar * sum + y(col.v, limits=(0, 10)) + color(col.g) + nest())
ok("a nest refuses the settings of an axis")


# --- order() with no column is refused by name -------------------------------------
# `order(desc=True)` alone raised Python's missing-argument error. The refusal names
# the spelling that runs a category axis backward. The same block runs in all four
# bindings.
for _call, _said in [(lambda: order(desc=True), "`order(desc=True)` names no column"),
                     (lambda: order(), "`order()` names no column")]:
    try:
        _call()
        raise AssertionError("order() with no column built")
    except GogError as refusal:
        assert _said in str(refusal) and "col.<category>" in str(refusal), refusal
ok("order() with no column is refused by name")


# --- A nest refuses layers that pack different rows --------------------------------
# A summed layer beside a plain one lays out two sets of regions, and the names
# landed in other groups' regions. The same block runs in all four bindings.
_pk2 = {"g": ["a", "a", "b", "c"], "v": [3.0, 1.0, 2.0, 1.0]}
try:
    render_svg(data(_pk2, name="pk2") + bar * sum + y(col.v) + color(col.g) + text
               + label(col.g) + nest())
    raise AssertionError("a summed layer beside a plain one drew")
except GogError as refusal:
    assert "`bar * sum` packs one region per group" in str(refusal), refusal
ok("a nest refuses layers that pack different rows")


# --- A size on a layout's degree draws its key -------------------------------------
# A layout makes `degree`, so the table does not hold it, and nodes sized by it drew
# no key. The same block runs in all four bindings.
_hub = {"a": ["Hub"] * 4, "b": ["p", "q", "r", "s"]}
assert ">Degree</text>" in render_svg(data(_hub, name="hub") + edge * layout(col.a, col.b)
                                      + point * layout(col.a, col.b) + size(col.degree) + network())
ok("a size on a layout's degree draws its key")


# --- A key takes its layer's set opacity -------------------------------------------
# Areas drawn at 1.0 were keyed at 0.82, paler than the bands they name. The same
# block runs in all four bindings.
_op = {"x": [1.0, 2.0, 1.0, 2.0], "y": [1.0, 2.0, 2.0, 3.0], "g": ["a", "a", "b", "b"]}
_svg = render_svg(data(_op, name="op") + area + x(col.x) + y(col.y) + color(col.g) + style(opacity=1))
assert 'fill-opacity="1.000"' in _svg.split(">G</text>")[1]
ok("a key takes its layer's set opacity")


# --- A histogram's end bars are drawn whole -----------------------------------------
# At three bins the first bar started at x = -78 and the panel cut 40% of it away.
# The same block runs in all four bindings.
_hv = {"v": [float(i % 17) * 1.3 for i in builtins.range(40)]}
_bars = re.findall(r'<rect x="([-0-9.]+)" y="[-0-9.]+" width="([0-9.]+)" height="[0-9.]+" fill="#4e79a7"',
                   render_svg(data(_hv, name="hv") + bar * bin(3) + x(col.v)))
assert len(_bars) == 3 and all(float(a) >= 0 and float(a) + float(w) <= 800 for a, w in _bars), _bars
ok("a histogram's end bars are drawn whole")


# --- A flow's count axis is ticked over its whole range ------------------------------
# Two stages labeled 500 / 1000 / 1500 and left out 0 and 2000. The same block runs
# in all four bindings.
_ft = {"a": ["p", "p", "q", "q"], "b": ["u", "v", "u", "v"], "n": [900.0, 500.0, 300.0, 500.0]}
_svg = render_svg(data(_ft, name="ft") + ribbon * flow(col.a, col.b) + y(col.n))
assert ">0</text>" in _svg and ">2K</text>" in _svg
ok("a flow's count axis is ticked over its whole range")


# --- Only a flat plot offers its axes to a page ---------------------------------------
# Two stacked map projections each label their own longitude, where the top one was
# read against the bottom one's. The same block runs in all four bindings.
_mp = {"lon": [-150.0, -20.0, 60.0, 150.0], "lat": [-40.0, 10.0, 30.0, 60.0]}
_svg = render_svg((data(_mp, name="mp") + point + x(col.lon) + y(col.lat) + map(preserve="area"))
                  / (data(_mp, name="mp") + point + x(col.lon) + y(col.lat) + map()))
assert _svg.count(">0\u00b0</text>") == 4, _svg.count(">0\u00b0</text>")
ok("only a flat plot offers its axes to a page")


# --- A map's meridians are the curves its projection makes ----------------------------
# Equal Earth bends them and Mercator keeps them straight; both were drawn straight.
# The same block runs in all four bindings.
_mt = {"lon": [176.0, 180.0, 186.0, 178.0], "lat": [-38.0, -20.0, -15.0, -30.0]}
_curve = re.compile(r'<polyline points="[^"]*" fill="none"/>')
assert _curve.search(render_svg(data(_mt, name="mt") + point + x(col.lon) + y(col.lat) + map()))
assert not _curve.search(render_svg(data(_mt, name="mt") + point + x(col.lon) + y(col.lat)
                                    + map(preserve="angle")))
ok("a map's meridians are the curves its projection makes")


# --- A globe's labels are clipped by the panel, not the disk --------------------------
# A name beside a place near the limb lost its last letters. The same block runs in
# all four bindings.
_gl = {"lon": [-150.0, 10.0], "lat": [61.0, 50.0], "name": ["Anchorage", "Frankfurt"]}
_svg = render_svg(data(_gl, name="gl") + point + x(col.lon) + y(col.lat) + text + label(col.name)
                  + globe())
# Frankfurt faces the default view; Anchorage is behind the sphere.
assert ">Frankfurt</text>" in _svg
_groups = re.findall(r'<g clip-path="url\(#[^)]*\)', _svg.split(">Frankfurt</text>")[0])
assert _groups and "-labels)" in _groups[-1], _groups[-1:]
ok("a globe's labels are clipped by the panel, not the disk")


# --- A cube's floor names every category ----------------------------------------------
# A name that met its neighbor was dropped at once, as a number is, and "Americas"
# went missing. The same block runs in all four bindings.
_conts = ["Asia", "Europe", "Africa", "Americas", "Oceania"]
_cf = {"life": [40.0 + i % 43 for i in builtins.range(60)],
       "continent": [_conts[i % 5] for i in builtins.range(60)]}
_svg = render_svg(data(_cf, name="cf") + bar * bin(12) + x(col.life) + y(col.continent) + space())
assert all(f">{k}</text>" in _svg for k in _conts), [k for k in _conts if f">{k}</text>" not in _svg]
ok("a cube's floor names every category")


# --- Zero is written 0 on a thousands axis ---------------------------------------------
# 0K and 0M read as a quantity where there is none. The same block runs in all four
# bindings.
_zk = render_svg(data({"g": ["a", "b", "c"], "v": [1200.0, 2500.0, 4100.0]}, name="zk")
                 + bar + x(col.g) + y(col.v))
assert ">0</text>" in _zk and ">0K</text>" not in _zk
ok("zero is written 0 on a thousands axis")


# --- An unmapped layer beside a color legend is neutral --------------------------------
# In the palette's first hue it read as the legend's first category. A texture
# legend's swatches take the ink their marks take. The same block runs in all four
# bindings.
_nt = {"x": [1.0, 2.0, 3.0, 4.0], "y": [1.0, 3.0, 2.0, 4.0], "g": ["a", "b", "a", "b"]}
_svg = render_svg(data(_nt, name="nt") + x(col.x) + y(col.y) + point + color(col.g) + line)
assert re.search(r'<polyline [^>]*stroke="#3c3c46"', _svg)
_svg = render_svg(data(_nt, name="nt") + x(col.x) + y(col.y) + point + line)
assert re.search(r'<polyline [^>]*stroke="#4e79a7"', _svg)
_nb = {"a": ["p", "q", "r"], "b": [3.0, 5.0, 4.0], "g": ["u", "v", "w"]}
_svg = render_svg(data(_nb, name="nb") + bar + x(col.a) + y(col.b) + pattern(col.g)
                  + style(color="firebrick"))
assert "#4e79a7" not in _svg
ok("an unmapped layer beside a color legend is neutral")


# --- A polar step draws its closing jump ------------------------------------------------
# On a wrapped angle the last category is adjacent to the first, so the value
# changes at the first spoke as it does at every other. The same block runs in all
# four bindings.
_svg = render_svg(data({"g": ["a", "b", "c", "d"], "v": [1.0, 4.0, 2.0, 3.0]}, name="ps")
                  + step + x(col.g) + y(col.v) + polar())
assert 'Z" stroke=' in _svg
ok("a polar step draws its closing jump")


# --- A stated angle domain is the whole turn -------------------------------------------
# The bins are cut on it and the axis is not widened past it, so 0 is at the top.
# The same block runs in all four bindings.
_svg = render_svg(data({"deg": [2.6 + i * 9.2 for i in builtins.range(40)]}, name="sd")
                  + bar * bin + x(col.deg, limits=(0, 360)) + polar())
assert 'text-anchor="middle">0</text>' in _svg
ok("a stated angle domain is the whole turn")


# --- A written value outside a stated domain is left out -------------------------------
# A count of 3 has no place on `y(count, limits = (0, 2))`, where it was drawn cut
# off at the top. The same block runs in all four bindings.
_svg = render_svg(data({"g": ["a", "a", "a", "b"]}, name="wo") + bar * count + x(col.g)
                  + y(col.count, limits=(0, 2)))
assert len(re.findall(r'<rect[^>]*fill="#4e79a7"', _svg)) == 1
ok("a written value outside a stated domain is left out")


# --- A banded crater leaves its middle to the bands below ------------------------------
# Each level is one region with its rings under even-odd, so a crater's middle is not
# painted as its highest band. The same block runs in all four bindings.
_r = [3 + 0.25 * math.sin(1.3 * i) for i in builtins.range(300)]
_ring = {"a": [_r[i] * math.cos(0.37 * i) for i in builtins.range(300)],
         "b": [_r[i] * math.sin(0.37 * i) for i in builtins.range(300)]}
_svg = render_svg(data(_ring, name="ring") + zone * density(levels=6) + x(col.a) + y(col.b))
assert 'fill-rule="evenodd"' in _svg and "<polygon" not in _svg
ok("a banded crater leaves its middle to the bands below")


# --- A zone that reads its positions checks their columns ------------------------------
# `zone * density` with a misspelled `y` drew an empty panel in silence, where `point`
# refuses it. The same block runs in all four bindings.
_zt = {"a": [1.0, 2.0, 3.0, 4.0, 5.0, 6.0], "b": [2.0, 1.0, 4.0, 3.0, 6.0, 5.0]}
try:
    render_svg(data(_zt, name="zt") + zone * density + x(col.a) + y(col.bb))
    raise AssertionError("a zone * density with a missing column drew")
except GogError as refusal:
    assert "`y(bb)` refers to a column that is not in the data" in str(refusal), refusal
ok("a zone that reads its positions checks their columns")


# --- A plot that gives up its y axis keeps no margin under a shared x ------------------
# In `top / (left | right)`, `right` gives its y axis to `left` and shares its x with
# `top`, and it kept a blank strip where the axis would have been. The same block runs
# in all four bindings.
_gt = {"u": [1.0, 2.0, 3.0, 4.0, 5.0, 6.0], "v": [0.0012, 0.0031, 0.0054, 0.0087, 0.0102, 0.014],
       "w": [3.0, 1.0, 4.0, 1.0, 5.0, 9.0]}
_left = data(_gt, name="gt") + point + x(col.w) + y(col.v)
_right = data(_gt, name="gt") + point + x(col.u) + y(col.v)
_top = data(_gt, name="gt") + bar * bin + x(col.u)
def _last_panel(svg):
    return re.search(r'<clipPath[^>]*><rect x="([^"]*)"', svg.split("<svg ")[-1]).group(1)
assert _last_panel(render_svg(_top / (_left | _right))) == _last_panel(render_svg(_left | _right))
ok("a plot that gives up its y axis keeps no margin under a shared x")


# --- The title follows a panel a ratio shortened ----------------------------------------
# Left at the top of the rectangle, it stood far over a map set beside a globe. The
# same block runs in all four bindings.
_svg = render_svg(data({"u": [1.0, 2.0, 3.0, 4.0], "v": [1.0, 2.0, 3.0, 4.0]}, name="tw")
                  + point + x(col.u) + y(col.v) + title("Wide") + theme(ratio=3))
assert float(re.search(r'<text x="[^"]*" y="([^"]*)"[^>]*font-weight="600"', _svg).group(1)) > 100
ok("the title follows a panel a ratio shortened")


# --- An area refused in the cube gets its own direction ---------------------------------
# It said "A `area`" and called `path` "`area` with that sort removed", which is
# `line`'s sentence. The same block runs in all four bindings.
try:
    render_svg(data({"a": [1.0, 2.0, 3.0, 4.0], "b": [2.0, 1.0, 4.0, 3.0], "c": [5.0, 6.0, 7.0, 8.0]},
                    name="az") + x(col.a) + y(col.b) + area + z(col.c))
    raise AssertionError("an area in the cube drew")
except GogError as refusal:
    assert "An `area` in space" in str(refusal) and "draw the area's edge with `path`" in str(refusal), refusal
ok("an area refused in the cube gets its own direction")


# --- save_svg writes the drawing byte for byte ------------------------------------------
# It returns its path, refuses a path not ending in `.svg` toward the same path with
# that ending, and draws before it writes, so a refused plot leaves the file there as
# it was. The same block runs in all four bindings.
_ss = {"a": [1.0, 2.0], "b": [3.0, 4.0]}
_sp = data(_ss, name="ss") + point + x(col.a) + y(col.b)
_spath = os.path.join(tempfile.gettempdir(), "gog-save-svg.svg")
assert save_svg(_sp, _spath) == _spath
with open(_spath, "rb") as _f:
    _sbytes = _f.read()
assert _sbytes == render_svg(_sp).encode("utf-8")
try:
    save_svg(_sp, os.path.join(tempfile.gettempdir(), "life.png"))
    raise AssertionError("a .png path was written")
except GogError as refusal:
    assert 'life.svg")`' in str(refusal), refusal
try:
    save_svg(data(_ss, name="ss") + point + x(col.a) + y(col.missing_col), _spath)
except GogError:
    pass
with open(_spath, "rb") as _f:
    assert _f.read() == _sbytes, "a refused plot changed the file"
os.remove(_spath)
ok("save_svg writes the drawing byte for byte and refuses a wrong ending")


# --- A zone's transform refusal says why and lists what a zone takes -------------------
# It was one fixed sentence that called `bounds` refused, though `zone * bounds`
# draws. The same block runs in all four bindings.
try:
    render_svg(data({"a": [1.0, 2.0, 3.0, 4.0], "b": [2.0, 1.0, 4.0, 3.0]}, name="zr")
               + zone * smooth + x(col.a) + y(col.b))
    raise AssertionError("a zone * smooth drew")
except GogError as refusal:
    assert "`smooth` fits a curve along a domain" in str(refusal) and "A zone takes `bin`" in str(refusal), refusal
ok("a zone's transform refusal says why and lists what a zone takes")


# --- The circular-tree refusal names the tree's horizontal bars -------------------------
# Not "treads", a word the book does not use. The same block runs in all four
# bindings.
_ct = {"g": ["a", "a", "b", "b", "c", "c"], "k": ["u", "v", "u", "v", "u", "v"],
       "v": [1.0, 2.0, 2.0, 1.0, 3.0, 3.0]}
try:
    render_svg(data(_ct, name="ct") + path * cluster(col.v, over=col.k) + x(col.g) + polar())
    raise AssertionError("a circular cluster tree drew")
except GogError as refusal:
    assert "the tree's horizontal bars" in str(refusal) and "treads" not in str(refusal), refusal
ok("the circular-tree refusal names the tree's horizontal bars")


# --- A clustered tile plot with one position says it needs a category on each ---------
# It refused the empty leaf axis as "``", a name the table does not have. The same
# block runs in all four bindings.
try:
    render_svg(data({"g": ["a", "a", "b", "b"], "k": ["u", "v", "u", "v"], "v": [1.0, 2.0, 3.0, 4.0]},
                    name="tc") + zone * cluster(over=col.g) + x(col.g) + color(col.v))
    raise AssertionError("a one-position clustered tile plot drew")
except GogError as refusal:
    assert "binds `x` but not `y`" in str(refusal) and "``" not in str(refusal), refusal
ok("a clustered tile plot with one position says it needs a category on each")


# --- A refused transform is not followed by a missing y ---------------------------------
# `line * cluster` also said "Add `y(<column>)`", and adding one reached only another
# refusal. The same block runs in all four bindings.
try:
    render_svg(data({"g": ["a", "a", "b", "b"], "k": ["u", "v", "u", "v"], "v": [1.0, 2.0, 3.0, 4.0]},
                    name="rt") + line * cluster(col.v, over=col.k) + x(col.g))
    raise AssertionError("line * cluster drew")
except GogError as refusal:
    assert "`cluster` joins the closest leaves" in str(refusal) and "but none is set" not in str(refusal), refusal
ok("a refused transform is not followed by a missing y")


# --- A layer the graph places is refused once outside the network -----------------------
# `edge + polar()` also said "Drop `polar()` to draw it flat", and a flat `edge` is
# refused too; `point * layout` in `map()` was also told to add `x`. The same block runs
# in all four bindings.
_net = data({"a": ["u", "v", "w"], "b": ["v", "w", "u"]}, name="t")
for _p in (_net + edge + polar(), _net + edge + map(), _net + point * layout(col.a, col.b) + map()):
    try:
        render_svg(_p)
        raise AssertionError("a layer the graph places drew outside the network")
    except GogError as refusal:
        _s = str(refusal)
        assert "network()" in _s and "Drop `" not in _s and "but none is set" not in _s, _s
ok("a layer the graph places is refused once outside the network")


# --- A bar over two categories is sent to the cube, not to count ------------------------
# `bar * count + x(a) + y(b)` was told there was nothing to measure and to use
# `bar * count`; its own refusal now offers `space()`. A floor summary with no `z` is
# asked for `z`, not told to count, and is not also told it "is drawn flat". The same
# block runs in all four bindings.
_floor = data({"g": ["a", "a", "b", "b"], "k": ["u", "v", "u", "v"], "v": [1.0, 2.0, 3.0, 4.0]},
              name="t")
try:
    render_svg(_floor + bar * count + x(col.g) + y(col.k))
    raise AssertionError("bar * count over two categories drew")
except GogError as refusal:
    _s = str(refusal)
    assert "`+ space()` stands one bar on each pair" in _s and "nothing for it to measure" not in _s, _s
try:
    render_svg(_floor + bar * mean + x(col.g) + y(col.k) + space())
    raise AssertionError("a floor summary with no z drew")
except GogError as refusal:
    _s = str(refusal)
    assert "add `z(<column>)`" in _s and "drawn flat" not in _s and "bar * count" not in _s, _s
ok("a bar over two categories is sent to the cube, not to count")


# --- Asking for a declared order names all four languages -------------------------------
# "Set the column's factor levels" was R's word, printed to all four. The `order()`
# refusal and the `play()` note now name a factor in R and `ordered()` in the other
# three. The same block runs in all four bindings.
_lv = data({"g": ["b", "a", "c"], "v": [1.0, 2.0, 3.0], "w": [4.0, 6.0, 5.0]}, name="t")
_four = "a factor in R, `ordered()` in Python, Julia and JavaScript"
try:
    render_svg(_lv + point + x(col.v) + y(col.w) + order(col.v))
    raise AssertionError("order() with no categorical axis drew")
except GogError as refusal:
    _sorted = str(refusal)
with contextlib.redirect_stderr(io.StringIO()) as _said:
    render_svg(_lv + point + x(col.v) + y(col.w) + play(col.g))
for _s in (_sorted, _said.getvalue()):
    assert _four in _s and "factor levels" not in _s, _s
ok("asking for a declared order names all four languages")


# --- A flat zone with group() names every transform that gives it sides -----------------
# The list named `bounds`, `bin` and `density` and left out `count`, `proportion`,
# `partition` and `flow`. The same block runs in all four bindings.
try:
    render_svg(data({"g": ["a", "a", "b"], "v": [1.0, 2.0, 3.0], "w": [4.0, 6.0, 5.0]}, name="t")
               + zone * bin + x(col.v) + y(col.w) + group(col.g))
    raise AssertionError("a flat zone with group() drew")
except GogError as refusal:
    assert "`bin`, `count`, `density`, `proportion`, `bounds`, `partition` and `flow`" in str(refusal), refusal
ok("a flat zone with group() names every transform that gives it sides")


# --- A log z is refused by the cube only in the cube ------------------------------------
# On a globe the cube's refusal printed first, above the globe's own refusal of a `z`
# scale. The same block runs in all four bindings.
_lz = data({"lon": [1.0, 2.0, 3.0], "lat": [4.0, 5.0, 6.0], "v": [1.0, 10.0, 100.0]}, name="t")
_said = {}
for _k, _p in (("cube", _lz + point + x(col.lon) + y(col.lat) + z(col.v, scale="log")),
               ("globe", _lz + bar + x(col.lon) + y(col.lat) + z(col.v, scale="log") + globe())):
    try:
        render_svg(_p)
        raise AssertionError(f"a log z drew in the {_k}")
    except GogError as refusal:
        _said[_k] = str(refusal)
assert "log `z`-axis" in _said["cube"], _said["cube"]
assert "log `z`-axis" not in _said["globe"] and "a `globe()` plot draws none" in _said["globe"], _said["globe"]
ok("a log z is refused by the cube only in the cube")


# --- A globe's hidden rows are said once per table, and name it -------------------------
# Two tables with the same counts printed one unnamed line. The same block runs in all
# four bindings.
_places = {"lon": [-150.0, 10.0, 170.0], "lat": [61.0, 50.0, -20.0]}
with contextlib.redirect_stderr(io.StringIO()) as _said:
    render_svg(data(_places, name="cities") + point + x(col.lon) + y(col.lat)
               + data(_places, name="copy") + point + x(col.lon) + y(col.lat) + globe())
assert "row(s) of `cities` face away" in _said.getvalue() and \
    "row(s) of `copy` face away" in _said.getvalue(), _said.getvalue()
ok("a globe's hidden rows are said once per table, and name it")


# --- The globe and map refusals use the globe chapter's words ---------------------------
# A span mark on a map is refused by the map alone, not first told to add a range
# transform the map refuses too. On a globe: a sphere has no straight side for an axis
# label, a `z` with no spike keeps the marks drawn on the sphere, and `turn` is a
# longitude. The same block runs in all four bindings.
_g = data({"lon": [1.0, 2.0, 3.0], "lat": [4.0, 5.0, 6.0], "v": [1.0, 2.0, 3.0]}, name="t")
def _refusal(p):
    try:
        render_svg(p)
    except GogError as refusal:
        return str(refusal)
    raise AssertionError("drew")
_spanned = _refusal(_g + interval + x(col.lon) + y(col.lat) + map())
assert "produces those extents" not in _spanned and "map()" in _spanned, _spanned
assert "no straight side to write one along" in _refusal(
    _g + point + x(col.lon) + y(col.lat) + x_label("Longitude") + globe())
assert "to keep the marks drawn on the sphere" in _refusal(
    _g + point + x(col.lon) + y(col.lat) + z(col.v) + globe())
assert "a longitude wraps and a latitude does not" in _refusal(
    _g + point + x(col.lon) + y(col.lat) + globe(tilt=100))
ok("the globe and map refusals use the globe chapter's words")


# --- A network's refusals are said once and can be followed -----------------------------
# The self-loop refusal printed once per layer and offered to carry the fact as a
# node's own column, which a node does not have; a brush was first told to use
# `group()` on an edge, which an edge refuses. The same block runs in all four bindings.
_loop = data({"a": ["u", "v", "w"], "b": ["v", "v", "u"]}, name="t")
try:
    render_svg(_loop + edge * layout(col.a, col.b) + point * layout(col.a, col.b)
               + text * layout(col.a, col.b) + label(col.name) + network())
    raise AssertionError("a self-loop drew")
except GogError as refusal:
    _s = str(refusal)
    assert _s.count("connect a node to itself") == 1 and "row(s) of `t` connect" in _s \
        and "own column" not in _s, _s
_ring = data({"a": ["u", "v", "w"], "b": ["v", "w", "u"]}, name="t")
try:
    render_svg(_ring + edge * layout(col.a, col.b) + point * layout(col.a, col.b) + brush + network())
    raise AssertionError("a brush on a network drew")
except GogError as refusal:
    _s = str(refusal)
    assert "a network's positions are the layout's" in _s and "group()" not in _s \
        and "Give it an `x()`" not in _s, _s
ok("a network's refusals are said once and can be followed")


# --- A line faceted by its own x is refused once ----------------------------------------
# On a number column the facet's type refusal followed, saying to make the column text,
# and its advice led straight back to the first refusal. The same block runs in all four
# bindings.
try:
    render_svg(data({"year": [2000.0, 2001.0, 2000.0, 2001.0], "v": [1.0, 2.0, 3.0, 4.0]}, name="t")
               + line + x(col.year) + y(col.v) | facet(col.year))
    raise AssertionError("a line faceted by its own x drew")
except GogError as refusal:
    _s = str(refusal)
    assert "both cuts the plot into panels and supplies `x`" in _s \
        and "splits on a number column" not in _s, _s
ok("a line faceted by its own x is refused once")


# --- A ribbon with z hears only the cube ------------------------------------------------
# It was first told to add a range transform, and `ribbon * range` with `z` was then
# refused by the cube alone. The same block runs in all four bindings.
try:
    render_svg(data({"east": [1.0, 2.0, 3.0], "north": [2.0, 3.0, 4.0], "altitude": [10.0, 20.0, 30.0]},
                    name="t") + ribbon + x(col.east) + y(col.north) + z(col.altitude))
    raise AssertionError("a ribbon with z drew")
except GogError as refusal:
    assert "a cube has no left to right" in str(refusal) \
        and "produces those extents" not in str(refusal), refusal
ok("a ribbon with z hears only the cube")


# --- A type refusal offers only the channels the mark takes -----------------------------
# `size(<category>)` on a point offered `pattern`, which a point refuses, and
# `bar + color(<number>)` was given no direction, though `opacity` draws it. The same
# block runs in all four bindings.
_ty = data({"g": ["a", "b", "c"], "v": [1.0, 2.0, 3.0], "w": [3.0, 5.0, 4.0]}, name="t")
def _typed(p):
    try:
        render_svg(p)
    except GogError as refusal:
        return str(refusal)
    raise AssertionError("drew")
_sized = _typed(_ty + point + x(col.v) + y(col.w) + size(col.g))
assert "Use `color` or `shape` to distinguish categories" in _sized and "pattern" not in _sized, _sized
assert "Use `opacity` to show a numeric column" in _typed(_ty + bar + x(col.g) + y(col.v) + color(col.w))
ok("a type refusal offers only the channels the mark takes")


# --- A missing feature is refused toward the marks that have it -------------------------
# The mapping refusal and its `style()` sibling said "use a mark that has one" and named
# none. The same block runs in all four bindings.
_ft = data({"g": ["a", "b", "c"], "v": [1.0, 2.0, 3.0], "w": [3.0, 5.0, 4.0]}, name="t")
def _feature(p):
    try:
        render_svg(p)
    except GogError as refusal:
        return str(refusal)
    raise AssertionError("drew")
assert "use a mark that maps it: `line`, `area`" in _feature(_ft + point + x(col.v) + y(col.w) + group(col.g))
assert "use a mark that has one: `point`, `line`" in _feature(_ft + bar + x(col.g) + y(col.v) + style(size=3))
# An edge's positions come from the layout, and the network says so alone.
assert "cannot be bound to `edge`" not in _feature(_ft + edge + x(col.v) + y(col.w) + network())
ok("a missing feature is refused toward the marks that have it")


# --- line * dodge is not sent to stack --------------------------------------------------
# A line refuses `stack` too. It is told that lines cross rather than cover, and that
# `area * stack` piles the groups. The same block runs in all four bindings.
try:
    render_svg(data({"g": ["a", "a", "b", "b"], "yr": [1.0, 2.0, 1.0, 2.0], "v": [1.0, 2.0, 3.0, 4.0]},
                    name="t") + line * dodge + x(col.yr) + y(col.v) + color(col.g))
    raise AssertionError("line * dodge drew")
except GogError as refusal:
    _s = str(refusal)
    assert "cross rather than cover each other" in _s and "`area * stack`" in _s and "(`stack`)" not in _s, _s
ok("line * dodge is not sent to stack")


# --- A transform written twice is told so once ------------------------------------------
# `mean * mean` was offered "`bar * mean` or `bar * mean`", and `proportion * proportion`
# was told about `stack(share = TRUE)`, which it never wrote. The same block runs in all
# four bindings.
_tw = data({"g": ["a", "a", "b", "b"], "v": [1.0, 2.0, 3.0, 4.0]}, name="t")
def _twice(p):
    try:
        render_svg(p)
    except GogError as refusal:
        return str(refusal)
    raise AssertionError("drew")
_meaned = _twice(_tw + bar * mean * mean + x(col.g) + y(col.v))
_shared = _twice(_tw + bar * proportion * proportion + x(col.g))
assert "names `mean` twice" in _meaned and "or `bar * mean`" not in _meaned, _meaned
assert "names `proportion` twice" in _shared and "stack(share = TRUE)" not in _shared, _shared
ok("a transform written twice is told so once")


# --- A flow in polar is refused once for every layer ------------------------------------
# It said "the bands bent round a rim" for a zone and a text too, once per layer. The
# same block runs in all four bindings.
try:
    render_svg(data({"a": ["u", "u", "v"], "b": ["p", "q", "q"]}, name="t")
               + ribbon * flow(col.a, col.b) + zone * flow(col.a, col.b) + polar())
    raise AssertionError("a flow in polar drew")
except GogError as refusal:
    _s = str(refusal)
    assert _s.count("chord diagram") == 1 and "a flow bent round a rim" in _s and "bands" not in _s, _s
ok("a flow in polar is refused once for every layer")


# --- A path is sent to line only with a statistic ---------------------------------------
# `path * flow` was also told "Use `line * flow`", which a line refuses too; `path * count`
# was also told to write `path * count + x() + y()`, refused in turn. The same block runs
# in all four bindings.
_pa = data({"a": ["u", "u", "v"], "b": ["p", "q", "q"], "v": [1.0, 2.0, 3.0], "w": [2.0, 3.0, 4.0]},
           name="t")
def _pathed(p):
    try:
        render_svg(p)
    except GogError as refusal:
        return str(refusal)
    raise AssertionError("drew")
_flowed = _pathed(_pa + path * flow(col.a, col.b))
_counted = _pathed(_pa + path * count + x(col.v) + y(col.w))
assert "Use `line * flow`" not in _flowed and "`flow` lays" in _flowed, _flowed
assert "Use `line * count`" in _counted and "contours" not in _counted, _counted
ok("a path is sent to line only with a statistic")


# --- A transform that refuses a mark itself is refused once -----------------------------
# `surface * bounds` printed two refusals of `bounds`, and a text was told that `bounds`
# "replaces those rows with one summary per key". The same block runs in all four
# bindings.
_bd = data({"a": ["u", "v", "w"], "v": [1.0, 2.0, 3.0], "w": [2.0, 3.0, 4.0], "h": [5.0, 6.0, 7.0]},
           name="t")
def _bounded(p):
    try:
        render_svg(p)
    except GogError as refusal:
        return str(refusal)
    raise AssertionError("drew")
_surfaced = _bounded(_bd + surface * bounds(col.v, col.w) + x(col.v) + y(col.w) + z(col.h))
assert "`bounds` supplies" in _surfaced and "gives each row two edges" not in _surfaced, _surfaced
assert "replaces those rows" not in _bounded(_bd + text * bounds(col.v, col.w) + x(col.a) + label(col.a))
ok("a transform that refuses a mark itself is refused once")


# --- A page refusal names the atom it was given -----------------------------------------
# Every atom was shown with `title('...')`, and a mark as `mark()`. A plot added to a page
# is told to place it. The same block runs in all four bindings.
_pt = data({"a": [1.0, 2.0], "b": [3.0, 4.0], "g": ["u", "v"]}, name="t")
_pp = _pt + point + x(col.a) + y(col.b)
def _paged(rhs):
    try:
        (_pp | _pp) + rhs
    except GogError as refusal:
        return str(refusal)
    raise AssertionError("a page took it")
_colored = _paged(color(col.g))
assert "`color()` belongs to a plot" in _colored and "`(plot + color(...)) | other_plot`" in _colored, _colored
assert "`point` belongs to a plot" in _paged(point)
assert "`page | other_plot`" in _paged(_pt)
ok("a page refusal names the atom it was given")


# --- A page written without parentheses is told to add them -----------------------------
# `/` binds before `+`, so it joined the atom written just before it to the next plot,
# and the refusal sent the reader to facet a plot. It names the atom as it is written
# (`y()`, not the `coord_y()` it is held as). The same block runs in all four bindings.
_st = data({"a": [1.0, 2.0], "b": [3.0, 4.0]}, name="t")
_sp = _st + point + x(col.a) + y(col.b)
try:
    _st + point + x(col.a) + y(col.b) / _st + bar * count + x(col.a)
    raise AssertionError("an unparenthesized page was taken")
except GogError as refusal:
    _s = str(refusal)
    assert "places one plot below another, and it binds before `+`" in _s and "joined `y()`" in _s \
        and "facets a *plot*" not in _s, _s
try:
    (_sp | _sp) + x(col.a)
    raise AssertionError("a page took x()")
except GogError as refusal:
    assert "`x()` belongs to a plot" in str(refusal), refusal
ok("a page written without parentheses is told to add them")


# --- An atom written after a facet is told to come before it ----------------------------
# `+` binds before `|`, so the atom joined the facet, and it was told there was "no plot
# to join". The same block runs in all four bindings; Julia's `|` binds as `+` does, so
# there the sentence draws.
_ft2 = data({"a": [1.0, 2.0], "b": [3.0, 4.0], "g": ["u", "v"]}, name="t")
try:
    _ft2 + point + x(col.a) + y(col.b) | facet(col.g) + title("t")
    raise AssertionError("an atom after a facet was taken")
except GogError as refusal:
    _s = str(refusal)
    assert "`title()` was added to `facet(col.g)`" in _s \
        and "`plot + title('...') | facet(col.g)`" in _s and "no plot to join" not in _s, _s
ok("an atom written after a facet is told to come before it")
