# errors.py — the one exception type the binding raises
#
# A refusal is a refusal whether the engine made it or the binding did, so both
# arrive as `GogError` and both carry a message that starts `gog: ` and says
# what to write instead (spec §12: errors must give direction). R has no
# equivalent file because `stop()` already is one thing; Python needs a name.
#
# It subclasses `Exception` rather than `TypeError`/`ValueError` on purpose. A
# caller catching it is catching "the grammar refused", which is one category,
# and sorting refusals into Python's exception taxonomy would invite `except
# ValueError` to swallow half of them.


class GogError(Exception):
    """The error gog raises when it refuses a plot.

    Both kinds of refusal raise `GogError`: one this package makes at the line
    that writes the sentence, and one the engine makes when the plot is drawn.
    The message starts with `gog: ` and says what to write instead. `GogError`
    is a direct subclass of `Exception`, so `except GogError` catches every
    refusal gog makes and nothing else.

    Not every message is an error. A plot that draws can still carry a note,
    such as how many rows with a missing value were left out; gog prints
    those on stderr, each starting with `gog: `, and raises nothing.

    Examples
    --------
    A string where a column belongs is refused at once.

    >>> cars = {"cylinders": ["6", "6", "4", "6", "8", "6", "8", "4"],
    ...         "mpg": [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4]}
    >>> try:
    ...     p = data(cars) + bar + x("cylinders") + y(col.mpg)
    ... except GogError as refusal:
    ...     message = str(refusal)

    A sentence the engine cannot draw is refused when it is drawn: a bar
    needs a height, from `y` or from a transform such as `count`.

    >>> try:
    ...     svg = render_svg(data(cars) + bar + x(col.cylinders))
    ... except GogError as refusal:
    ...     message = str(refusal)
    """

    def __str__(self) -> str:  # keep the engine's wording verbatim
        return "\n".join(str(a) for a in self.args)
