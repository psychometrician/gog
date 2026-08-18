# spec.py — the plot under construction, and the four operators that build it
#
# The mirror of `r-pkg/gog/R/spec.R`. A `Plot` holds the in-progress
# specification plus the actual tables; `+` accumulates atoms left to right,
# `*` derives a layer from a mark and a transform, `|` and `/` facet.
#
# Python's operator precedence is R's for the four gog uses — `*` binds tighter
# than `+`, `+` tighter than `|`, and `/` sits with `*` — so the sentences read
# the same in both languages and mean the same thing, including the two places
# the precedence does real work:
#
#     data(df) + bar * bin + x(col.height)     `bar * bin` resolves first
#     plot | facet(col.a) / facet(col.b)       `/` resolves first, into a pair
#
# That is not luck, and it is worth stating because it is the part of the
# grammar most likely to be assumed rather than checked: `|` is the lowest of
# the four in both languages, which is what lets a whole plot sit to its left
# without parentheses.
#
# One thing R gives away that Python must do by hand: **`+` returns a new plot
# and never touches the old one.** R's copy-on-modify makes that free; here the
# spec is a mutable dict, so a shared `base` plot would grow a layer every time
# a variant was built from it, and `base + color(...)` would silently change
# what `base + size(...)` meant.

import copy
import sys
import warnings
from typing import Any, Dict, List, Optional, Set, Tuple

from .columns import Column
from .errors import GogError
from .render import Query, refusal_block, render_svg, save, show, svg_block

# ---------------------------------------------------------------------------
# Atoms
# ---------------------------------------------------------------------------


# The five transforms whose names Python already uses for something else. R has
# the same list against base R (`range`, `sum`, `min`, `max`, `data`, `text`,
# `box`, `order`, `jitter`, `stack`) and answers it the same way: a DSL keeps
# its own vocabulary. What differs is that Python's are *builtins*, so the
# shadowing follows a star-import into the whole module rather than sitting in
# the search path — which makes saying so, at the moment it bites, part of the
# binding's job.
_SHADOWED_BUILTINS = frozenset({"bin", "sum", "min", "max", "range"})


class Atom:
    """One word of the grammar — a mark, a transform, a channel, a setting."""

    __slots__ = ("kind", "fields")

    def __init__(self, kind: str, **fields: Any) -> None:
        self.kind = kind
        self.fields = fields

    def __repr__(self) -> str:
        name = self.fields.get("mark") or self.fields.get("transform") or self.kind
        field = self.fields.get("field")
        return f"<gog {name}({field})>" if field else f"<gog {name}>"

    # -- `*` — derive a layer from a mark and a transform -------------------

    def __mul__(self, other: Any) -> "Atom":
        if not isinstance(other, Atom):
            raise GogError(
                f"gog: `*` combines a mark with a transform — `bar * bin`, "
                f"`line * smooth`. Got {type(other).__name__} on the right."
            )

        if self.kind == "mark" and other.kind == "transform":
            layer = Atom(
                "layer",
                mark=self.fields["mark"],
                transforms=[other.fields["transform"]],
                encodings={},
            )
            if self.fields.get("box") is not None:
                layer.fields["box"] = self.fields["box"]
            _carry(layer, other)
            return layer

        if self.kind == "layer" and other.kind == "transform":
            layer = Atom("layer", **copy.deepcopy(self.fields))
            layer.fields["transforms"] = list(layer.fields["transforms"]) + [
                other.fields["transform"]
            ]
            _carry(layer, other)
            return layer

        raise GogError(
            f"gog: `*` is not defined for {self.kind} * {other.kind}. "
            f"Use `*` to combine a mark with a transform, e.g. `bar * bin`."
        )

    # -- `+` — an atom with no plot to join --------------------------------

    def __add__(self, other: Any) -> "Plot":
        raise GogError(
            "gog: these atoms have no plot to join — the sentence starts with the "
            "data: `data(df) + point + x(col.a) + y(col.b)`."
        )

    def __radd__(self, other: Any) -> "Plot":
        # `df + point`: a table on the left. Python asked the atom because a
        # dict and a DataFrame both decline `+` with an unknown type, which is
        # what lets the answer say what to write instead of Python's
        # "unsupported operand type(s)".
        raise GogError(
            "gog: a plot starts with `data()`, which names the table — columns are "
            "`col.<name>` and the nearest named table wins, so the name matters. "
            "Write `data(df) + " + str(self.fields.get("mark", "...")) + " + ...`."
        )

    # -- `|` and `/` — facet -----------------------------------------------

    def __or__(self, other: Any) -> "Atom":
        return _facet_join(self, other, "col", "|")

    def __truediv__(self, other: Any) -> "Atom":
        return _facet_join(self, other, "row", "/")

    # -- calling an atom that takes no parameters ---------------------------

    def __call__(self, *_args: Any, **_kwargs: Any) -> "Atom":
        """A bare atom is not a function — and one such mistake is Python's own.

        `from gog import *` puts five transforms over five builtins, so a module
        that speaks the grammar and then writes `range(10)` reaches gog's
        transform and gets "'Atom' object is not callable", which names neither
        the cause nor the fix. Since the atom knows its own name, it can say
        both. (`bin(30)` is the one that cannot be caught: it is a legal call on
        gog's transform, so the shadowing is silent there — the cost stated in
        the package docstring.)
        """
        name = self.fields.get("transform") or self.fields.get("mark") or self.kind
        if name in _SHADOWED_BUILTINS:
            raise GogError(
                f"gog: `{name}` here is gog's transform, not Python's builtin — "
                f"`from gog import *` shadows `bin`, `sum`, `min`, `max` and `range`. "
                f"For Python's: `from builtins import {name}`, or call "
                f"`builtins.{name}(...)`. For gog's, use it bare: `bar * {name}`."
            )
        raise GogError(
            f"gog: `{name}` takes no parameters — use it bare, e.g. `bar * {name}`."
        )


class CallableAtom(Atom):
    """An atom that is usable bare and also takes parameters.

    `bar * bin` and `bar * bin(30)` have to reach the same code path, the way
    they do in R — where a transform used bare arrives as the function itself
    and `*` calls it with its defaults. Python has no such hook, so the atom
    carries its own defaults and calling it returns a configured copy. The atom
    *is* the bare form; `__call__` is the parameterized one.
    """

    __slots__ = ()

    def __call__(self, *args: Any, **kwargs: Any) -> Atom:  # pragma: no cover
        raise NotImplementedError


def _carry(layer: Atom, transform: Atom) -> None:
    """Move a transform's parameters onto the layer, where the engine reads them.

    `bin`'s count, `density`'s bandwidth, `range`'s two band ends,
    `confidence`'s level, `jitter`'s
    amount, `stack`'s share flag and baseline, and `bounds`' column names ride the *layer*
    on the wire
    (`layer.bin`, `layer.density`, …), not the transform list — the transform
    list is names only. Absent parameters attach nothing, so a bare
    `bar * bin` stays on Sturges' rule and a bare `interval * range` on the
    extremes.
    """
    name = transform.fields["transform"]
    if name not in ("bin", "density", "range", "confidence", "deviation",
                    "quantile", "jitter", "stack", "bounds", "partition", "flow", "layout",
                    "cluster"):
        return
    params = {
        key: value
        for key, value in transform.fields.items()
        if key != "transform" and value is not None
    }
    if params:
        layer.fields[name] = params


# ---------------------------------------------------------------------------
# data() — the table, and its name
# ---------------------------------------------------------------------------


def _name_in_caller(frame_object: Any, depth: int) -> Optional[str]:
    """The variable the caller passed, found by identity in its own frame.

    Law 4 rests on the table having a name: "nearest table wins" resolves *by
    name*, so a table whose name was lost cannot be referred to again and two
    that lost theirs collide. R reads the name off the expression with
    `substitute()`; Python has no expression to read, but it does have the
    caller's frame, and the object the caller just handed over is almost always
    sitting in it under the name they typed. Same mechanism, one level down:
    ask the caller what it calls this thing.

    It answers for `data(gapminder)` and declines for `data(read_csv(...))`,
    which is the honest split — the second genuinely has no name, and §12's
    omission rule makes that an Assumption said out loud rather than a silent
    default.
    """
    frame = sys._getframe(depth)
    while frame is not None:
        for scope in (frame.f_locals, frame.f_globals):
            for name, value in list(scope.items()):
                if value is frame_object and not name.startswith("_"):
                    return name
        # A wrapper function (a plotting helper of the user's own) puts one more
        # frame between us and the name; walk out until the name appears.
        frame = frame.f_back
    return None


def data(frame: Any, name: Optional[str] = None) -> "Plot":
    """Start a plot with a table.

    `data(df)` names the table `df`. Pass `name=` when the table is an
    expression rather than a variable, or when two tables would otherwise
    collide.
    """
    if isinstance(frame, Atom):
        raise GogError(
            "gog: `data()` takes a table, not an atom — `data(df) + point + "
            "x(col.a) + y(col.b)`."
        )
    if isinstance(frame, Column):
        raise GogError(
            "gog: `data()` takes the table itself, not a column — `data(df)`, "
            "then the columns are bare inside the plot: `+ x(col.gdp)`."
        )

    if name is None:
        name = _name_in_caller(frame, depth=2)
    if name is None:
        # The ambiguous case §12 reserves the Assumption for: only the caller
        # knows what this table should be called, so the default is announced
        # with the direction rather than taken in silence.
        #
        # The name is only ever used to *distinguish* one table from another, so
        # a second anonymous table is given `data2` rather than refused — the
        # counting happens where two of them meet, since a plot built on its own
        # has nothing to count against.
        warnings.warn(
            "gog: this table was built in the call rather than passed as a "
            "variable, so it has no name to take, and it is called `data`. Name "
            "it with `data(df, name='...')` if a message about it should say "
            "something you recognize.",
            stacklevel=2,
        )
        return Plot(_new_spec("data"), {"data": frame}, anonymous={"data"})

    return Plot(_new_spec(name), {name: frame})


def _new_spec(name: str) -> Dict[str, Any]:
    """The empty sentence, given the name of the table it is about.

    One skeleton, shared by every atom that can open a plot — `data()` and
    `query()` today. Two copies of this dict is how a field gets added to one
    data source and not the other.
    """
    return {
        "data": name,
        "layers": [],
        "coord": "flat",
        "title": None,
        # `AxisSpec` is the axis's furniture, which is only its name: `tick_count`
        # moved to the channel binding 2026-07-26, beside `scale` and `limits`,
        # because how many ticks an axis gets is a property of the scale (§10).
        "x_axis": {"label": None},
        "y_axis": {"label": None},
        "z_axis": {"label": None},
        "x": None,
        "y": None,
        "z": None,
        "channels": {},  # plot-scoped channels — those written before any mark
    }


def query(connection: Any, sql: Optional[str] = None, name: Optional[str] = None) -> "Plot":
    """Start a plot with a table that lives in a database.

    `query()` stands exactly where `data()` stands, and **nothing after it
    changes** — same operators, same channels, same bare column names, same
    transforms:

        data(orders)                              + bar + x(col.status)
        query(con, "SELECT * FROM main.orders")   + bar + x(col.status)

    The SQL is confined to this one argument and never enters the grammar, which
    is the whole point: `x(col.status)` is still a column name resolved by the
    same mask, not a fragment of another language.

    The connection is the caller's own — gog opens none and depends on no
    driver. Either a PEP 249 connection (`sqlite3`, DuckDB, `psycopg`,
    `databricks-sql-connector`) or a Spark session, whose `.sql()` reaches a
    Unity Catalog table.

    The query is **not run here**. It runs once, at render.

    The table is called `query` unless `name=` says otherwise, which is what a
    second one in the same sentence needs — a layer resolves its columns against
    the nearest table *by name*, so two tables sharing one name collide, exactly
    as two unnamed `data()` tables do.
    """
    # `sql` defaults so that `query("SELECT ...")` — the mistake `data(df)`
    # invites, since that atom takes one argument — reaches this refusal instead
    # of Python's own "missing 1 required positional argument", which names the
    # parameter and not the fix.
    if sql is None:
        if isinstance(connection, str):
            raise GogError(
                "gog: `query()` takes the connection first, then the SELECT — "
                "`query(con, 'SELECT ...')`. A query on its own cannot say which "
                "database it runs against, which is why the connection is written "
                "out loud. If the rows are already in hand, that is `data(df)`."
            )
        raise GogError(
            "gog: `query()` takes a connection and a SELECT — "
            f"`query(con, 'SELECT ...')`. Got {type(connection).__name__} and no query."
        )
    if isinstance(connection, str):
        raise GogError(
            "gog: `query()` takes the connection first, then the SELECT — "
            "`query(con, 'SELECT ...')`. A query on its own cannot say which "
            "database it runs against."
        )
    if not isinstance(sql, str):
        raise GogError(
            "gog: `query()` takes a SELECT as text — "
            f"`query(con, 'SELECT ...')`. Got {type(sql).__name__} for the query."
        )

    return Plot(_new_spec(name or "query"), {name or "query": Query(connection, sql)})


# ---------------------------------------------------------------------------
# Plot — the sentence so far
# ---------------------------------------------------------------------------


class Plot:
    """A plot specification under construction. Build it with `+`."""

    __slots__ = ("spec", "frames", "current_layer", "pending_data", "anonymous")

    def __init__(
        self,
        spec: Dict[str, Any],
        frames: Dict[str, Any],
        current_layer: Optional[Dict[str, Any]] = None,
        pending_data: Optional[str] = None,
        anonymous: Optional[Set[str]] = None,
    ) -> None:
        self.spec = spec
        self.frames = frames
        self.current_layer = current_layer
        self.pending_data = pending_data
        # Which of these names the binding invented rather than read from the
        # caller. Only a name the *author* wrote can clash: a generated one
        # means nothing to them, so it can be renamed to make room.
        self.anonymous = set(anonymous) if anonymous else set()

    # -- copying: `+` never edits the plot on its left ----------------------

    def _copy(self) -> "Plot":
        return Plot(
            copy.deepcopy(self.spec),
            dict(self.frames),  # the tables themselves are shared, not copied
            copy.deepcopy(self.current_layer),
            self.pending_data,
            self.anonymous,
        )

    # -- `+` ----------------------------------------------------------------

    def __add__(self, other: Any) -> "Plot":
        plot = self._copy()

        # A second table joins mid-sentence: `... + data(notes) + text + ...`
        if isinstance(other, Plot):
            new_name = next(iter(other.frames))

            # This path keeps the table and returns, so anything else the right
            # operand is carrying would go no further. That is fine for a bare
            # `data(df)`, which carries nothing, and silent loss for a
            # parenthesized group, whose marks, positions and titles simply stop
            # existing. Refuse instead: a dropped binding is never acceptable
            # (§12), and a sub-expression that means one thing alone and nothing
            # at all in context breaks Compositional Invariance (Law 6).
            if (
                other.spec != _new_spec(new_name)
                or other.current_layer is not None
                or other.pending_data is not None
            ):
                raise GogError(
                    f"gog: parentheses do not group marks, so everything inside these "
                    f"would be dropped. Write the marks in sequence instead, and repeat "
                    f"`data()` before each one that reads that table: "
                    f"`+ data({new_name}) + point + data({new_name}) + area`. "
                    f"Parentheses compose whole plots, with `|` and `/`."
                )

            existing = plot.frames.get(new_name)
            if existing is not None and existing is not other.frames[new_name]:
                if new_name not in other.anonymous:
                    raise GogError(
                        f"gog: two different tables are both called `{new_name}` — a layer "
                        f"resolves its columns against the nearest table by name, so one of "
                        f"these can never be reached. Give them distinct names: "
                        f"`data(df, name='...')`."
                    )
                # The binding invented this name, so it can move. Nothing refers
                # to it yet — a bare `data()` carries no layers — so giving it a
                # free one is the whole rename.
                fresh = _free_name(plot.frames)
                plot.frames[fresh] = other.frames[new_name]
                plot.anonymous.add(fresh)
                plot.pending_data = fresh
                return plot
            plot.frames.update(other.frames)
            plot.anonymous |= other.anonymous
            plot.pending_data = new_name
            return plot

        if not isinstance(other, Atom):
            if callable(other):
                label = getattr(other, "__name__", "that atom")
                raise GogError(
                    f"gog: `{label}` needs its argument — it is a function, and adding "
                    f"it bare adds the function itself. Write `{label}(...)`."
                )
            raise GogError(
                "gog: `+` joins gog atoms to a plot — a mark, a channel, a setting. "
                f"Got {type(other).__name__}. A table joins through `data()`: "
                "`+ data(notes)`."
            )

        kind = other.kind

        if kind == "mark":
            plot._open_layer(
                {
                    "mark": other.fields["mark"],
                    "encodings": {},
                    "transforms": [],
                    "data": plot.pending_data,
                }
            )
            if other.fields.get("box") is not None:
                plot.current_layer["box"] = other.fields["box"]

        elif kind == "layer":
            layer = {
                "mark": other.fields["mark"],
                "encodings": copy.deepcopy(other.fields.get("encodings", {})),
                "transforms": list(other.fields["transforms"]),
                "data": plot.pending_data,
            }
            for param in ("bin", "density", "range", "confidence", "deviation",
                          "quantile", "jitter", "stack", "bounds", "partition", "flow", "layout",
                          "cluster", "box"):
                if other.fields.get(param) is not None:
                    layer[param] = copy.deepcopy(other.fields[param])
            plot._open_layer(layer)

        elif kind in ("coord_x", "coord_y", "coord_z"):
            plot._set_position(kind[-1], other)

        elif kind == "coord_space":
            plot.spec["coord"] = {
                "space": {"turn": other.fields["turn"], "tilt": other.fields["tilt"]}
            }

        elif kind == "coord_polar":
            plot.spec["coord"] = {"polar": {"start": other.fields["start"]}}

        # Nest carries no view parameter, so it crosses as the bare string
        # "nest" — the one unit variant left in `CoordSpace`.
        elif kind == "coord_nest":
            plot.spec["coord"] = "nest"

        # A globe carries the place its view faces, in space's own two words:
        # {"globe": {"turn": 0, "tilt": 0}} matches `CoordSpace::Globe(GlobeView)`,
        # and a bare "globe" is not a legal form.
        elif kind == "coord_network":
            view = {
                key: other.fields[key]
                for key in ("turn", "tilt")
                if other.fields.get(key) is not None
            }
            plot.spec["coord"] = {"network": view}

        elif kind == "coord_globe":
            plot.spec["coord"] = {
                "globe": {"turn": other.fields["turn"], "tilt": other.fields["tilt"]}
            }

        # A map carries what the flattening must preserve, the same way space and
        # polar carry theirs: {"map": {"preserve": "area"}} matches
        # `CoordSpace::Map(MapView)`, and a bare "map" is not a legal form.
        elif kind == "coord_map":
            plot.spec["coord"] = {"map": {"preserve": other.fields["preserve"]}}

        elif kind in ("color", "group", "size", "shape", "opacity", "label", "pattern",
                      "play"):
            plot._set_channel(kind, other)

        # Plot-scoped, like `palette`: a predicate over rows is a fact about the
        # data, so every layer reading that column answers to it.
        elif kind == "brush":
            entry = {"field": other.fields["field"]}
            for key in ("at", "levels"):
                if other.fields.get(key) is not None:
                    entry[key] = other.fields[key]
            plot.spec.setdefault("brush", []).append(entry)

        elif kind == "style":
            plot._set_style(other.fields["props"])

        elif kind == "palette":
            plot.spec["palette"] = other.fields["value"]

        elif kind == "theme":
            # Merged rather than replaced, so two `theme()` calls accumulate the
            # way two `style()` calls on one mark do. Only the properties named
            # are written, keeping "said nothing" apart from "asked for the
            # default" (spec §7).
            theme = plot.spec.setdefault("theme", {})
            for key in ("preset", "grid", "ratio", "tick_angle", "font_size", "background", "strip", "strip_text",
                        "frame", "width", "height"):
                if other.fields.get(key) is not None:
                    theme[key] = other.fields[key]

        elif kind == "title":
            plot.spec["title"] = other.fields["value"]

        elif kind in ("x_label", "y_label", "z_label"):
            plot.spec[f"{kind[0]}_axis"]["label"] = other.fields["value"]

        elif kind == "order":
            plot.spec["order"] = {
                "field": other.fields["field"],
                "descending": other.fields["descending"],
            }

        elif kind == "facet":
            raise GogError(
                f"gog: `facet()` joins with `|` (panels side by side) or `/` (panels "
                f"stacked), not `+`. Write `plot | facet(col.{other.fields['field']})` "
                f"or `plot / facet(col.{other.fields['field']})`."
            )

        elif kind == "atom_then_facet":
            # `... + y(col.b) / facet(col.g)`: `/` binds tighter than `+`, so it
            # took the atom written just before it. Apply the atom, then the
            # facet — left to right, as written.
            plot = plot + other.fields["atom"]
            plot.spec.setdefault("facet", {"col": None, "row": None})
            plot.spec["facet"][other.fields["slot"]] = other.fields["facet"]
            if other.fields.get("wrap") is not None:
                plot.spec["facet"]["wrap"] = other.fields["wrap"]

        else:
            raise GogError(f"gog: unknown atom `{kind}`.")

        return plot

    def __radd__(self, other: Any) -> "Plot":
        raise GogError(
            "gog: a plot starts with `data()`, and everything joins it with `+` from "
            "there. A table joins through `data()`: `data(df) + point + x(col.a)`."
        )

    # -- `|` and `/` — facet -------------------------------------------------

    def __or__(self, other: Any) -> "Plot":
        return _facet_join(self, other, "col", "|")

    def __truediv__(self, other: Any) -> "Plot":
        return _facet_join(self, other, "row", "/")

    # -- helpers -------------------------------------------------------------

    def _open_layer(self, layer: Dict[str, Any]) -> None:
        if self.current_layer is not None:
            self.spec["layers"].append(self.current_layer)
        self.current_layer = layer
        self.pending_data = None

    def _set_position(self, channel: str, atom: Atom) -> None:
        """A position is scoped by position, like every other channel.

        Written before any mark it is the plot's; written after one it is that
        layer's, which is what lets a second `data()` say where its own rows go
        (`+ data(notes) + text + x(col.at)`). One axis with two column names,
        never two axes — the scale, the ticks and the space stay the plot's.
        """
        definition = _channel_def(atom)
        if self.current_layer is None:
            self.spec[channel] = definition
        else:
            self.current_layer["encodings"][channel] = definition

    def _set_channel(self, channel: str, atom: Atom) -> None:
        definition = _channel_def(atom)
        if self.current_layer is None:
            self.spec["channels"][channel] = definition
        else:
            self.current_layer["encodings"][channel] = definition

    def _set_style(self, props: Dict[str, Any]) -> None:
        if self.current_layer is None:
            raise GogError(
                "gog: `style()` has no mark to style. Put it after a mark, e.g. "
                "`point + style(color='tomato')`."
            )
        self.current_layer.setdefault("style", {}).update(props)

    def _wire(self) -> Tuple[Dict[str, Any], Dict[str, Any]]:
        """The sealed spec and its tables, ready for the bridge."""
        spec = copy.deepcopy(self.spec)
        if self.current_layer is not None:
            spec["layers"].append(copy.deepcopy(self.current_layer))
        return spec, self.frames

    # -- display -------------------------------------------------------------

    def render(self) -> str:
        """The plot as an SVG string."""
        return render_svg(self)

    def save(self, path: str) -> str:
        """Draw the plot and write the SVG to `path`."""
        return save(self, path)

    def show(self) -> None:
        """Draw the plot and open it in the default browser."""
        show(self)

    def _repr_html_(self) -> str:
        # Jupyter asks for a mime bundle, and this is the method that puts a
        # plot in the cell rather than a repr line.
        #
        # A refusal is the author's mistake, not a fault in the program, so it
        # is shown as the sentence the engine wrote rather than raised into the
        # frontend, which presents it as a traceback through this file and
        # IPython's internals. `render_svg()` and `save()` still raise.
        try:
            return svg_block(render_svg(self), self)
        except GogError as refusal:
            return refusal_block(str(refusal))

    def __repr__(self) -> str:
        spec, _ = self._wire()
        marks = " + ".join(layer["mark"] for layer in spec["layers"])
        return f"<gog plot: {marks or 'no mark'} on {spec['data']}>"


# ---------------------------------------------------------------------------
# Page — separate plots arranged together
#
#   plot_a | plot_b        side by side
#   plot_a / plot_b        one above the other
#   top / (main | right)   nested: the marginal plot
#
# Faceting is one plot split by a variable and sharing everything; composition is
# several plots on one page, each keeping its own coordinate space (spec §11).
# The two wear the same operators and are told apart by the operand types.
#
# What relates the composed plots is one rule, and the engine owns it: the same
# column on the same axis in two of them is one axis — one scale, one panel
# extent, drawn once (`render::page`). Nothing about it is decided here.
#
# **`/` binds tighter than `|` in Python too**, so `a | b / c` reads as
# `a | (b / c)`. Parenthesize when the reading matters; the marginal plot does.
# ---------------------------------------------------------------------------


# The theme properties that describe a *panel*, and so cannot be said about a
# page. The engine holds the same list in `check_page_theme`; this copy is what
# puts the refusal on the line that wrote it.
PANEL_THEME = ("preset", "grid", "ratio", "tick_angle", "font_size",
               "background", "strip", "strip_text", "frame")


class Page:
    """Plots arranged on one page. Build it with `|` and `/`."""

    __slots__ = ("arrange", "cells", "frames", "theme", "anonymous")

    def __init__(self, arrange: str, cells: List[Dict[str, Any]],
                 frames: Dict[str, Any],
                 theme: Optional[Dict[str, Any]] = None,
                 anonymous: Optional[Set[str]] = None) -> None:
        self.arrange = arrange
        self.cells = cells
        self.frames = frames
        self.theme = dict(theme) if theme else {}
        # Which names the binding invented — carried onward so a page composed
        # into a larger one can still give way to an author's name.
        self.anonymous = set(anonymous) if anonymous else set()

    def __or__(self, other: Any) -> "Page":
        return _facet_join(self, other, "col", "|")

    def __truediv__(self, other: Any) -> "Page":
        return _facet_join(self, other, "row", "/")

    def __add__(self, other: Any) -> "Page":
        # A page is plots arranged, and an atom belongs to one of them — with the
        # one exception whose subject is the figure rather than a panel.
        # `theme(width=, height=)` says how big this page is, which is the same
        # sentence a plot writes about itself, and there is nowhere else to write
        # it: two plots side by side divide the page's width and each keep the
        # whole of its height, so only the page can say how much height that is.
        if isinstance(other, Atom) and other.kind == "theme":
            named = [k for k in PANEL_THEME if other.fields.get(k) is not None]
            if named:
                written = (f'theme("{other.fields["preset"]}")'
                           if "preset" in named else f"theme({named[0]}=)")
                raise GogError(
                    f"gog: `{written}` describes a panel, and a page is plots arranged "
                    f"rather than a panel of its own. On a page, `theme()` states how "
                    f"big the figure is — `theme(width=)` and `theme(height=)` — and "
                    f"nothing else. Write this into the plot it describes, before "
                    f"composing: `(plot + {written}) | other_plot`."
                )
            theme = dict(self.theme)
            for key in ("width", "height"):
                if other.fields.get(key) is not None:
                    theme[key] = other.fields[key]
            return Page(self.arrange, copy.deepcopy(self.cells), self.frames, theme,
                        self.anonymous)

        # Everything else belongs one level down. A title for the page as a whole
        # is real and not built — designed, not implemented.
        what = f"`{other.kind}()`" if isinstance(other, Atom) else "that"
        raise GogError(
            f"gog: {what} belongs to a plot, and the left side is a page of them. "
            f"Write it into the plot it describes, before composing: "
            f"`(plot + title('...')) | other_plot`."
        )

    def _wire(self) -> Tuple[Dict[str, Any], Dict[str, Any]]:
        """The sealed page and its tables — the same pair a plot hands over."""
        wire: Dict[str, Any] = {
            "arrange": self.arrange,
            "cells": copy.deepcopy(self.cells),
        }
        if self.theme:
            wire["theme"] = dict(self.theme)
        return wire, self.frames

    # -- display: a page draws through exactly what a plot draws through -----

    def render(self) -> str:
        """The page as an SVG string."""
        return render_svg(self)

    def save(self, path: str) -> str:
        """Draw the page and write the SVG to `path`."""
        return save(self, path)

    def show(self) -> None:
        """Draw the page and open it in the default browser."""
        show(self)

    def _repr_html_(self) -> str:
        # A page is a figure like any other, and a refusal reaches a cell the
        # same way — shown, not raised. See `Plot._repr_html_`.
        try:
            return svg_block(render_svg(self), self)
        except GogError as refusal:
            return refusal_block(str(refusal))

    def __repr__(self) -> str:
        return f"<gog page: {len(self.cells)} cells, {self.arrange}>"


def _figure_cells(figure: Any, arrange: str) -> List[Dict[str, Any]]:
    """The cells `figure` contributes to a page running `arrange`.

    A page already running that way is *flattened* into it, so `a | b | c` is one
    row of three rather than a row of (a row of two, and one) — the reading the
    eye gives it. A page running the other way stays a cell of its own, which is
    what makes `top / (main | right)` two rows, the second holding two plots.

    A page that has stated its own size does not flatten either, whichever way it
    runs: flattening keeps the cells and drops the node, and the node is where the
    size was written.
    """
    if isinstance(figure, Page) and figure.arrange == arrange and not figure.theme:
        return list(figure.cells)
    wire, _ = figure._wire()
    return [wire]


def _free_name(taken: Any) -> str:
    """The next generated table name that nothing is using: `data`, `data2`, …"""
    if "data" not in taken:
        return "data"
    n = 2
    while f"data{n}" in taken:
        n += 1
    return f"data{n}"


def _rename_table(cells: List[Dict[str, Any]], old: str, new: str) -> None:
    """Rewrite every reference to a table, through nested pages.

    A name reaches the wire in exactly two places — the plot's own table and a
    layer that reads a different one — so this is the whole rewrite.
    """
    for cell in cells:
        if cell.get("data") == old:
            cell["data"] = new
        for layer in cell.get("layers", []) or []:
            if layer.get("data") == old:
                layer["data"] = new
        if cell.get("cells"):
            _rename_table(cell["cells"], old, new)


def _merge_frames(
    left: Any, right: Any,
    left_cells: List[Dict[str, Any]], right_cells: List[Dict[str, Any]],
) -> Tuple[Dict[str, Any], Set[str]]:
    """Two figures' tables, under Law 4's rule: one name, one table.

    A name the author wrote is theirs and cannot be moved, so two different
    tables under one of those is still refused. A generated name is the
    binding's own and means nothing to them, so it is renamed to make room
    instead — which is what keeps a page of two anonymous tables legal, the way
    a plot of two already is.
    """
    frames = dict(left.frames)
    anonymous = set(left.anonymous)
    for name, frame in right.frames.items():
        existing = frames.get(name)
        if existing is not None and existing is not frame:
            taken = set(frames) | set(right.frames)
            if name in right.anonymous:
                fresh = _free_name(taken)
                _rename_table(right_cells, name, fresh)
                frames[fresh] = frame
                anonymous.add(fresh)
                continue
            if name in anonymous:
                # The author wrote the incoming one; the binding invented the one
                # already here, so that is the one that moves.
                fresh = _free_name(taken)
                _rename_table(left_cells, name, fresh)
                frames[fresh] = frames.pop(name)
                anonymous.discard(name)
                anonymous.add(fresh)
                frames[name] = frame
                continue
            raise GogError(
                f"gog: two different tables on one page are both called `{name}` — a "
                f"layer resolves its columns against the nearest table by name, so one "
                f"of these can never be reached. Give them distinct names: "
                f"`data(df, name='...')`."
            )
        frames[name] = frame
        if name in right.anonymous:
            anonymous.add(name)
    return frames, anonymous


def _compose(left: Any, right: Any, arrange: str) -> Page:
    left_cells = _figure_cells(left, arrange)
    right_cells = _figure_cells(right, arrange)
    frames, anonymous = _merge_frames(left, right, left_cells, right_cells)
    return Page(arrange, left_cells + right_cells, frames, anonymous=anonymous)


def _channel_def(atom: Atom) -> Dict[str, Any]:
    """A binding: the column, plus optionally how its numbers become positions.

    ``limits`` is the domain the channel runs over when the data is not the
    authority (spec §10) — two numbers with ``None`` for an end the data should
    decide, which is already the engine's ``[0, null]``.
    """
    return {
        "field": atom.fields["field"],
        "scale": atom.fields.get("scale"),
        "base": atom.fields.get("base"),
        "limits": atom.fields.get("limits"),
        "tick_count": atom.fields.get("tick_count"),
        "speed": atom.fields.get("speed"),
        "free": atom.fields.get("free", False),
    }


# ---------------------------------------------------------------------------
# facet — `|` puts panels side by side, `/` stacks them
# ---------------------------------------------------------------------------


def _facet_join(left: Any, right: Any, slot: str, operator: str) -> Any:
    other = "row" if slot == "col" else "col"

    # Two figures: composition, not faceting. The operators tell the two apart by
    # what is on their right — a facet split takes an atom, a page takes another
    # plot — which is the door the design left open when `plot | plot` still
    # refused (spec §11).
    if isinstance(left, (Plot, Page)) and isinstance(right, (Plot, Page)):
        return _compose(left, right, "beside" if slot == "col" else "below")

    if isinstance(left, Page):
        raise GogError(
            f"gog: `{operator}` faceted a page of plots, and a facet splits *one* plot "
            f"by a column. Facet the plots before composing them: "
            f"`(plot {operator} facet(col.g)) {operator} other_plot`."
        )

    if isinstance(left, Atom):
        # The operator reached an atom instead of the plot. Two legitimate ways
        # in: an inner pair (`facet(a) / facet(b)`), whose slots the *outer*
        # operator assigns; and `y(col.b) / facet(col.g)`, where `/` bound
        # tighter than `+` and took the atom written just before it.
        if isinstance(right, Atom) and right.kind == "facet":
            if left.kind == "facet":
                return Atom(
                    "facet_pair",
                    first=left.fields["field"],
                    second=right.fields["field"],
                    wrap=left.fields.get("wrap") or right.fields.get("wrap"),
                )
            if left.kind == "facet_pair":
                raise GogError(
                    "gog: a plot crosses at most two facet columns — one for the panel "
                    "rows, one for the columns."
                )
            return Atom(
                "atom_then_facet",
                atom=left,
                facet=right.fields["field"],
                slot=slot,
                wrap=right.fields.get("wrap"),
            )
        raise GogError(
            f"gog: `{operator}` facets a *plot* — build the plot first, then facet it: "
            f"`data(df) + point + x(col.a) + y(col.b) {operator} facet(col.g)`."
        )

    if not isinstance(left, Plot):
        raise GogError(
            f"gog: `{operator}` facets a gog plot, and the left side is not one. Start "
            f"the sentence with `data()`: `data(df) + point + x(col.a) + y(col.b) "
            f"{operator} facet(col.g)`."
        )

    if not isinstance(right, Atom) or right.kind not in ("facet", "facet_pair"):
        raise GogError(
            f"gog: the right side of `{operator}` must be `facet(col.<name>)`."
        )

    plot = left._copy()
    plot.spec.setdefault("facet", {"col": None, "row": None})
    if right.kind == "facet":
        plot.spec["facet"][slot] = right.fields["field"]
    else:
        # `plot | facet(a) / facet(b)`: the operator's own slot takes the first
        # column written, the other slot the second — left to right, as read.
        plot.spec["facet"][slot] = right.fields["first"]
        plot.spec["facet"][other] = right.fields["second"]
    # The count rides with the column it was written on; which way the line runs
    # is the operator's, already settled. Carried even onto a crossing, where the
    # engine refuses it with the reason — dropping a binding in silence is what
    # spec §12 forbids.
    if right.fields.get("wrap") is not None:
        plot.spec["facet"]["wrap"] = right.fields["wrap"]
    return plot
