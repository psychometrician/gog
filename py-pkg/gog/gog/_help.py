# _help.py — how an exported object, rather than a function, carries its own help
#
# `help(bin)` reads `bin.__doc__`, and most of gog's words are not functions:
# `point` and `count` are `Atom` instances, `bin` and `space` are instances of
# small callable classes, and `col` is the one `_ColumnAccessor`. An instance
# whose class declares `__slots__` cannot be given a `__doc__` of its own, so
# without this every mark answered `help()` with the class's text, which says
# what an atom is and nothing about the mark.
#
# The answer is a descriptor installed as the class's `__doc__`. Read from the
# class, it gives the class's own text. Read from an instance, it gives the
# instance's `_help` when one was attached (`with_help`), and the class's text
# otherwise, so a configured copy such as `bin(20)` still answers with
# something. `pydoc` and `inspect.getdoc` both read `__doc__` through it, and
# `pydoc` prints an instance whose text differs from its class's as that
# instance: its repr, then its own help.
#
# Both `spec.py` (for `Atom`) and `columns.py` (for `col`) need it, and
# `spec.py` imports `columns.py`, so it lives here, below both.

import inspect
from typing import Any, Optional


class Help:
    """The `__doc__` of a class whose exported instances each carry their own help."""

    __slots__ = ("text", "instance_text")

    def __init__(self, text: Optional[str], instance_text: Optional[str] = None) -> None:
        self.text = text
        self.instance_text = instance_text

    def __get__(self, obj: Any, cls: Any = None) -> Optional[str]:
        if obj is None:
            return self.text
        # Read past any `__getattr__`: `col._help` would otherwise be a column.
        try:
            own = object.__getattribute__(obj, "_help")
        except AttributeError:
            own = None
        return own or self.instance_text or self.text


def with_help(obj: Any, text: str) -> Any:
    """Attach help to one exported instance, and return the instance."""
    obj._help = inspect.cleandoc(text)
    return obj
