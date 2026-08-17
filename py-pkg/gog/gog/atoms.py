# atoms.py — the vocabulary: marks, transforms, channels, settings
#
# The mirror of `r-pkg/gog/R/atoms.R`, and the words are the same words: the
# grammar is the engine's, not the binding's, so anything that differs here is
# a bug in one of the two front ends. What each atom *means* is documented once,
# in the book and the spec; this file only says how Python spells it.
#
# Two spellings do differ from R, and both are Python's doing:
#
#   * a column is `col.gdp`, never a bare name (see `columns.py`);
#   * an atom that takes a parameter — `bin`, `density`, `confidence`,
#     `jitter`, `box`, `space`, `polar` — is an *object that is also callable*,
#     so `bar * bin` and `bar * bin(30)` reach the same code path exactly as
#     they do in R, where `*` calls an uncalled transform with its defaults.
#
# The value checks live here rather than in Rust for the reason R's do: the
# caller gets the error at the line that wrote it, and a misspelling never
# reaches the wire as an enum serde cannot decode. What is *legal* — which mark
# takes which channel, whether this transform means anything on that mark —
# stays in `legality.rs`, where every binding inherits it.

from datetime import date, datetime
from typing import Any, Dict, List, Optional, Sequence, Union

from .columns import Column, column_name
from .errors import GogError
from .render import _epoch_seconds
from .spec import Atom, CallableAtom

# ---------------------------------------------------------------------------
# Marks — the geometric forms
# ---------------------------------------------------------------------------

point = Atom("mark", mark="point")
line = Atom("mark", mark="line")
path = Atom("mark", mark="path")
rule = Atom("mark", mark="rule")
zone = Atom("mark", mark="zone")
area = Atom("mark", mark="area")
bar = Atom("mark", mark="bar")
step = Atom("mark", mark="step")
interval = Atom("mark", mark="interval")
ribbon = Atom("mark", mark="ribbon")
text = Atom("mark", mark="text")
# A sheet through the samples, and the one mark that draws in the cube alone. Its
# rows are nodes: the grid the two position columns describe is recovered rather
# than declared, so it wants one row per (x, y) crossing. Three positions, all
# required and all numeric — a face asserts every value *between* two nodes, and
# between two categories there is nothing to assert (for a mesh over categories,
# `bar * bin + space()`). One transform, `density`, which makes it the third
# geometry of one field: `zone * density` paints it as cells, `path * density`
# traces its contours, `surface * density` raises it with the estimate as height.
surface = Atom("mark", mark="surface")
# The stroke whose two endpoints the layout supplies — one row is one edge of a
# graph. Nothing binds to its positions; its minimum syllable is
# `edge * layout(col.a, col.b)` inside `network()`, and like `surface` it does
# not draw flat.
edge = Atom("mark", mark="edge")


class _Box(CallableAtom):
    """`box` — the box-and-whisker mark, with its one knob."""

    __slots__ = ()

    def __call__(self, whiskers: Optional[str] = None) -> Atom:
        if whiskers is not None and whiskers not in ("tukey", "range"):
            raise GogError(
                'gog: `box(whiskers=)` is either "tukey" (the default — whiskers to '
                '1.5*IQR, points beyond drawn as outliers) or "range" (whiskers to the '
                "true min and max, no outliers)."
            )
        atom = Atom("mark", mark="box")
        if whiskers is not None:
            atom.fields["box"] = {"whiskers": whiskers}
        return atom


box = _Box("mark", mark="box")

# ---------------------------------------------------------------------------
# Transforms — used with `*`:  bar * bin,  line * smooth
# ---------------------------------------------------------------------------

smooth = Atom("transform", transform="smooth")
count = Atom("transform", transform="count")
sum = Atom("transform", transform="sum")
mean = Atom("transform", transform="mean")
median = Atom("transform", transform="median")
max = Atom("transform", transform="max")
min = Atom("transform", transform="min")
proportion = Atom("transform", transform="proportion")
dodge = Atom("transform", transform="dodge")
# The fourth collision modifier, and the one whose collision is ink: `text * repel`
# moves labels that overlap each other, where `dodge`/`stack`/`jitter` move marks
# that share a position. Bare like `dodge`, and for the same reason — a label moves
# as far as the overlap requires and no further, so there is no free spread to knob.
repel = Atom("transform", transform="repel")


def _whole_number(value: Any, atom: str, argument: str, example: str) -> int:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise GogError(
            f"gog: `{atom}({argument}=)` needs one positive whole number, e.g. `{example}`."
        )
    if value < 1 or value != int(value):
        raise GogError(
            f"gog: `{atom}({argument}=)` needs one positive whole number, e.g. `{example}`."
        )
    return int(value)


def _one_word(value: Any, argument: str) -> str:
    """A named reading, checked for *shape* only — which words exist is the
    engine's question, so every binding forwards the string and one refusal
    message covers all four."""
    if not isinstance(value, str):
        raise GogError(
            f'gog: `density({argument}=)` takes one word — "shape" or "count".'
        )
    return value


def _positive(value: Any, atom: str, argument: str, example: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or value <= 0:
        raise GogError(
            f"gog: `{atom}({argument}=)` needs one positive number, e.g. `{example}`."
        )
    return float(value)


class _Bin(CallableAtom):
    """`bin` — equal-width buckets. How many dimensions it cuts is the mark's answer."""

    __slots__ = ()

    def __call__(
        self,
        bins: Optional[int] = None,
        width: Optional[float] = None,
        tiling: Optional[str] = None,
    ) -> Atom:
        if bins is not None and width is not None:
            raise GogError(
                "gog: `bin()` takes either `bins` or `width`, not both. Write `bin(30)` "
                "for a bin count or `bin(width=5)` for a bin width."
            )
        if tiling is not None and not isinstance(tiling, str):
            raise GogError('gog: `bin(tiling=)` needs one name, `"rect"` or `"hex"`.')
        return Atom(
            "transform",
            transform="bin",
            bins=None if bins is None else _whole_number(bins, "bin", "bins", "bin(30)"),
            width=None if width is None else _positive(width, "bin", "width", "bin(width=5)"),
            tiling=tiling,
        )


bin = _Bin("transform", transform="bin")


class _Density(CallableAtom):
    """`density` — the smooth estimate; `levels` cuts a field into bands;
    `compare` says what a violin's width means from one slot to the next."""

    __slots__ = ()

    def __call__(
        self,
        adjust: Optional[float] = None,
        bandwidth: Optional[float] = None,
        levels: Optional[int] = None,
        compare: Optional[str] = None,
        reach: Optional[float] = None,
    ) -> Atom:
        if adjust is not None and bandwidth is not None:
            raise GogError(
                "gog: `density()` takes either `adjust` or `bandwidth`, not both. Write "
                "`density(2)` to scale the automatic bandwidth, or `density(bandwidth=5)` "
                "to set it in the data's own units."
            )
        return Atom(
            "transform",
            transform="density",
            adjust=None if adjust is None else _positive(adjust, "density", "adjust", "density(2)"),
            bandwidth=None
            if bandwidth is None
            else _positive(bandwidth, "density", "bandwidth", "density(bandwidth=5)"),
            levels=None
            if levels is None
            else _whole_number(levels, "density", "levels", "path * density(levels=8)"),
            # One of two words, checked here only for *shape* — the word itself is
            # the engine's question, so a typo gets one message in all four bindings
            # rather than four (`legality::check_density_params`).
            compare=None if compare is None else _one_word(compare, "compare"),
            reach=None
            if reach is None
            else _positive(reach, "density", "reach", "density(reach=2.5)"),
        )


density = _Density("transform", transform="density")


class _Confidence(CallableAtom):
    """`confidence` — the mean's interval per group, 0.95 unless told otherwise."""

    __slots__ = ()

    def __call__(self, level: Optional[float] = None) -> Atom:
        if level is not None and (
            isinstance(level, bool)
            or not isinstance(level, (int, float))
            or not 0 < level < 1
        ):
            raise GogError(
                "gog: `confidence(level=)` needs one number strictly between 0 and 1, "
                "e.g. `confidence(0.95)`."
            )
        return Atom(
            "transform",
            transform="confidence",
            level=None if level is None else float(level),
        )


confidence = _Confidence("transform", transform="confidence")


class _Deviation(CallableAtom):
    """`deviation` — the spread band per group, one standard deviation unless told."""

    __slots__ = ()

    def __call__(self, multiplier: Optional[float] = None) -> Atom:
        if multiplier is not None and (
            isinstance(multiplier, bool)
            or not isinstance(multiplier, (int, float))
            or multiplier <= 0
        ):
            raise GogError(
                "gog: `deviation(multiplier=)` needs one positive number — it counts "
                "standard deviations out from the mean. `deviation` is one, "
                "`deviation(2)` is two."
            )
        return Atom(
            "transform",
            transform="deviation",
            multiplier=None if multiplier is None else float(multiplier),
        )


deviation = _Deviation("transform", transform="deviation")


class _Quantile(CallableAtom):
    """`quantile` — the value at one probability. No default; 0.5 is `median`."""

    __slots__ = ()

    def __call__(self, p: Optional[float] = None) -> Atom:
        if p is not None:
            if isinstance(p, bool) or not isinstance(p, (int, float)):
                raise GogError(
                    "gog: `quantile()` takes one number between 0 and 1, the "
                    "probability it reduces to, e.g. `quantile(0.9)`."
                )
            if not 0 <= p <= 1:
                raise GogError(
                    f"gog: `quantile({p})` is not a probability — a quantile is "
                    "between 0 and 1. `quantile(0.9)` is the 90th percentile, "
                    "`quantile(0.5)` the middle."
                )
        return Atom("transform", transform="quantile", p=None if p is None else float(p))


quantile = _Quantile("transform", transform="quantile")


class _Range(CallableAtom):
    """`range` — the band per group, the whole group unless told otherwise."""

    __slots__ = ()

    def __call__(
        self, low: Optional[float] = None, high: Optional[float] = None
    ) -> Atom:
        # Two different mistakes reach here and they want different directions,
        # so the message splits on the shape Python's own builtin is called
        # with. `range(10)` is a whole number out of 0..1, which is nobody's
        # quantile, so that reading is named. A float out of 0..1 is a mistyped
        # band end and gets told about quantiles instead. Both refuse either
        # way; only the direction is chosen.
        for name, value in (("low", low), ("high", high)):
            if value is None:
                continue
            builtin_shaped = isinstance(value, bool) or not isinstance(
                value, (int, float)
            ) or (isinstance(value, int) and not 0 <= value <= 1)
            if builtin_shaped:
                raise GogError(
                    "gog: `range()` takes the band's two ends, each one number "
                    "between 0 and 1, e.g. `range(0.25, 0.75)`. For a sequence of "
                    "integers, gog shadows that name: use `builtins.range` for "
                    "Python's."
                )
            if not 0 <= value <= 1:
                raise GogError(
                    f"gog: `range({name}={value})` is not a probability — the "
                    "band's ends are quantiles, so each is between 0 and 1. "
                    "`range(0.25, 0.75)` is the middle half, `range(0.1, 0.9)` the "
                    "middle 80 percent, and bare `range` the whole group."
                )
        return Atom(
            "transform",
            transform="range",
            low=None if low is None else float(low),
            high=None if high is None else float(high),
        )


range = _Range("transform", transform="range")


class _Jitter(CallableAtom):
    """`jitter` — the categorical-axis spread, a multiple of the default."""

    __slots__ = ()

    def __call__(self, amount: Optional[float] = None) -> Atom:
        if amount is not None and (
            isinstance(amount, bool) or not isinstance(amount, (int, float)) or amount < 0
        ):
            raise GogError(
                "gog: `jitter(amount=)` needs one non-negative number — the spread as a "
                "multiple of the default, e.g. `jitter(0.5)` for half or `jitter(2)` for "
                "double."
            )
        return Atom(
            "transform",
            transform="jitter",
            amount=None if amount is None else float(amount),
        )


jitter = _Jitter("transform", transform="jitter")


class _Stack(CallableAtom):
    """`stack` — the measure-axis pile, optionally filled to 1.

    `stack(share=True)` is the 100% stacked bar: every pile divided by its own
    slot's total, so a slot reads as its split's composition rather than its
    size. A parameter here rather than a second reading of `proportion` because
    the two divide by different totals — `proportion` by the whole frame's, this
    by the slot's — and because it composes with any measurement, including a
    `sum` that `proportion` has no column to take.

    `stack(baseline=)` says where each pile *hangs*, which is the other free
    choice once the heights are fixed. `"zero"` stands every pile on the axis
    (the default), `"center"` hangs each so its middle is at zero, `"wiggle"`
    chooses the foot that makes the bands as flat as it can — the streamgraph.
    Orthogonal to `share`, which scales the heights rather than placing them.
    """

    __slots__ = ()

    def __call__(
        self, share: Optional[bool] = None, baseline: Optional[str] = None
    ) -> Atom:
        if share is not None and not isinstance(share, bool):
            raise GogError(
                "gog: `stack(share=)` is True or False — True fills every pile to 1 "
                "(the 100% stacked bar), False piles the values themselves. For shares "
                "of the whole plot rather than of each slot, `proportion` is the "
                "transform you want."
            )
        if baseline is not None and not isinstance(baseline, str):
            raise GogError(
                'gog: `stack(baseline=)` is one of "zero", "center" or "wiggle" — '
                '"zero" stands every pile on the axis, "center" hangs each pile so its '
                'middle is at zero, "wiggle" chooses the foot that makes the bands as '
                "flat as it can (the streamgraph)."
            )
        return Atom("transform", transform="stack", share=share, baseline=baseline)


stack = _Stack("transform", transform="stack")


def bounds(
    lower: Optional[Column] = None,
    upper: Optional[Column] = None,
    start: Optional[Column] = None,
    end: Optional[Column] = None,
) -> Atom:
    """Pre-computed bounds: `lower`/`upper` bound the measure axis, `start`/`end` the domain."""
    if lower is None and upper is None and start is None and end is None:
        raise GogError(
            "gog: `bounds()` needs column names — `bounds(col.lo, col.hi)` bounds the "
            "measure axis, and on a `zone` `bounds(start=col.a, end=col.b)` bounds the "
            "domain axis."
        )
    return Atom(
        "transform",
        transform="bounds",
        lower=None if lower is None else column_name(lower, "bounds"),
        upper=None if upper is None else column_name(upper, "bounds"),
        start=None if start is None else column_name(start, "bounds"),
        end=None if end is None else column_name(end, "bounds"),
    )


def partition(*levels: Column, cross: bool = False) -> Atom:
    """Divide a whole among nested parts — one ring per level of a hierarchy.

    The hierarchy arrives as **columns**, outermost first: one row of the table is
    one leaf, and `partition(col.group, col.item, col.detail)` says which columns
    spell the path down to it. A blank level ends that branch early, which is what
    gives a real hierarchy its ragged rim.

    `zone * partition(...)` flat is the icicle; the same sentence `+ polar()` is
    the sunburst. `text * partition(...) + label(col.name)` names each node where
    it sits. What each branch is weighed by rides on `x`; bind nothing and every
    leaf weighs 1.

    `cross=True` turns the levels across each other instead of down one axis: the
    first divides the width, the second divides the height *within* each of those
    columns. That is the **mosaic**, and because both directions are then spent on
    the hierarchy there is no ring left to step and only the leaves are drawn.
    """
    if not levels:
        raise GogError(
            "gog: `partition()` needs the hierarchy's columns, outermost first — "
            "`partition(col.group, col.item, col.detail)` puts `group` on the "
            "innermost ring and `detail` on the rim."
        )
    if not isinstance(cross, bool):
        raise GogError(
            "gog: `partition(cross=)` is True or False — True crosses the levels "
            "(the mosaic: the first divides the width, the second the height "
            "within each column), False nests them down one axis (the icicle, and "
            "the sunburst in `polar()`)."
        )
    return Atom(
        "transform",
        transform="partition",
        levels=[column_name(level, "partition") for level in levels],
        # Sent only when True, so a nested partition's wire form is byte-identical
        # to what it was before this existed — `carry` drops a `None`.
        cross=True if cross else None,
    )


def flow(*stages: Column) -> Atom:
    """Lay a magnitude through its stages — the flow diagram.

    The stages arrive as **columns**, in reading order: one row of the table is
    one path through all of them, and `flow(col.klass, col.sex, col.survived)`
    runs each row from its first stage to its last. Rows sharing a path add
    together, which quietly sets aside any column the atom did not name.

    Three marks read the one layout. `ribbon * flow(...)` draws the bands,
    `zone * flow(...)` the stacked slots, and `text * flow(...) +
    label(col.name)` names each slot where it sits. What each path is weighed
    by rides on `y`; bind nothing and every path weighs 1. On the band layer,
    `color(<stage>)` colors every band by the category its path holds there;
    the slots take their paint from `style()`.

    The stage axis is drawn from the atom's own columns, so there is nothing
    for `x()` to say — to reorder the stages, reorder the arguments.
    """
    if len(stages) < 2:
        raise GogError(
            "gog: `flow()` needs at least two stage columns, in reading order — "
            "`flow(col.klass, col.sex, col.survived)` runs each row from its "
            "first stage to its last. One column has no between."
        )
    return Atom(
        "transform",
        transform="flow",
        stages=[column_name(stage, "flow") for stage in stages],
    )


def layout(from_, to):
    """Place a graph — node positions computed from an edge table.

    The two columns name each row's endpoints, and one row of the table is one
    edge between its two values. The nodes derive: their names from the
    columns' union, their `degree` from counting. Three marks read the one
    placement inside `network()`: `edge * layout(col.a, col.b)` draws the
    connections, `point * layout(...)` the nodes, and `text * layout(...) +
    label(col.name)` names them; `size(col.degree)` reads each node's relation
    count.

    The first parameter is spelled `from_` because `from` is Python's word,
    the same escape the language itself uses; the column it names is ordinary.
    Nothing binds to `x`, `y` or `z` — the third dimension, when you want it,
    is the space's: `network(turn=30, tilt=25)`.
    """
    return Atom(
        "transform",
        transform="layout",
        **{"from": column_name(from_, "layout"), "to": column_name(to, "layout")},
    )


# ---------------------------------------------------------------------------
# Positions and coordinate spaces — always the plot's, unless a layer says so
# ---------------------------------------------------------------------------

SCALE_NAMES = ("linear", "log", "time", "category")


def _check_scale(scale: Optional[str]) -> Optional[str]:
    if scale is None:
        return None
    if not isinstance(scale, str):
        raise GogError(
            'gog: `scale=` needs a single string, e.g. `x(col.gdp, scale="log")`.'
        )
    if scale not in SCALE_NAMES:
        names = ", ".join(f'"{name}"' for name in SCALE_NAMES)
        raise GogError(f'gog: `scale="{scale}"` is not a scale. gog has {names}.')
    return scale


def _check_base(base: Optional[float]) -> Optional[float]:
    if base is None:
        return None
    if isinstance(base, bool) or not isinstance(base, (int, float)):
        raise GogError(
            'gog: `base=` needs a single number, e.g. `x(col.bits, scale="log", base=2)`.'
        )
    if base <= 1:
        raise GogError(
            f"gog: `base={base}` is not a base a logarithm can have — it must be greater "
            f"than 1. Use 10 (the default), 2 for doublings, or `math.e` for e-foldings."
        )
    return float(base)


Limits = Optional[Sequence[Optional[float]]]


def _check_limits(limits: Limits) -> Limits:
    """The domain the channel runs over, when the data is not the authority.

    Two numbers, either of which may be ``None`` on its own to leave that end to
    the data: ``limits=(0, None)`` pins a baseline and lets the top follow.
    ``None`` is already JSON ``null``, which is the engine's shape for an
    unstated end, so Python's spelling and the wire agree with no special case
    (spec §10).
    """
    if limits is None:
        return None
    if isinstance(limits, (str, bytes)) or not isinstance(limits, Sequence) or len(limits) != 2:
        raise GogError(
            "gog: `limits=` needs two numbers, e.g. `x(col.hour, limits=(0, 24))`. "
            "Use `None` for an end the data should decide: `(0, None)`."
        )
    out: list = []
    for end in limits:
        if end is None:
            out.append(None)
            continue
        # A domain on a temporal axis is written in dates, not epoch arithmetic:
        # `limits=(date(2024, 1, 1), date(2024, 12, 31))`. Converted by the same
        # function `to_wire` converts the *column* by, because representation is
        # the binding's job — and because the two disagreeing silently would put
        # the domain off by a factor of 86400 and exclude every row.
        if isinstance(end, (date, datetime)):
            out.append(_epoch_seconds(end))
            continue
        if isinstance(end, bool) or not isinstance(end, (int, float)):
            raise GogError(
                "gog: `limits=` needs two numbers, e.g. `x(col.hour, limits=(0, 24))`. "
                "Use `None` for an end the data should decide: `(0, None)`."
            )
        out.append(float(end))
    lo, hi = out
    if lo is not None and hi is not None and not lo < hi:
        # Written out rather than with `min`/`max`: inside this module those
        # names are gog's transforms, so calling them here raises the shadowing
        # error at the user instead of the message they need.
        low, high = (lo, hi) if lo < hi else (hi, lo)
        raise GogError(
            f"gog: `limits=({lo}, {hi})` runs backwards or has no width — the first "
            f"number is the low end. Write `({low}, {high})`."
        )
    return out


def _check_tick_count(tick_count: Any) -> Optional[int]:
    """How many ticks an axis should aim for (spec §10).

    A *target*, not a promise: the count picks a step and the step is then
    rounded to a human number, so 8 on a 0..100 axis gets a step of 10 and nine
    ticks. Two is the floor — one tick shows a place but no direction — and the
    engine says so as well, because a binding is not the only way in.
    """
    if tick_count is None:
        return None
    if isinstance(tick_count, bool) or not isinstance(tick_count, (int, float)):
        raise GogError(
            "gog: `tick_count=` needs one number, e.g. `x(col.gdp, tick_count=8)`. "
            "It is how many ticks the axis aims for."
        )
    if int(tick_count) != tick_count:
        raise GogError(
            f"gog: `tick_count={tick_count}` is not a whole number — an axis cannot "
            f"have a fraction of a tick. Try `tick_count={round(tick_count)}`."
        )
    if tick_count < 2:
        raise GogError(
            f"gog: `tick_count={int(tick_count)}` — an axis needs at least two ticks "
            "to show a direction as well as a place. Ask for 2 or more, or leave "
            "`tick_count` off for the default of 5."
        )
    return int(tick_count)


def _check_free(free: Any, name: str) -> bool:
    """`free=True` — fit this axis from each panel's own rows (spec §11).

    A flag rather than a value, because the rest of the question is answered by
    *where* it was written: `y(col.life, free=True)` frees y, `x(...)` frees x.
    """
    if free is None or free is False:
        return False
    if free is not True:
        raise GogError(
            "gog: `free=` is True or False — it says whether this axis is fitted "
            "per panel. Which axis is up to which binding you write it on: "
            f"`{name}(col.<name>, free=True)` frees {name}."
        )
    return True


def _position(kind: str, name: str, field: Any, scale: Any, base: Any, limits: Any = None,
              tick_count: Any = None, free: Any = None) -> Atom:
    return Atom(
        kind,
        field=column_name(field, name),
        scale=_check_scale(scale),
        base=_check_base(base),
        limits=_check_limits(limits),
        tick_count=_check_tick_count(tick_count),
        free=_check_free(free, name),
    )


def x(field: Column, scale: Optional[str] = None, base: Optional[float] = None,
      limits: Limits = None, tick_count: Optional[int] = None,
      free: bool = False) -> Atom:
    """Bind the x axis to a column."""
    return _position("coord_x", "x", field, scale, base, limits, tick_count, free)


def y(field: Column, scale: Optional[str] = None, base: Optional[float] = None,
      limits: Limits = None, tick_count: Optional[int] = None,
      free: bool = False) -> Atom:
    """Bind the y axis to a column."""
    return _position("coord_y", "y", field, scale, base, limits, tick_count, free)


def z(field: Column, scale: Optional[str] = None, base: Optional[float] = None,
      limits: Limits = None, tick_count: Optional[int] = None,
      free: bool = False) -> Atom:
    """Bind the z axis to a column — one more vowel, not a chart type."""
    return _position("coord_z", "z", field, scale, base, limits, tick_count, free)


def _degrees(value, atom: str, name: str) -> float:
    """A viewing angle: one finite number of degrees, refused with direction.

    `float("left")` raised a bare `ValueError` here once, which `except
    GogError` missed — the one class every other refusal in the package
    arrives as. Julia and JavaScript run the same check in the same words.
    """
    if (
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or value != value
        or value in (float("inf"), float("-inf"))
    ):
        raise GogError(f"gog: `{atom}({name}=)` needs a single number of degrees.")
    return float(value)


class _Space(CallableAtom):
    """`space` — the angle a 3-D plot is viewed from."""

    __slots__ = ()

    def __call__(self, turn: float = 30, tilt: float = 25) -> Atom:
        return Atom(
            "coord_space",
            turn=_degrees(turn, "space", "turn"),
            tilt=_degrees(tilt, "space", "tilt"),
        )


space = _Space("coord_space", turn=30.0, tilt=25.0)


class _Polar(CallableAtom):
    """`polar` — the plane bent into a circle: x is the angle, y the radius."""

    __slots__ = ()

    def __call__(self, start: float = 0) -> Atom:
        return Atom("coord_polar", start=_degrees(start, "polar", "start"))


polar = _Polar("coord_polar", start=0.0)


class _Nest(CallableAtom):
    """`nest` — the panel packed with nested regions: the measure becomes an area.

    Callable and bare both work, like every other space. It takes no argument
    because it has no view to set: `space` and `polar` carry an angle you could
    turn the same picture through, and a packing has nothing underneath to turn.
    """

    __slots__ = ()

    def __call__(self) -> Atom:
        return Atom("coord_nest")


nest = _Nest("coord_nest")


class _Globe(CallableAtom):
    """`globe` — the sphere itself, viewed: x is longitude, y is latitude.

    The marks that draw on a `map` draw here, standing at their places on the
    sphere's surface: a point is a place, a path bends along great circles, a
    rule is a meridian or a parallel, and a zone with `group` fills each region
    of a boundary. `turn` and `tilt` name the place the view faces — a bearing
    that wraps and a latitude that stops at the poles — and rows on the far
    half are hidden behind the sphere, with the count said out loud. The
    graticule is the panel grid.
    """

    __slots__ = ()

    def __call__(self, turn: float = 0, tilt: float = 0) -> Atom:
        return Atom(
            "coord_globe",
            turn=_degrees(turn, "globe", "turn"),
            tilt=_degrees(tilt, "globe", "tilt"),
        )


globe = _Globe("coord_globe", turn=0.0, tilt=0.0)


class _Network(CallableAtom):
    """`network` — the graph-theoretic space, where a `layout` is drawn.

    A layout's positions are the graph's, not the data's, so this space draws
    no axes, no ticks and no grid. Stating a viewing angle states the third
    dimension: bare `network()` draws the graph flat, `network(turn=30,
    tilt=25)` computes the placement in a cube and draws it from that angle.
    """

    __slots__ = ()

    def __call__(self, turn: float | None = None, tilt: float | None = None) -> Atom:
        fields = {}
        if turn is not None:
            fields["turn"] = _degrees(turn, "network", "turn")
        if tilt is not None:
            fields["tilt"] = _degrees(tilt, "network", "tilt")
        return Atom("coord_network", **fields)


network = _Network("coord_network")


class _Map(CallableAtom):
    """`map` — the sphere flattened onto the page: x is longitude, y is latitude.

    Both positions are spent on the place, so a mark that measures along an axis
    has none left. Carry a quantity on a channel instead: ``size(col.x)`` gives
    the proportional-symbol map and ``color(col.x)`` shades each place.

    A sphere cannot be laid flat without giving something up, and area and angle
    cannot both survive. ``preserve`` names which one does: ``"area"`` (the
    default, Equal Earth) keeps every region's true size, which is what a map
    read by area needs; ``"angle"`` (Mercator) keeps every small shape's true
    form and pays for it in area.
    """

    __slots__ = ()

    def __call__(self, preserve: str = "area") -> Atom:
        # Validated at the line the caller wrote, rather than at the wire. The
        # engine checks it too — a rule implemented in one binding is a rule the
        # other three get wrong — but a reader is owed the error where they typed
        # it.
        # `GogError`, as every refusal in this package is: `errors.py` records
        # why — `except ValueError` around a plot would swallow this one and
        # keep the other hundred, and a refusal a user cannot catch uniformly
        # is a refusal they stop catching.
        if preserve not in ("area", "angle"):
            raise GogError(
                'gog: `map(preserve=)` takes "area" or "angle". '
                '"area" keeps every region\'s true size, which is what a map read '
                'by area needs; "angle" keeps every small shape\'s true form and '
                "pays for it in area. A sphere cannot do both."
            )
        return Atom("coord_map", preserve=preserve)


map = _Map("coord_map", preserve="area")

# ---------------------------------------------------------------------------
# Channels — they map a column, and earn a legend to decode it
# ---------------------------------------------------------------------------


def color(field: Column, scale: Optional[str] = None, base: Optional[float] = None,
          limits: Limits = None) -> Atom:
    """Map fill/stroke color to a column."""
    return Atom(
        "color",
        field=column_name(field, "color"),
        scale=_check_scale(scale),
        base=_check_base(base),
        limits=_check_limits(limits),
    )


def colour(*args: Any, **kwargs: Any) -> Atom:
    """The British spelling of `color()`, refused with direction.

    gog writes American English throughout and accepts no second spelling,
    which is Law 2 applied to the vocabulary itself: two ways to write one word
    is a silent letter, and the reader pays for it. ggplot2 accepts both, so a
    reader arriving from there types `colour` and, unexported, would meet a
    bare `NameError` — a message that names no fix. Exported for the same
    reason JavaScript still exports `facet` (spec §13).
    """
    raise GogError(
        "gog: there is no `colour()` channel. gog spells it `color(col.<name>)`: "
        "American English is the grammar's only spelling, and unlike ggplot2 "
        "there is no British alternative."
    )


def size(field: Column, scale: Optional[str] = None, base: Optional[float] = None,
          limits: Limits = None) -> Atom:
    """Map size to a numeric column."""
    return Atom(
        "size",
        field=column_name(field, "size"),
        scale=_check_scale(scale),
        base=_check_base(base),
        limits=_check_limits(limits),
    )


def opacity(field: Column, scale: Optional[str] = None, base: Optional[float] = None,
          limits: Limits = None) -> Atom:
    """Map opacity to a numeric column."""
    return Atom(
        "opacity",
        field=column_name(field, "opacity"),
        scale=_check_scale(scale),
        base=_check_base(base),
        limits=_check_limits(limits),
    )


def group(field: Column) -> Atom:
    """Group a line/path by a column, without giving each group a color."""
    return Atom("group", field=column_name(field, "group"))


def shape(field: Column) -> Atom:
    """Map glyph shape to a categorical column."""
    return Atom("shape", field=column_name(field, "shape"))


def pattern(field: Column) -> Atom:
    """Map paint texture to a categorical column — `shape`'s twin."""
    return Atom("pattern", field=column_name(field, "pattern"))


def label(field: Column) -> Atom:
    """Draw a column's values as text — the `text` mark's content."""
    return Atom("label", field=column_name(field, "label"))


def _check_speed(speed: Optional[float]) -> Optional[float]:
    if speed is None:
        return None
    if isinstance(speed, bool) or not isinstance(speed, (int, float)):
        raise GogError(
            "gog: `speed=` needs a single number, e.g. `play(col.year, speed=2)`. "
            "It is how many times faster than normal the frames run."
        )
    if speed <= 0:
        raise GogError(
            f"gog: `speed={speed}` — a speed is a multiple of the normal pace, so it "
            f"has to be above zero. `speed=2` is twice as fast, `speed=0.5` half."
        )
    return float(speed)


def play(field: Column, speed: Optional[float] = None) -> Atom:
    """Cut the plot into frames and play them — the time dimension.

    `play` is `facet` read in time. Both split the rows by a column's distinct
    values; `| facet(col.continent)` lays the pieces out across the page and
    `play(col.year)` lays them out in sequence.

    Every scale, the color map and every legend are fitted across the whole
    sequence rather than per frame, so the axes hold still and only the data
    moves. A layer that does not bind `play` is drawn in every frame, which is
    how a reference line stands still behind the marks that move.

    Unlike `facet`, a number is welcome: panels compete for page area, frames
    compete for time. `speed` is how many times faster than normal the frames
    run. A static image made from the plot shows the first frame.
    """
    return Atom("play", field=column_name(field, "play"), speed=_check_speed(speed))


def _check_brush_at(at):
    """What `at=` was given, and which of the two readings it is.

    One argument rather than two, because the *value* answers the question the
    way a column answers it everywhere else in this grammar: numbers are a
    range, names are a set of slots.
    """
    if at is None:
        return {}
    if isinstance(at, str):
        at = [at]
    seq = list(at)
    if seq and all(isinstance(v, str) for v in seq):
        return {"levels": seq}
    if len(seq) != 2 or not all(isinstance(v, (int, float)) and v == v for v in seq):
        raise GogError(
            "gog: `at=` is where the selection opens: two numbers on a column that "
            "measures, e.g. `brush(col.gdp, at=(1200, 45000))`, or the names to "
            "select on a column of categories."
        )
    return {"at": [float(seq[0]), float(seq[1])]}


class _Brush(CallableAtom):
    """`brush` — bare for a region over the plot's positions, called for a
    bound on one named column. The same both-shapes pattern `bin` uses, and
    for the same reason: `+ brush` must add the atom rather than the function."""

    __slots__ = ()

    def __call__(self, field: Optional[Column] = None, at=None) -> Atom:
        """Let the reader select rows, and push back the rest.

        `brush` puts a bound on one column's values. Rows inside it keep the plot's
        colors; rows outside it are dimmed, so a selection is read against what it
        was taken from. Where the page can run the engine, dragging moves the bound;
        on paper it stays where the sentence put it.

        **A brush highlights. It never removes rows.** Removing rows before the
        statistics run is what `limits` does, on the binding, and it counts what it
        dropped. Change a domain and a histogram re-bins the survivors; brush it and
        the same bars stay, with the selected part standing out.

        One column per `brush`. Write two for a rectangle:
        `brush(col.gdp, at=(1200, 45000)) + brush(col.life, at=(55, 78))`.

        A mark can be brushed when one row is one shape: `point`, `text`, `rule` and
        `zone`. A `line` draws one shape through many rows, so there is no single row
        to select, and gog says so rather than guessing.
        """
        name = "" if field is None else column_name(field, "brush")
        return Atom("brush", field=name, **_check_brush_at(at))


brush = _Brush("brush", field="")


# ---------------------------------------------------------------------------
# Settings — they fix a value, map nothing, and earn no legend (spec §7)
# ---------------------------------------------------------------------------

_STYLE_STRINGS = ("color", "shape", "border_color")
_STYLE_NUMBERS = ("opacity", "size", "border_size")
_STYLE_FLAGS = ("caps", "center")
_STYLE_VALUES: Dict[str, Sequence[str]] = {
    "nudge": ("up", "down", "left", "right"),
    "pattern": ("solid", "dashed", "dotted", "hatch", "crosshatch", "grid", "dots"),
    "arrow": ("end", "start", "both"),
    "reach": ("panel", "edge"),
}
_STYLE_PROPS = (
    _STYLE_STRINGS + _STYLE_NUMBERS + _STYLE_FLAGS + tuple(_STYLE_VALUES)
)

# The British spelling of a setting, and what gog spells it instead. One entry
# per gog word that has a British form; there are three, and `colour()` the
# channel is the fourth word in the grammar with one.
_BRITISH_SETTINGS = {
    "colour": "color",
    "border_colour": "border_color",
    "centre": "center",
}


def style(**props: Any) -> Atom:
    """Set constant visual properties on the nearest preceding mark.

    Channels *map*: `color(col.species)` asks the reader "which species?" and
    earns a legend to answer it. `style()` *sets*: it fixes a property at one
    value for the whole layer, consumes no scale, and produces no legend —
    there is nothing to decode.
    """
    if not props:
        raise GogError(
            "gog: `style()` sets nothing. Name at least one property, e.g. "
            "`style(color='tomato')`."
        )

    for name, value in props.items():
        if name not in _STYLE_PROPS:
            # The British spelling and the ordinary typo part on the *message*
            # and not on the check: one names the word to write, the other
            # lists what exists.
            if name in _BRITISH_SETTINGS:
                raise GogError(
                    f"gog: `style({name}=)` is not a setting. gog spells it "
                    f"`{_BRITISH_SETTINGS[name]}`: American English is the "
                    "grammar's only spelling, and unlike ggplot2 there is no "
                    "British alternative."
                )
            known = ", ".join(sorted(_STYLE_PROPS))
            raise GogError(
                f"gog: `style({name}=)` is not a setting. gog sets: {known}."
            )
        # A column where a value belongs — the mirror of a string where a column
        # belongs, and the same §7 distinction seen from the other side.
        if isinstance(value, Column):
            raise GogError(
                f"gog: `style({name}=)` fixes one value for the whole layer, and "
                f"`{value!r}` is a column. To *map* it — one value per category, with a "
                f"legend to decode it — that is a channel: `{name}({value!r})`."
                if name in ("color", "size", "opacity", "shape", "pattern")
                else f"gog: `style({name}=)` fixes one value, and `{value!r}` is a column."
            )
        if name in _STYLE_STRINGS and not isinstance(value, str):
            raise GogError(
                f"gog: `style({name}=)` needs a single string, e.g. "
                f"`style({name}='tomato')`."
            )
        if name in _STYLE_NUMBERS and (
            isinstance(value, bool) or not isinstance(value, (int, float))
        ):
            raise GogError(
                f"gog: `style({name}=)` needs a single number, e.g. `style({name}=0.3)`."
            )
        if name in _STYLE_FLAGS and not isinstance(value, bool):
            raise GogError(
                f"gog: `style({name}=)` needs True or False."
            )
        if name in _STYLE_VALUES and value not in _STYLE_VALUES[name]:
            allowed = ", ".join(f'"{v}"' for v in _STYLE_VALUES[name])
            raise GogError(f"gog: `style({name}=)` needs one of {allowed}.")

    clean = {
        name: float(value) if name in _STYLE_NUMBERS else value
        for name, value in props.items()
    }
    return Atom("style", props=clean)


# ---------------------------------------------------------------------------
# Plot-level atoms
# ---------------------------------------------------------------------------


def order(field: Column, desc: bool = False) -> Atom:
    """Order the categorical axis by a column."""
    return Atom("order", field=column_name(field, "order"), descending=bool(desc))


def facet(field: Column, wrap: Optional[int] = None) -> Atom:
    """Name the column that splits the plot into panels. Joins with `|` or `/`.

    `wrap` folds a long line of panels into a rectangle — the number is how many
    panels before the line turns. Which *way* the line runs is the operator's to
    say: `| facet(col.g, wrap=4)` puts four to a row, `/ facet(col.g, wrap=4)`
    four to a column.
    """
    if wrap is not None and (isinstance(wrap, bool) or not isinstance(wrap, int)):
        raise GogError(
            "gog: `facet(wrap=)` takes the number of panels to draw before the "
            "line of them turns — one whole number, e.g. `wrap=4`."
        )
    return Atom("facet", field=column_name(field, "facet"), wrap=wrap)


def palette(pal: Union[str, Sequence[str]]) -> Atom:
    """Set the categorical palette — a name, or a list of hex colors."""
    if isinstance(pal, str):
        value: Dict[str, Any] = {"named": pal}
    else:
        try:
            colors: List[str] = [str(c) for c in pal]
        except TypeError:
            raise GogError(
                'gog: `palette()` takes a palette name ("gog", "okabe") or a list of '
                "hex colors."
            ) from None
        value = {"custom": colors}
    return Atom("palette", value=value)


def _text_value(value: Any, atom: str) -> str:
    if not isinstance(value, str):
        raise GogError(f"gog: `{atom}()` needs a string, e.g. `{atom}('Life expectancy')`.")
    return value


THEME_PRESETS = ("gog", "minimal", "bw")
_GRID_VALUES = ("both", "x", "y", "none")
_FRAME_VALUES = ("full", "axes", "none")


def theme(
    preset: Optional[str] = None,
    *,
    grid: Optional[str] = None,
    ratio: Optional[float] = None,
    tick_angle: Optional[float] = None,
    font_size: Optional[float] = None,
    background: Optional[str] = None,
    strip: Optional[str] = None,
    strip_text: Optional[str] = None,
    frame: Optional[str] = None,
    width: Optional[float] = None,
    height: Optional[float] = None,
) -> Atom:
    """Set the plot's furniture — the page rather than the ink.

    Everything here maps no column, so each is a *setting*; but none of it
    belongs to a mark either, which is why it is not `style()`. A layer has no
    gridlines and a plot has no fill, so the two property sets are disjoint, and
    telling them apart by where they were written would make a sub-expression
    mean different things in different places (Law 6). Spec §7 is the ruling.

    A named preset comes first and anything named adjusts it, because a preset
    you cannot adjust sends you straight back to asking for knobs.

    `font_size` is how many pixels a tick label is, and through it the size of
    every other piece of text the plot draws — the axis names and the title are
    a fixed step above it, so `11` (the default) gives 11, 13 and 16 while `16`
    gives 16, 19 and 23. One number rather than three. It is a measurement, not
    a multiplier, so `font_size=1.5` is refused, and it names no typeface: the
    engine measures text with its own width table and has none to choose.

    `strip` is the facet strip's fill: the band above a panel that names the
    level it holds. Same colors as `background`. `theme('bw')` sets it white,
    because a gray band reproduces poorly in print.

    `strip_text` is the ink of the strip's label. Leave it out and gog picks
    whichever of its two defaults reads on the band, so `theme(strip='black')`
    already gives white type; name it when the ink is a real choice, such as a
    navy strip with gold type.

    `width` and `height` are how many pixels the plot asks for. On its own that
    is the image; composed onto a page with `|` or `/` it is the plot's *cell*,
    and the plots that ask for nothing split what is left — which is how a
    marginal histogram says it is thin. One meaning in both places (Law 6), and
    not to be confused with `ratio`, which shapes the panel inside whatever room
    the plot was given.
    """
    if (preset is None and grid is None and ratio is None and tick_angle is None
            and font_size is None and background is None and strip is None
            and strip_text is None and frame is None and width is None
            and height is None):
        raise GogError(
            "gog: `theme()` sets nothing. Name a preset or a property, e.g. "
            "`theme('minimal')` or `theme(grid='none', ratio=1)`."
        )
    if preset is not None and not isinstance(preset, str):
        raise GogError(
            "gog: `theme()` takes a preset name first — `theme('minimal')` — and "
            "everything else by name: `theme(grid='none')`."
        )
    # Checked in the engine too (`check_theme`), which is what makes the rule the
    # grammar's rather than this binding's. Checking here as well is what puts
    # the error on the line that wrote it.
    if grid is not None and grid not in _GRID_VALUES:
        raise GogError(
            "gog: `theme(grid=)` is one of " + ", ".join(f"'{v}'" for v in _GRID_VALUES) + "."
        )
    if ratio is not None and (
        isinstance(ratio, bool) or not isinstance(ratio, (int, float)) or ratio <= 0
    ):
        raise GogError(
            "gog: `theme(ratio=)` is the panel's width divided by its height, so it "
            "needs one positive number. `ratio=1` is a square."
        )
    if tick_angle is not None and (
        isinstance(tick_angle, bool)
        or not isinstance(tick_angle, (int, float))
        or abs(tick_angle) > 90
    ):
        raise GogError(
            "gog: `theme(tick_angle=)` turns the x tick labels between -90 and 90 "
            "degrees. `tick_angle=45` is the usual answer to names that overlap."
        )
    if font_size is not None and (
        isinstance(font_size, bool)
        or not isinstance(font_size, (int, float))
        or font_size < 4
    ):
        raise GogError(
            "gog: `theme(font_size=)` is how many pixels a tick label is, not a "
            "multiplier, so it needs one number of at least 4. The default is 11, "
            "and the axis names and the title are derived from it."
        )
    if frame is not None and frame not in _FRAME_VALUES:
        raise GogError(
            "gog: `theme(frame=)` is one of 'full' (a rectangle round the panel), "
            "'axes' (bottom and left only) or 'none'."
        )
    if background is not None and not isinstance(background, str):
        raise GogError(
            "gog: `theme(background=)` needs a single color, e.g. "
            "`theme(background='white')` or `'transparent'`."
        )
    if strip is not None and not isinstance(strip, str):
        raise GogError(
            "gog: `theme(strip=)` needs a single color for the band above each "
            "panel, e.g. `theme(strip='white')`."
        )
    if strip_text is not None and not isinstance(strip_text, str):
        raise GogError(
            "gog: `theme(strip_text=)` needs a single color for the strip's label. "
            "Leave it out and gog picks the one that reads on the band."
        )
    # One loop for both, because they are one property asked twice — see the
    # engine's `check_theme`, which states the same rule for every binding.
    for name, value in (("width", width), ("height", height)):
        if value is not None and (
            isinstance(value, bool) or not isinstance(value, (int, float)) or value < 40
        ):
            raise GogError(
                f"gog: `theme({name}=)` is how many pixels the plot asks for, so it "
                f"needs one number of at least 40. On its own it sizes the image; "
                f"composed with `|` or `/` it sizes the plot's cell on the page."
            )
    return Atom(
        "theme",
        preset=preset,
        grid=grid,
        ratio=None if ratio is None else float(ratio),
        tick_angle=None if tick_angle is None else float(tick_angle),
        font_size=None if font_size is None else float(font_size),
        background=background,
        strip=strip,
        strip_text=strip_text,
        frame=frame,
        width=None if width is None else float(width),
        height=None if height is None else float(height),
    )


def title(text: str) -> Atom:
    """Set the plot title."""
    return Atom("title", value=_text_value(text, "title"))


def x_label(text: str) -> Atom:
    """Override the x-axis label."""
    return Atom("x_label", value=_text_value(text, "x_label"))


def y_label(text: str) -> Atom:
    """Override the y-axis label."""
    return Atom("y_label", value=_text_value(text, "y_label"))


def z_label(text: str) -> Atom:
    """Override the z-axis label."""
    return Atom("z_label", value=_text_value(text, "z_label"))
