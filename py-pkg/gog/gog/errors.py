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
    """A plot the grammar refuses — from the binding or from the engine."""

    def __str__(self) -> str:  # keep the engine's wording verbatim
        return "\n".join(str(a) for a in self.args)
