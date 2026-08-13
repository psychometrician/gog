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
import urllib.request

from .errors import GogError

BOOK_DATA_URL = "https://psychometrician.github.io/gog-book/data/"

__all__ = ["gog_table"]


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
    url = f"{BOOK_DATA_URL}{name}.csv"
    with urllib.request.urlopen(url) as response:
        body = response.read().decode("utf-8")
    return _columns(list(csv.DictReader(body.splitlines())), text)
