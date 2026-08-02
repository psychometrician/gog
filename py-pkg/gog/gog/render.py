# render.py — the bridge: a table to the wire, a spec to the CLI, an SVG back
#
# The mirror of `r-pkg/gog/R/render.R`, and deliberately the same shape: find
# the binary, turn each table into the column-oriented wire form, hand
# `{spec, data}` to `gog-cli` on stdin, read the SVG off stdout and the
# diagnostics off stderr. No policy lives here. Which plots are legal, what a
# missing value does to a row, what `GOG_STRICT` means — all of that is
# `gog-core`'s, because a rule implemented in a binding is a rule the other
# bindings will get wrong (spec §14).

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import uuid
import webbrowser
from datetime import date, datetime
from typing import Any, Dict, List, Optional, Tuple

from .errors import GogError

# ---------------------------------------------------------------------------
# Find the gog-cli binary
# ---------------------------------------------------------------------------


def _bundled_cli() -> Optional[str]:
    """The engine shipped inside this wheel, if this is an installed copy.

    A released wheel carries `gog-cli` for its own platform, built by CI, so an
    installed package draws on a machine with no Rust toolchain and no checkout.
    A development copy has no `_bin/` and falls through to the build below.

    The executable bit is restored if it went missing: a zip records the mode,
    but not every tool that repacks or copies a wheel preserves it, and the
    failure that produces (`Permission denied` from a subprocess) says nothing
    about what to do.
    """
    binary = os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "_bin",
        "gog-cli.exe" if os.name == "nt" else "gog-cli",
    )
    if not os.path.isfile(binary):
        return None
    if os.name != "nt" and not os.access(binary, os.X_OK):
        try:
            os.chmod(binary, os.stat(binary).st_mode | 0o111)
        except OSError:
            return None
    return binary


def find_gog_cli() -> str:
    """Locate the engine: an override, the shipped one, PATH, then a local build."""
    env_path = os.environ.get("GOG_CLI_PATH", "")
    if env_path and os.path.isfile(env_path):
        return env_path

    # Before PATH, because the binary that shipped with this package is the one
    # whose wire format matches it. An unrelated `gog-cli` earlier on PATH would
    # otherwise silently answer for it.
    bundled = _bundled_cli()
    if bundled:
        return bundled

    on_path = shutil.which("gog-cli")
    if on_path:
        return on_path

    exe = "gog-cli.exe" if os.name == "nt" else "gog-cli"
    # Walk up from this file as well as from the working directory, so a plot
    # drawn from anywhere inside the repo finds the build.
    roots = [os.getcwd()]
    here = os.path.dirname(os.path.abspath(__file__))
    for _ in range(5):
        here = os.path.dirname(here)
        roots.append(here)
    for root in roots:
        for build in ("release", "debug"):
            candidate = os.path.join(root, "target", build, exe)
            if os.path.isfile(candidate):
                return candidate

    raise GogError(
        "gog: cannot find the `gog-cli` binary — the engine that draws the plot.\n"
        "  Build it:  cargo build --release -p gog-cli\n"
        "  Or point at one:  os.environ['GOG_CLI_PATH'] = '/path/to/gog-cli'"
    )


# ---------------------------------------------------------------------------
# A Python table → the wire
#
# The wire form is column-oriented and typed by which map a column lands in:
#
#   floats   {"gdp": [1.0, 2.0, null]}      numbers, and temporal values
#   strings  {"continent": ["Asia", null]}  text
#   levels   {"size": ["Low", "High"]}      a declared category order
#   dates    {"day": "day"}                 a column of floats read as time
#
# Two tables are accepted, for one reason each. A dict of lists is Python with
# nothing installed, which is what a smoke test and a first plot should need.
# Anything carrying `.columns` and `df[name]` is pandas (and polars, and
# whatever else adopts the shape) — duck-typed rather than imported, so the
# binding keeps the engine's promise of depending on nothing.
# ---------------------------------------------------------------------------


def _is_missing(value: Any) -> bool:
    """None, NaN, NaT, pandas NA — everything that means 'no value here'."""
    if value is None:
        return True
    try:
        return bool(value != value)
    except Exception:
        # pandas' NA is not even comparable to itself without raising; a value
        # that cannot answer "am I myself" is missing by any useful reading.
        return True


def _as_number(value: Any) -> Optional[float]:
    """The value as a float, or None if it is not a number.

    A bool is deliberately not a number. R's `is.numeric(TRUE)` is FALSE and a
    logical column crosses as text, so a flag is a category in both bindings —
    two colors, a legend with two rows, not an axis running 0 to 1.
    """
    if isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        return float(value)
    # numpy scalars are not int/float subclasses on every platform, but they do
    # convert; str/bytes define no __float__, so they cannot slip through here.
    if hasattr(value, "__float__") and not isinstance(value, (str, bytes)):
        try:
            return float(value)
        except (TypeError, ValueError):
            return None
    return None


def _epoch_seconds(value: Any) -> float:
    """Seconds since 1970-01-01, the engine's one temporal unit.

    Timezone-naive on purpose, as in the R binding: the engine draws the clock
    time the reader sees, so an aware datetime contributes its own wall clock
    and no zone survives the trip to disagree with it later.
    """
    if isinstance(value, datetime):
        naive = value.replace(tzinfo=None)
        days = naive.date().toordinal() - date(1970, 1, 1).toordinal()
        seconds = naive.hour * 3600 + naive.minute * 60 + naive.second
        return days * 86400.0 + seconds + naive.microsecond / 1e6
    return (value.toordinal() - date(1970, 1, 1).toordinal()) * 86400.0


class Ordered(list):
    """A column that remembers what order its categories go in.

    A `list` subclass so it *is* a column everywhere one is read — iterated,
    measured, indexed — carrying the declared order as one extra attribute.
    """

    def __init__(self, values: Any, levels: Any) -> None:
        super().__init__(values)
        self.levels = [str(level) for level in levels]


def ordered(values: Any, levels: Any) -> "Ordered":
    """Declare a category column's order — Python's answer to R's `factor()`.

    A pandas `Categorical` already says this, and reading one is why `to_wire`
    looks for `.cat.categories`. But **pandas is deliberately not a dependency**
    (see `pyproject.toml`): the binding's own advertised table is a dict of
    lists, and until this shipped that table had *no way at all* to declare an
    order. JavaScript and Julia each grew this same helper for the same reason —
    neither language has a factor type either — and Python was left out on the
    grounds that pandas has `Categorical`, which is true only for the users who
    happen to have pandas.

    A host-language word like `col`, not a word of the grammar: dropping the
    declared order would make an ordered-category plot fall back to the order of
    the rows and say nothing, which is the silent drop §12 forbids.

        ordered(["Low", "High"], ["Low", "Mid", "High"])
    """
    if not isinstance(values, (list, tuple)) or not isinstance(levels, (list, tuple)):
        raise GogError(
            "gog: `ordered()` takes the column's values and its category order — "
            '`ordered(["Low", "High"], ["Low", "Mid", "High"])`.'
        )
    return Ordered(values, levels)


def _column_values(frame: Any, name: str, table: str) -> Tuple[List[Any], Optional[List[str]]]:
    """One column as a plain list, plus its declared category order if it has one."""
    series = frame[name]

    # A pandas Categorical is Python's factor: the categories are a *declared*
    # order, and dropping them is the bug the R binding's `levels` map exists to
    # stop — the chart falls back to the order of the rows and says nothing.
    # Both kinds count, ordered or not: plenty of tables use a categorical
    # purely to fix display order and mean nothing mathematical by it.
    # `ordered()` is the same declaration for a table that is only lists.
    categories = None
    cat = getattr(series, "cat", None)
    if cat is not None and hasattr(cat, "categories"):
        categories = [str(c) for c in cat.categories]
    elif isinstance(series, Ordered):
        categories = list(series.levels)

    if hasattr(series, "tolist"):
        values = series.tolist()
    elif isinstance(series, (list, tuple)):
        values = list(series)
    else:
        try:
            values = list(series)
        except TypeError:
            raise GogError(
                f"gog: column `{name}` of `{table}` is not a column — a column is a "
                f"list of values, one per row. A single value is a length-1 list: "
                f"`{{'{name}': [{series!r}]}}`."
            ) from None
    return values, categories


def _frame_columns(frame: Any, table: str) -> List[str]:
    if isinstance(frame, dict):
        return list(frame.keys())
    columns = getattr(frame, "columns", None)
    if columns is not None:
        return [str(c) for c in columns]
    raise GogError(
        "gog: `data()` takes a table — a dict of columns "
        "(`{'x': [1, 2], 'y': [3, 4]}`) or a DataFrame (pandas, polars). "
        f"Got {type(frame).__name__}."
    )


class Query:
    """A table named by a SQL query instead of held in memory.

    **Deliberately not executed when it is written.** A query that ran at the
    moment the sentence was built would foreclose pushing the transform down to
    the database, because the planner has to see the whole sentence before it can
    know what to ask for. So this holds the connection and the text, and
    `resolve()` runs exactly once, at render.

    The connection is the caller's own object and gog never opens one: no
    credentials, no driver dependency, no socket of its own. That is the same
    duck-typing that lets `data()` take a pandas frame without pandas being a
    dependency, one level out.
    """

    __slots__ = ("connection", "sql")

    def __init__(self, connection: Any, sql: str) -> None:
        self.connection = connection
        self.sql = sql

    def __repr__(self) -> str:
        text = self.sql if len(self.sql) <= 40 else self.sql[:37] + "..."
        return f"query({text!r})"

    def resolve(self, table: str) -> Dict[str, List[Any]]:
        """Run the query and return a dict of columns — a table `to_wire` eats."""
        return _rows_from_connection(self.connection, self.sql, table)


def _rows_from_connection(con: Any, sql: str, table: str) -> Dict[str, List[Any]]:
    """Run `sql` on `con`, as a dict of columns.

    Two protocols, tried in this order, and **the order is not arbitrary**:
    DuckDB satisfies both, and only its PEP 249 side returns rows (its `.sql()`
    hands back a relation, not a frame). A `SparkSession` has no `.cursor` at
    all, so the two cases never overlap — checked rather than assumed.

    1. **PEP 249**, Python's database API — `sqlite3`, DuckDB, `psycopg`,
       `pyodbc`, `databricks-sql-connector`, Snowflake. The standard, so it goes
       first.
    2. **Spark** — a `SparkSession`, whose `.sql()` returns a DataFrame that
       `.toPandas()` collects. This is the Databricks route, where the table is
       a Unity Catalog table and the session is already in the notebook.
    """
    if hasattr(con, "cursor"):
        cursor = con.cursor()
        try:
            cursor.execute(sql)
            if cursor.description is None:
                raise GogError(
                    f"gog: the query for `{table}` returned no columns. `query()` "
                    f"takes a SELECT — a statement that produces a table."
                )
            names = [str(d[0]) for d in cursor.description]
            rows = cursor.fetchall()
        finally:
            close = getattr(cursor, "close", None)
            if close is not None:
                close()
        return {name: [row[i] for row in rows] for i, name in enumerate(names)}

    if hasattr(con, "sql"):
        frame = con.sql(sql)
        collect = getattr(frame, "toPandas", None)
        if collect is None:
            raise GogError(
                f"gog: `query()` ran `{table}` on a Spark-shaped connection, but its "
                f"result has no `toPandas()`, so the rows cannot be collected."
            )
        return collect()

    raise GogError(
        "gog: `query()` takes a database connection and a SELECT — "
        "`query(con, 'SELECT ...')`. The connection must be either a PEP 249 one "
        "(`sqlite3`, DuckDB, `psycopg`, `databricks-sql-connector` — anything with "
        "`.cursor()`) or a Spark session (`.sql()`). "
        f"Got {type(con).__name__}, which is neither. If the rows are already in "
        "hand, that is a table: `data(df)`."
    )


def to_wire(frame: Any, table: str) -> Dict[str, Any]:
    """A table in the engine's column-oriented wire form."""
    floats: Dict[str, List[Optional[float]]] = {}
    strings: Dict[str, List[Optional[str]]] = {}
    levels: Dict[str, List[str]] = {}
    dates: Dict[str, str] = {}

    for name in _frame_columns(frame, table):
        values, categories = _column_values(frame, name, table)
        present = [v for v in values if not _is_missing(v)]

        # A column is one type — the engine's table has a `Float` column and a
        # `Str` column and nothing that is both. Deciding by majority, or by the
        # first row, would be the silent drop §12 forbids one level down, so a
        # mixed column is refused here where the caller can still see which
        # column it was.
        if present and all(isinstance(v, datetime) for v in present):
            floats[name] = [None if _is_missing(v) else _epoch_seconds(v) for v in values]
            dates[name] = "second"
        elif present and all(isinstance(v, date) for v in present):
            floats[name] = [None if _is_missing(v) else _epoch_seconds(v) for v in values]
            dates[name] = "day"
        elif all(_is_missing(v) or _as_number(v) is not None for v in values):
            floats[name] = [None if _is_missing(v) else _as_number(v) for v in values]
        elif all(_is_missing(v) or _as_number(v) is None for v in values):
            strings[name] = [None if _is_missing(v) else str(v) for v in values]
            if categories:
                levels[name] = categories
        else:
            kinds = sorted({type(v).__name__ for v in present})
            raise GogError(
                f"gog: column `{name}` of `{table}` mixes {' and '.join(kinds)} — a "
                f"column is one type, because a scale reads it as one kind of thing. "
                f"Make it numbers (a position, a magnitude) or text (a category)."
            )

    return {"floats": floats, "strings": strings, "levels": levels, "dates": dates}


# ---------------------------------------------------------------------------
# Render to an SVG string
# ---------------------------------------------------------------------------


def render_svg(plot: Any) -> str:
    """Draw a plot and return the SVG as a string."""
    spec, frames = plot._wire()
    # A `query()` table is resolved here and nowhere else — one place, at render,
    # which is what leaves room for the planner to rewrite the sentence before
    # the database is ever asked (the pushdown design).
    frames = {
        name: (frame.resolve(name) if isinstance(frame, Query) else frame)
        for name, frame in frames.items()
    }
    request = {
        "spec": spec,
        "data": {name: to_wire(frame, name) for name, frame in frames.items()},
    }

    # allow_nan=False is a backstop, not a policy: a NaN reaches the wire as the
    # bare token `NaN`, which is not JSON and which serde rejects with a parse
    # error naming a byte offset. Every missing value has already become `null`
    # in `to_wire`, so this can only fire on a bug here — and failing loudly
    # beats handing the engine something it must guess at.
    payload = json.dumps(request, allow_nan=False)

    result = subprocess.run(
        [find_gog_cli()],
        input=payload,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )

    messages = result.stderr.strip()
    if result.returncode != 0:
        # The diagnostics *are* the error. Surfacing them as-is rather than
        # wrapping them in an exit-code message keeps the direction the engine
        # wrote (spec §12).
        raise GogError(messages or f"gog-cli exited with status {result.returncode}")

    # Non-fatal diagnostics — an Assumption, a dropped row — belong beside the
    # plot, not inside it: stderr, exactly where the engine put them.
    if messages:
        print(messages, file=sys.stderr)

    return result.stdout


# ---------------------------------------------------------------------------
# Showing the plot — a notebook, a file, a browser
# ---------------------------------------------------------------------------


# Where the browser assets live: the WebAssembly engine, and the module that
# drives it. Overridable the way the CLI path is, and for the same reason — a
# book wants one cached file beside its HTML where a notebook wants the bytes
# carried inside it.
WASM_URL: Optional[str] = os.environ.get("GOG_WASM_URL") or None
JS_URL: Optional[str] = os.environ.get("GOG_JS_URL") or None


def _find_wasm_assets() -> Optional[Tuple[str, str]]:
    """The engine and its runtime, or None — in which case plots stay static.

    Searched the way `find_gog_cli` searches for the binary: an installed copy
    carries its own, and a checkout is *walked up to* rather than counted, since
    the distance to the root differs between a script, a notebook and a test.
    """
    here = os.path.dirname(os.path.abspath(__file__))
    bundled = (os.path.join(here, "_www", "gog.wasm"),
               os.path.join(here, "_www", "interactive.js"))
    if all(os.path.exists(p) for p in bundled):
        return bundled

    for start in (os.getcwd(), here):
        root = os.path.abspath(start)
        for _ in range(7):
            pair = (
                os.path.join(root, "gog-wasm", "target", "wasm32-unknown-unknown",
                             "release", "gog_wasm.wasm"),
                os.path.join(root, "js-pkg", "gog", "src", "interactive.js"),
            )
            if all(os.path.exists(p) for p in pair):
                return pair
            parent = os.path.dirname(root)
            if parent == root:
                break
            root = parent
    return None


def _data_uri(path: str, mime: str) -> str:
    """A file as a `data:` URI — the only form that survives being emailed."""
    import base64

    with open(path, "rb") as handle:
        return f"data:{mime};base64," + base64.b64encode(handle.read()).decode("ascii")


def _inline_modules(paths: List[str]) -> str:
    """The modules' own source, ready to sit inside `<script type="module">`.

    **A `data:` URL cannot be imported where a page has a content-security
    policy**, and every host that shows a plot outside a plain browser has one:
    JupyterLab, VS Code notebooks, and the Positron and RStudio viewer panes.
    `script-src` there does not list `data:`, so importing the module from one is
    refused, silently, because a blocked module import throws nothing the page
    can catch. The plot still drew, since the SVG is markup, and every control
    was missing. Inlining the source survives that policy: an inline module runs
    under `script-src 'unsafe-inline'`, and needs no URL of any scheme.
    """
    src = "\n".join(open(p, encoding="utf-8").read() for p in paths)
    # `interactive.js` takes its view helpers from the sibling file. Inlined,
    # that specifier has nothing to resolve against, and both files are already
    # in this one scope, so the two statements naming it go.
    return re.sub(r'(?:import|export)\s*\{[^}]*\}\s*from\s*"\./view\.js";?', "", src)


def _wasm_expression(path: str) -> str:
    """The engine as a JavaScript expression evaluating to its bytes.

    `loadEngine()` takes a URL *or* a BufferSource, so this is the second of the
    two: no fetch, no scheme, nothing the policy can refuse.
    """
    import base64

    if WASM_URL:
        return f'"{WASM_URL}"'
    with open(path, "rb") as handle:
        b64 = base64.b64encode(handle.read()).decode("ascii")
    return f'Uint8Array.from(atob("{b64}"), c => c.charCodeAt(0))'


def _module_specifier(url: str) -> str:
    """An `import` needs a module specifier, which is stricter than a URL.

    A bare word like `"gog.js"` is reserved for import maps, so a browser
    refuses it outright: the script never runs, no asset is requested, and the
    page shows the static plot with nothing in the console explaining why. That
    silence is why this normalizes rather than documents — a bare filename is
    the natural thing to configure and the one spelling that fails.
    """
    if re.match(r"^(data:|https?:|file:|/|\./|\.\./)", url):
        return url
    return "./" + url


def _needs_engine(spec: Dict[str, Any]) -> bool:
    """Two reasons to carry the engine, not one. A plot in the cube has an angle worth dragging; a plot that names a brush has a bound worth moving. A flat plot with neither stays a still image and pays nothing."""
    return _is_spatial(spec) or bool(spec.get("brush"))


def _is_spatial(spec: Dict[str, Any]) -> bool:
    """Does this spec draw in the cube?

    The twin of `isSpatial` in the browser module and of `space_of` in the
    engine. A bound `z` projects a plot even when the coordinate still reads
    "flat", so naming `space()` is sufficient and not necessary.
    """
    coord = spec.get("coord")
    if isinstance(coord, dict) and coord.get("space") is not None:
        return True
    if spec.get("z") is not None:
        return True
    for layer in spec.get("layers") or []:
        if (layer.get("encodings") or {}).get("z") is not None:
            return True
    # A page: one cell in the cube makes the page carry the engine.
    return any(_needs_engine(cell) for cell in (spec.get("plots") or []))


def _interactive_block(plot: Any, container_id: str) -> str:
    """The script that upgrades a static cube into a turnable one, or ""."""
    try:
        spec, frames = plot._wire()
    except Exception:
        return ""
    # **`_wire()` hands back raw frames, not wire tables.** `render_svg` converts
    # them with `to_wire` before the engine sees them and this did not, so the
    # browser engine received a bare frame, read each column *name* where a type
    # group belongs, and refused every column. Static drawing was unaffected,
    # because that path goes through `render_svg`, which is why it survived: the
    # SVG harness compares pictures the CLI drew and never reaches this block.
    # It broke 3-D, `brush` and `play` for every Python user.
    data = {name: to_wire(frame, name) for name, frame in frames.items()}
    # Two questions, not one. Carrying the *engine* has two reasons — an angle
    # worth dragging, a bound worth moving — and both redraw. Carrying the
    # *module* has a third, and it is every plot: looking closer. A zoom scales
    # the SVG's viewBox and recomputes nothing, so it needs this file and not the
    # WebAssembly beside it, 65 KB against 861 KB.
    needs_engine = _needs_engine(spec)

    assets = _find_wasm_assets()
    if assets is None:
        return ""
    wasm_path, js_path = assets

    # A flat plot names the smaller module and sends no data: `mountView` takes a
    # container and stops, so the block is one line beside an 8 KB module where
    # naming `interactive.js` inlined 88 KB and the whole table again.
    view_path = os.path.join(os.path.dirname(js_path), "view.js")

    if not needs_engine:
        head = (
            f'import {{ mountView }} from '
            f'"{_module_specifier(JS_URL.replace("interactive.js", "view.js"))}";\n'
            if JS_URL
            else _inline_modules([view_path]) + "\n"
        )
        return (
            '\n<script type="module">\n'
            f"{head}"
            f'mountView("{container_id}");\n'
            "</script>\n"
        )

    # The module arrives one of two ways, and the engine likewise. A book names
    # files it serves; everything else carries them, because a notebook cell has
    # no server behind it and a temp page in a viewer pane has no directory.
    head = (
        f'import {{ mount }} from "{_module_specifier(JS_URL)}";\n'
        if JS_URL
        else _inline_modules([view_path, js_path]) + "\n"
    )

    request = json.dumps({"spec": spec, "data": data})
    return (
        '\n<script type="module">\n'
        f"{head}"
        f'mount("{container_id}", {request}, '
        f"{{ wasm: {_wasm_expression(wasm_path)} }});\n"
        "</script>\n"
    )


def svg_block(svg: str, plot: Any = None) -> str:
    """The SVG wrapped for an HTML host, sized to fit its column.

    A plot in the cube also gets the script that makes it turnable. The static
    SVG is still what is written, and it is what a reader sees in a PDF, in a
    viewer that strips JavaScript, and before the engine loads — the script only
    upgrades a picture that is already there.

    **The script goes inside the container**, which is a layout rule rather than
    a style choice. Quarto's `layout-ncol` divides a chunk's output into cells by
    counting top-level blocks, so a `<div>` with a sibling `<script>` is two
    cells and two plots become four — wrapping into two rows, each plot alone at
    full width beside an empty cell holding only its script. One element is one
    cell. Nothing else cares where it sits: the container is resolved by id, the
    SVG is still its first element, and a redraw can only remove a module script
    that has already run.
    """
    # **Whatever size the canvas is.** This matched the literal 800x600 for as
    # long as that was the only canvas, so `size()` on a plot quietly opted it
    # out of fitting. Anchored inside the opening `<svg` tag, because `[^>]`
    # cannot cross the tag's own `>` — which keeps it off the background `<rect>`
    # carrying the same two numbers a few characters later.
    svg = re.sub(
        r'(<svg[^>]*) width="(\d+)" height="(\d+)"',
        r'\1 width="\2" height="\3" style="max-width:100%;height:auto;"',
        svg,
        count=1,
    )
    container_id = "gog-" + uuid.uuid4().hex[:10]
    block = _interactive_block(plot, container_id) if plot is not None else ""
    if not block:
        return f'<div class="gog-plot" style="text-align:center;">\n{svg}\n</div>'
    return (
        f'<div class="gog-plot" id="{container_id}" style="text-align:center;">\n'
        f"{svg}\n{block}</div>"
    )


def save(plot: Any, path: str) -> str:
    """Draw the plot and write the SVG to `path`. Returns the path."""
    svg = render_svg(plot)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(svg)
    return path


def show(plot: Any) -> None:
    """Draw the plot and open it in the default browser (for scripts)."""
    html = (
        "<!DOCTYPE html>\n<html>\n<head><meta charset='utf-8'>"
        "<style>body{margin:0;background:#fff;display:flex;"
        "justify-content:center;padding:16px;}</style></head>\n<body>\n"
        f"{render_svg(plot)}\n</body>\n</html>"
    )
    handle = tempfile.NamedTemporaryFile(
        "w", suffix=".html", delete=False, encoding="utf-8"
    )
    with handle:
        handle.write(html)
    webbrowser.open(f"file://{handle.name}")
