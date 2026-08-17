"""Basic sanity test for the Python binding — plain Python, no test runner.

Run from the project root:

    python3 py-pkg/gog/tests/test_basic.py

The mirror of `r-pkg/gog/tests/test_basic.R`: does a sentence reach the engine,
does the engine draw it, and do the refusals refuse. It loads the package from
source (the binding is not installed anywhere yet) and finds `gog-cli` the way
a user's first plot would.
"""

import builtins
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
    ok("`save_gif()` refuses a plot with no moments, with direction")

    # The name says what the file is, so a path that says otherwise is refused
    # rather than quietly corrected.
    try:
        save_gif(played, os.path.join(folder, "wave.png"))
        raise AssertionError("save_gif() should refuse a path that is not a .gif")
    except GogError as e:
        assert "ends in `.gif`" in str(e), str(e)
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
# A label pushed clear of its dot keeps a line back to it.
assert 'stroke-width="0.7"' in moved_svg, "a travelled label should keep its leader"
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
    assert "one shape through many rows" in _t and "group()" in _t, _t
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
