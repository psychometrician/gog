# translate.py — one gog sentence, from R spelling to Python spelling
#
# The claim this file exists to make checkable: an R user moving to Python is
# not learning a second library, they are applying a handful of *mechanical*
# rewrites. So the rewrites are written out, one function each, and the harness
# reports which ones fired across the manual's 481 sentences. A rule set that
# stays this short over real code is the evidence for "the transition is
# seamless"; a sixth rule appearing would be the evidence against.
#
# The rules, in the order they apply:
#
#   1. a column becomes `col.name`        x(gdp)          → x(col.gdp)
#   2. R's literals become Python's       desc = TRUE     → desc=True
#                                         limits = c(0, NA) → [0, None]
#   3. a vector becomes a list            c("a", "b")     → ["a", "b"]
#   4. e is spelled from the library      exp(1)          → math.e
#   5. a multi-line sentence takes parens `a +\n b`       → `(a +\n b)`
#
# Plus two shapes that are R code around a sentence rather than the sentence:
# an assignment prefix (`p <- …`) is dropped, and an inline `data.frame(…)`
# becomes a dict. What is deliberately NOT translated is the R pipe: `df |>
# data()` is a host-language idiom the R chapter exists to document, and Python
# has no pipe to answer it with, so those sentences are reported as
# language-specific rather than silently mangled.

import ast
import re
from typing import List, Optional, Tuple

# Every atom whose first argument is a column. `bounds` and `partition` are the
# exceptions that take columns in *every* argument, which is why they are listed
# apart. **A missing name here fails silently**: the translator does not decline,
# it just leaves the bare names alone, and the tab system reports nothing because
# translation "succeeded". `partition` was added 2026-07-27 after exactly that —
# five sentences whose Python tab named undefined variables and whose parity run
# counted them as a missing table.
COLUMN_ATOMS = {
    "x", "y", "z", "color", "size", "shape", "opacity", "label", "pattern",
    "group", "facet", "order", "play",
    "brush",
    # `colour` is exported only to be refused, but it takes its argument the
    # way `color` does, so the accessor must still be added or the sentence
    # fails on the *column* name and never reaches the refusal being compared.
    "colour",
}
ALL_COLUMN_ARGS = {"bounds", "partition"}

# The exception to the exception: a *knob* on an atom whose other arguments are
# all columns. `partition(city, mode, cross = TRUE)` names two columns and sets
# one flag, and without this the flag is rewritten to `col.TRUE` — `TRUE` being a
# perfectly good Python identifier, so nothing complains until the sentence is
# executed. Keyed by atom, because `cross` would be a column name anywhere else.
KNOBS = {"partition": {"cross"}}


def _is_bare_name(text: str) -> bool:
    """Is this argument a bare column name rather than a value?

    `str.isidentifier()` rather than an ASCII pattern, because both languages
    take Unicode column names and the book has a Korean example — `x(지역)`
    becomes `col.지역`, which is a legal Python attribute. An ASCII rule quietly
    left that sentence untranslated, which is the kind of gap a harness is
    supposed to find rather than create.
    """
    return text.isidentifier() or (
        # R allows a dot in a name where Python does not; `col["a.b"]` is the
        # accessor's escape hatch for exactly that.
        bool(re.match(r"^[A-Za-z.][\w.]*$", text)) and "." in text
    )


def _accessor(name: str) -> str:
    """`col.gdp`, or `col["a.b"]` when Python cannot spell the name as an attribute."""
    return f"col.{name}" if name.isidentifier() else f'col["{name}"]'


def _spans(text: str):
    """Walk the text, yielding (index, char, in_string) so rewrites skip strings.

    A title is data — `title("point * jitter(0.5)")` contains what looks like a
    call and is a sentence about one. Every rewrite below consults this.
    """
    quote = None
    for index, char in enumerate(text):
        if quote:
            yield index, char, True
            if char == quote and text[index - 1] != "\\":
                quote = None
        else:
            if char in "\"'":
                quote = char
                yield index, char, True
            else:
                yield index, char, False


def _match_paren(text: str, open_index: int) -> int:
    """Index of the `)` closing the `(` at open_index."""
    depth = 0
    for index, char, in_string in _spans(text):
        if index < open_index or in_string:
            continue
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return index
    raise ValueError("unbalanced parentheses")


def _split_args(text: str) -> List[str]:
    """Split an argument list on top-level commas."""
    args, depth, start = [], 0, 0
    for index, char, in_string in _spans(text):
        if in_string:
            continue
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        elif char == "," and depth == 0:
            args.append(text[start:index])
            start = index + 1
    args.append(text[start:])
    return args


def _calls(text: str, names):
    """Every call to one of `names`, innermost last, as (start, open, close)."""
    found = []
    for match in re.finditer(r"\b([A-Za-z_][A-Za-z0-9_.]*)\s*\(", text):
        if match.group(1) not in names:
            continue
        open_index = match.end() - 1
        if any(i == open_index and s for i, _, s in _spans(text)):
            continue  # inside a string
        found.append((match.start(), open_index, _match_paren(text, open_index)))
    return found


def _columns(text: str, fired: List[str]) -> str:
    """Rule 1 — the argument that names a column gets the accessor."""
    for start, open_index, close in reversed(_calls(text, COLUMN_ATOMS | ALL_COLUMN_ARGS)):
        name = text[start:open_index].strip()
        args = _split_args(text[open_index + 1:close])
        every = name in ALL_COLUMN_ARGS
        rewritten, positional = [], 0
        for arg in args:
            body = arg.strip()
            keyword = re.match(r"^([A-Za-z_][A-Za-z0-9_.]*)\s*=\s*(.*)$", body, re.S)
            if keyword and keyword.group(1) in KNOBS.get(name, ()):
                rewritten.append(body)
                continue
            if keyword and (every or keyword.group(1) in ("lower", "upper", "start", "end")):
                if _is_bare_name(keyword.group(2).strip()):
                    rewritten.append(f"{keyword.group(1)}={_accessor(keyword.group(2).strip())}")
                    fired.append("column accessor")
                    continue
            if not keyword:
                positional += 1
                first = positional == 1
                if (every or first) and _is_bare_name(body):
                    rewritten.append(_accessor(body))
                    fired.append("column accessor")
                    continue
            rewritten.append(body)
        text = text[:open_index + 1] + ", ".join(rewritten) + text[close:]
    return text


def _literals(text: str, fired: List[str]) -> str:
    """Rule 2 — TRUE/FALSE/NULL/NA are True/False/None/None.

    `NA` joined the list when scale limits shipped: `limits = c(0, NA)` leaves
    an end to the data, and Python's word for that absent end is the same `None`
    it uses for `NULL`. One more entry in this tuple rather than a sixth rule —
    the two R words differ in what they mean *to R*, and both cross the wire as
    JSON `null`, which is the only thing the engine sees.
    """
    out, changed = [], False
    for index, char, in_string in _spans(text):
        out.append(char)
        del index, char, in_string
    body = "".join(out)
    for r_word, py_word in (("TRUE", "True"), ("FALSE", "False"),
                            ("NULL", "None"), ("NA", "None")):
        pattern = re.compile(rf"\b{r_word}\b")
        pieces, last = [], 0
        for match in pattern.finditer(body):
            if any(i == match.start() and s for i, _, s in _spans(body)):
                continue
            pieces.append(body[last:match.start()] + py_word)
            last = match.end()
            changed = True
        pieces.append(body[last:])
        body = "".join(pieces)
    if changed:
        fired.append("R literal")
    return body


def _vectors(text: str, fired: List[str]) -> str:
    """Rule 3 — `c(a, b)` is a list."""
    for start, open_index, close in reversed(_calls(text, {"c"})):
        text = text[:start] + "[" + text[open_index + 1:close] + "]" + text[close + 1:]
        fired.append("vector to list")
    return text


def _constants(text: str, fired: List[str]) -> str:
    """Rule 4 — R has no `e`, Python keeps it in `math`."""
    if "exp(1)" in text:
        fired.append("math.e")
        return text.replace("exp(1)", "math.e")
    return text


def _inline_frames(text: str, fired: List[str]) -> str:
    """A table written into the sentence becomes a dict, keeping R's name for it.

    `data(data.frame(life = 50))` names the table by its own source text, which
    is what lets the book stack four anonymous tables in one plot (spec §8). The
    Python table has no such name to take, so the R spelling is passed through
    as `name=` — otherwise the two bindings would build the same plot over
    differently-named tables.
    """
    for start, open_index, close in reversed(_calls(text, {"data.frame"})):
        body = _frame_columns(text[open_index + 1:close])
        if body is None:
            return text  # not a literal frame; leave it to fail loudly
        original = text[start:close + 1]
        text = (text[:start] + body + ", name=" + repr(original) +
                text[close + 1:])
        fired.append("inline table")
    return text


def _frame_columns(args: str) -> Optional[str]:
    """The `{...}` body of a `data.frame(...)` argument list, or None.

    A column is a vector at every length, so each value is bracketed — except
    when it is already a `c(...)`, which `_vectors` brackets later. Wrapping
    both produced `{"freq": [[55.0, 110.0]]}`, a column holding one list
    instead of two numbers. It stayed hidden because every inline frame in the
    book had scalar columns (`data.frame(life = 50)`), where the single wrap is
    right; it surfaced the moment a *named* table with `c()` columns was
    translated. The two R-side emitters already test `startsWith(inner, "[")`
    for the same reason.
    """
    columns = []
    for arg in _split_args(args):
        keyword = re.match(r"^\s*([A-Za-z_.][A-Za-z0-9._]*)\s*=\s*(.*)$", arg, re.S)
        if not keyword:
            return None
        value = keyword.group(2).strip()
        cell = value if re.match(r"^c\s*\(", value) else f"[{value}]"
        columns.append(f'"{keyword.group(1)}": {cell}')
    return "{" + ", ".join(columns) + "}"


def translate(source: str) -> Tuple[Optional[str], List[str], Optional[str]]:
    """The sentence in Python, the rules that fired, or why it does not carry over."""
    fired: List[str] = []

    # R comments are not part of the sentence.
    body = "\n".join(
        line.split("#")[0].rstrip() if "#" in line and '"' not in line.split("#")[0] else line
        for line in source.splitlines()
    ).strip()

    if "|>" in body or "%>%" in body:
        return None, fired, "R pipe — a host-language idiom with no Python spelling"

    # R's own extractor on a table, as in `df[order(df$pop), ]`. The R chapter's
    # masked-names section says what happens when `order` is gog's rather than
    # base R's, which is host arithmetic and not a sentence: there is nothing
    # here for another language to spell. Matched as *syntax* (a name followed by
    # `[`) rather than by the characters, because `$` also appears inside a
    # legitimate title string ("Under $30,000") that must keep translating.
    if re.search(r"[A-Za-z0-9._)\"]\s*\[", body):
        return None, fired, "R subsetting — a host-language idiom with no Python spelling"

    # A chunk that sets its own small table up before saying its sentence keeps
    # the table's *name*, because the sentence below is about to use it. Only
    # the spec assignment (`p <- data(…) + …`) is droppable: there the name is
    # R's bookkeeping and the sentence is the whole content.
    table_def = re.match(
        r"^\s*([A-Za-z._][A-Za-z0-9._]*)\s*<-\s*data\.frame\s*\((.*)\)\s*$",
        body, re.S)
    if table_def:
        columns = _frame_columns(table_def.group(2))
        if columns is None:
            return None, fired, "table built by something other than column literals"
        columns = _literals(_vectors(columns, fired), fired)
        # The output must be a Python *literal*, and this proves it rather than
        # trusting the rewrites. `decay <- data.frame(hours = 0:5 + 0.0)` passes
        # every regex above and yields `{"hours": [0:5 + 0.0]}`, which is not
        # Python at all — a tab that lies is worse than a chunk with no tab, so
        # a table built by R computation is blocked and counted as a gap.
        try:
            ast.literal_eval(columns)
        except (ValueError, SyntaxError):
            return None, fired, "table computed in R, not written out as literal columns"
        fired.append("named table")
        return f"{table_def.group(1)} = {columns}", fired, None

    assignment = re.match(r"^\s*[A-Za-z._][A-Za-z0-9._]*\s*<-\s*(.*)$", body, re.S)
    if assignment:
        body = assignment.group(1)
        fired.append("assignment dropped")

    body = _inline_frames(body, fired)
    body = _columns(body, fired)
    body = _vectors(body, fired)
    body = _literals(body, fired)
    body = _constants(body, fired)

    if "\n" in body:
        body = "(" + body + ")"
        fired.append("parentheses for continuation")

    return body, fired, None
