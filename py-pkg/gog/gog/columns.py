# columns.py — how a Python expression names a column
#
# This is the module the R front end does not need a line of code for, and it
# is the whole of what makes the Python sentence differ from the R one. Spec §8
# ("the cross-language wrinkles"): every binding hands the engine a column
# *name*, and each language captures a bare name its own way — R with
# `substitute()`, Julia with a symbol (`:gdp`), Python with a small accessor,
# `col.gdp`. The mask logic lives once in Rust; only the capture differs.
#
# Capture is the *first* wrinkle, and for these three languages it is the only
# one. JavaScript has a second and larger one — it cannot overload `+`/`*`/`|`/`/`
# at all, so it is the first target that must write a different sentence rather
# than a differently-captured one (spec §8).
#
#     R       data(gapminder) + point + x(gdp)      + y(life)
#     Python  data(gapminder) + point + x(col.gdp)  + y(col.life)
#     Julia   data(gapminder) + point + x(:gdp)     + y(:life)
#     JS      plot(data(gapminder), point, x(col.gdp), y(col.life))
#
# JavaScript's *capture* answer, decided 2026-07-25, is this module's: the same
# `col` accessor, mandatory for the same reason, a `Proxy` where this is a
# `__getattr__`. The two languages with no bare names give the same answer, which
# is Law 2 rather than a coincidence — the reasoning below is written for Python
# and every line of it holds for JavaScript unchanged. Only the *sentence* around
# the atoms differs (spec §8, "JavaScript's surface").
#
# The accessor is not decoration, and this is the reason it earns its four
# characters rather than being a tax on Python users. Python has no bare names,
# so without `col` a column could only be written as a string — and in this
# grammar a string is how you spell a *value*: `style(color = "tomato")`,
# `title("...")`, `palette("okabe")`. Spec §18 refuses `color("red")` as a
# channel argument precisely because **a channel takes a column, never a
# value**; in R that refusal is free, because a bare `red` is not a string.
# Restoring the distinction here is what lets the same refusal be given with
# direction — "map with `color(col.species)`, set with `style(color='red')`" —
# instead of the engine reporting a missing column called `red`, which blames
# the reader for what the binding lost.
#
# So the rule this module enforces is one line: **`col.name` is a column,
# everything else is a value.** It is the same rule in every atom, which is
# what Law 1 asks of it.

from .errors import GogError


class Column:
    """A column name captured from `col.<name>` — never a value."""

    __slots__ = ("name",)

    def __init__(self, name: str) -> None:
        self.name = name

    def __repr__(self) -> str:
        if self.name.isidentifier():
            return f"col.{self.name}"
        return f"col[{self.name!r}]"

    def __eq__(self, other: object) -> bool:
        return isinstance(other, Column) and other.name == self.name

    def __hash__(self) -> int:
        return hash((Column, self.name))

    # Arithmetic on a column would be a *computed channel* — `x(sin(t))` —
    # which spec §8 parks as possible binding-side sugar and has not designed.
    # Without these, `col.a + col.b` raises Python's "unsupported operand
    # type(s) for +", which says nothing about what to do; with them the answer
    # names the place the computation belongs, which is the host language.
    def _computed(self, _other: object) -> "Column":
        raise GogError(
            "gog: a channel takes a column name, not an expression — gog has no "
            "computed channels. Compute the column in Python first and bind the "
            "result: `df['ratio'] = df['a'] / df['b']`, then `y(col.ratio)`."
        )

    __add__ = __radd__ = _computed
    __sub__ = __rsub__ = _computed
    __mul__ = __rmul__ = _computed
    __truediv__ = __rtruediv__ = _computed


class _ColumnAccessor:
    """`col` — the bare-name capture layer. `col.gdp`, or `col["life exp"]`."""

    __slots__ = ()

    def __getattr__(self, name: str) -> Column:
        # Dunder lookups must fail as attribute errors, or `copy.deepcopy`,
        # pickling and `repr()` machinery would each be handed a Column and
        # misbehave far from here.
        if name.startswith("__") and name.endswith("__"):
            raise AttributeError(name)
        return Column(name)

    def __getitem__(self, name: str) -> Column:
        # The escape hatch for a column name Python cannot spell as an
        # identifier — `col["life exp"]`, `col["2007"]`. Law 8: enforce
        # well-formedness hard, never forbid the ugly-but-legal.
        if not isinstance(name, str):
            raise GogError(
                "gog: `col[...]` takes a column name as a string — "
                '`col["life exp"]`. For a name Python can spell, `col.gdp` reads better.'
            )
        return Column(name)

    def __call__(self, *_args: object, **_kwargs: object) -> Column:
        raise GogError(
            "gog: `col` is not a function — a column is `col.gdp`, or "
            '`col["life exp"]` when the name is not a Python identifier.'
        )

    def __repr__(self) -> str:
        return "col"


col = _ColumnAccessor()


# Channels that also exist as a `style()` setting. Spec §7 is the distinction
# these two spellings sit either side of: a channel *maps* a column and earns a
# legend; a setting *fixes* one value and earns none. It is exactly the mistake
# a string in a channel is usually reaching for, so the refusal names both.
_SETTABLE = {"color", "size", "opacity", "shape", "pattern"}


def column_name(value: object, atom: str) -> str:
    """Take the column name out of `col.x`, refusing a value with direction."""
    if isinstance(value, Column):
        return value.name

    if value is col:
        raise GogError(
            f"gog: `{atom}()` needs a column — `col` on its own is the accessor, "
            f"not a column. Write `{atom}(col.<name>)`."
        )

    if isinstance(value, str):
        direction = (
            f"`{atom}(col.{value})` maps the column called `{value}`"
            if value.isidentifier()
            else f'`{atom}(col["{value}"])` maps the column of that name'
        )
        setting = (
            f"\n  To fix one value for the whole layer instead — no legend, nothing "
            f'to decode — that is a setting: `style({atom}="{value}")`.'
            if atom in _SETTABLE
            else ""
        )
        raise GogError(
            f'gog: `{atom}("{value}")` binds a *value*, and a channel takes a '
            f"*column*. Python has no bare names, so a column is written with the "
            f"accessor: {direction}.{setting}"
        )

    # A list, an array, a Series — the *values* rather than the name. This is
    # spec §18's refused sentence arriving in Python dress: a plot is a mapping
    # from a table (Law 4 — the table is the context that makes a bare name mean
    # something), so a channel takes a column and never values, and
    # vector-direct plotting is a decided refusal. Give it the direction §18
    # records: put the values in a table first.
    if hasattr(value, "__len__") and hasattr(value, "__iter__"):
        raise GogError(
            f"gog: `{atom}()` takes a column *name*, and this is a column's *values* "
            f"({type(value).__name__}). gog plots a table: a channel names one of its "
            f"columns, so that a legend, an axis and a second layer all know what they "
            f"are talking about. Put the values in a table first — "
            f"`data({{'value': values}}) + point + {atom}(col.value)` — or, if the table "
            f"already exists, name the column: `{atom}(col.<name>)`."
        )

    raise GogError(
        f"gog: `{atom}()` takes a column — `{atom}(col.<name>)`. "
        f"Got {type(value).__name__}."
    )
