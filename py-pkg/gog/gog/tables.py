"""The book's example tables, fetched by name.

Not a word of the grammar, and deliberately so. This is the same category as
``render_svg``: something the binding needs and the vocabulary does not.

It exists because every example in the manual begins with a table, and a reader
who wants to run one should not have to write a CSV reader first. The tables are
not shipped with the package; they are fetched from the book's own site, so one
copy serves all four languages and nothing goes stale inside a wheel.

The name carries the package's, and that is the whole of why it is no longer
``book_table()``. This package and ``god`` are built to be loaded together, so
``gog_table()`` and ``god_table()`` stand side by side at a prompt and read as
one idea in two spellings. They still differ by the one letter that separates
the two projects everywhere else, so neither masks the other.

The old name is gone rather than deprecated. An alias would have been the
careful move on a package with a readership, and this one does not have one yet:
the window where a rename costs nobody anything is open now and closes for good.
Two spellings of one function is a debt Law 3 would have carried until someone
finally removed it, so it was not taken on.
"""

import csv
import urllib.error
import urllib.request

from .errors import GogError

BOOK_DATA_URL = "https://psychometrician.github.io/gog-book/data/"
BOOK_DATA_CHAPTER = "https://psychometrician.github.io/gog-book/book-data.html"

__all__ = ["gog_table"]


def _table_names():
    """The names of the tables, read from the site rather than carried.

    A list shipped inside the wheel would be fixed at the version it shipped
    with, so the day a table is added an installed copy would deny a table that
    exists. That is the worst kind of refusal: confident and wrong. The site
    publishes the list beside the tables themselves, generated from the
    directory, so the answer is always the one the site can actually serve.

    Read only when a name has already failed, so the cost falls on the error
    path and never on a plot. Returns nothing rather than failing, because a
    diagnostic that can itself fail is not a diagnostic.
    """
    try:
        with urllib.request.urlopen(f"{BOOK_DATA_URL}tables.txt") as response:
            body = response.read().decode("utf-8")
    except Exception:
        return []
    return [line.strip() for line in body.splitlines() if line.strip()]


def _edit_distance(a, b):
    """Levenshtein distance, two-row variant — the engine's, in Python."""
    previous = list(range(len(b) + 1))
    for i, ca in enumerate(a, start=1):
        current = [i]
        for j, cb in enumerate(b, start=1):
            current.append(min(previous[j] + 1, current[j - 1] + 1,
                               previous[j - 1] + (ca != cb)))
        previous = current
    return previous[len(b)]


def _nearest_table(name, known):
    """The closest name, or ``None``.

    The rule is the engine's, which suggests a color the same way: within two
    edits, and fewer edits than the candidate has letters, so a short name
    cannot match everything. Deliberately conservative — a wrong suggestion
    sends the reader to a second wall, which is worse than the chapter.
    """
    lower = name.strip().lower()
    close = [(_edit_distance(lower, k), k) for k in known]
    close = [(d, k) for d, k in close if d <= 2 and d < len(k)]
    return min(close)[1] if close else None


def _unknown_table(name, known):
    """What to say about a name the site does not have.

    A near-miss is named on its own, because it is the whole answer. Without one
    the chapter is the answer, and the full list of names is not printed here:
    the engine declines a color the same way, naming the one candidate or
    pointing at the vocabulary, never reciting it.
    """
    near = _nearest_table(name, known)
    if near is not None:
        return f'gog: there is no table called "{name}". Did you mean "{near}"?'
    return (f'gog: there is no table called "{name}". The table names are '
            f"listed in the book's data chapter: {BOOK_DATA_CHAPTER}")


def _unreachable(name):
    """The site answered nothing at all — a different problem, said differently.

    Kept apart from the unknown-name refusal because the two ask opposite things
    of the reader: one is a name to correct, the other is a connection to check.
    Telling someone their table does not exist when the network is down is the
    confidently-wrong refusal this whole path exists to avoid.
    """
    return (f'gog: could not reach the book\'s data site to read "{name}". '
            f"The tables are fetched from {BOOK_DATA_URL}, so this needs a "
            "network connection.")


def _columns(rows, text=()):
    """Turn a list of CSV row dicts into columns, with the right types.

    A CSV is text, so every value arrives as text. A column becomes numbers when
    *every* value in it parses as one, and stays text otherwise. Naming a column
    in ``text`` keeps it text no matter what it looks like.
    """
    table = {}
    for key in rows[0]:
        values = [row[key] for row in rows]
        if key in text:
            table[key] = values
            continue
        try:
            table[key] = [float(value) for value in values]
        except ValueError:
            table[key] = values
    return table


def gog_table(name, text=()):
    """Read one of the book's example tables.

    Args:
        name: The table's name without the extension, such as
            ``"gapminder_2007"``. The full list is in the book's data chapter.
        text: Columns that must stay text. A CSV records what a value is and
            never what kind of thing it is, so a column of ``01``, ``02``, ``03``
            comes back as the numbers 1, 2, 3 unless it is named here.

    Returns:
        A dict of column name to list of values, ready for ``data()``.
    """
    if not isinstance(name, str):
        raise GogError(
            "gog: gog_table() takes one table name, as in "
            'gog_table("gapminder_2007"). The names are listed in the '
            "book's data chapter."
        )
    # A misspelt name is the commonest mistake this function has, and until the
    # refusal below it was answered by whichever words the host language happened
    # to use for a failed request. Python said `HTTPError`, which names neither
    # the table nor the fix, and which `except GogError` does not catch — so a
    # session that wraps the grammar's refusals missed this one entirely.
    try:
        with urllib.request.urlopen(f"{BOOK_DATA_URL}{name}.csv") as response:
            body = response.read().decode("utf-8")
    except urllib.error.HTTPError as error:
        if error.code == 404:
            raise GogError(_unknown_table(name, _table_names())) from None
        raise GogError(_unreachable(name)) from error
    except urllib.error.URLError as error:
        raise GogError(_unreachable(name)) from error
    return _columns(list(csv.DictReader(body.splitlines())), text)
