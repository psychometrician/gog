"""Run the manual's sentences through the Python binding and compare with R.

The book is 47 chapters of R that the engine draws live, so it is the best
corpus of real gog sentences in existence. `extract.R` records each one with
the SVG R got for it; this runs the same sentence in Python and asks whether
the two bindings said the same thing.

Three outcomes count as agreement, and they are reported apart because they
mean different things:

  * the same plot        byte-identical SVG — the engine saw the same spec
  * the same refusal     word-identical diagnostic — an engine refusal, which
                         must not depend on who asked
  * the same refusal,
    said in Python       a *binding* refusal, whose message teaches the
                         caller's own syntax; the two texts differ on purpose

Run from the project root, after `extract.R`:

    python3 py-pkg/gog/tests/book_parity/run.py
"""

import hashlib
import json
import math
import os
import sys
import warnings
from collections import Counter
from datetime import date, datetime, timedelta

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.abspath(os.path.join(HERE, "..", "..")))  # the package
sys.path.insert(0, HERE)  # translate.py, beside this file

from gog import *  # noqa: E402
from gog import GogError  # noqa: E402
from gog.render import find_gog_cli  # noqa: E402
from translate import translate  # noqa: E402

import corpus_stamp  # noqa: E402

CORPUS = os.path.join(HERE, "corpus")


def cli_path():
    """The engine this run would compare against — asked of the binding itself,
    so the manifest is checked against the binary that will actually draw. A
    missing one is not this check's business to report; `corpus_stamp.check`
    skips the engine comparison and the first render says so properly."""
    try:
        return find_gog_cli()
    except Exception:
        return ""
EPOCH = date(1970, 1, 1)


class Ordered(list):
    """A column that remembers its category order — Python's answer to a factor.

    The R corpus carries `levels` for a factor column, and dropping them would
    make every ordered-category plot differ for a reason that has nothing to do
    with the binding. A pandas `Categorical` presents the same `.cat.categories`
    shape, which is what `render.py` reads.
    """

    class _Cat:
        def __init__(self, categories):
            self.categories = categories

    def __init__(self, values, categories):
        super().__init__(values)
        self.cat = Ordered._Cat(categories)


def rebuild(wire):
    """A table in Python, from the wire form R sent for it."""
    # jsonlite writes every value as an array unless told to unbox, so the
    # `dates` map arrives as {"day": ["day"]}. Reading it as a scalar silently
    # dropped the temporal marker and turned eight calendar axes into plain
    # numbers — the harness lying about the binding, which is worse than the
    # binding being wrong.
    def unit_of(name):
        unit = wire.get("dates", {}).get(name)
        return unit[0] if isinstance(unit, list) else unit

    table = {}
    for name, values in wire.get("floats", {}).items():
        unit = unit_of(name)
        if unit == "day":
            table[name] = [None if v is None else EPOCH + timedelta(seconds=v) for v in values]
        elif unit == "second":
            table[name] = [
                None if v is None else datetime(1970, 1, 1) + timedelta(seconds=v)
                for v in values
            ]
        else:
            table[name] = list(values)
    for name, values in wire.get("strings", {}).items():
        levels = wire.get("levels", {}).get(name)
        table[name] = Ordered(values, levels) if levels else list(values)
    return table


def _index(value):
    """jsonlite writes a scalar as a one-element array unless told otherwise."""
    return value[0] if isinstance(value, list) else value


def main():
    sentences = json.load(open(os.path.join(CORPUS, "sentences.json")))
    wire_tables = json.load(open(os.path.join(CORPUS, "tables.json")))
    pool = [rebuild(wire) for wire in json.load(open(os.path.join(CORPUS, "pool.json")))]

    # **Is the corpus still about this book and this engine?** Asked before any
    # comparison, because a stale corpus does not fail here — it *narrows*. This
    # loop iterates the corpus, so a sentence the manual gained since the last
    # recording is not a disagreement, it is absent, and the run reports a clean
    # pass over a book that no longer exists. That is not hypothetical: the
    # recorder broke, six sentences from the 3-D histogram chapter never entered
    # the corpus, and four commits shipped while this printed a tidy zero.
    root = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
    stale = corpus_stamp.check(root, cli_path(), CORPUS, len(sentences))
    if stale:
        print("The corpus is not current, so this run would not mean what it says:\n")
        for complaint in stale:
            print("  * " + complaint + "\n")
        return 1

    # `chapter/name` first, then the shared `/name` — the same nearest-wins
    # order the chapters themselves resolve in.
    tables = {}
    for key, index in wire_tables.items():
        chapter, _, name = key.rpartition("/")
        tables.setdefault(chapter, {})[name] = pool[_index(index) - 1]

    tally = Counter()
    rules = Counter()
    failures = []
    binding_refusals = []
    language_specific = []

    for sentence in sentences:
        python_source, fired, blocked = translate(sentence["source"])
        rules.update(set(fired))

        if blocked:
            tally["language-specific (not translated)"] += 1
            language_specific.append((sentence["id"], blocked,
                                      sentence["source"].splitlines()[0]))
            continue

        scope = dict(globals())
        # The chapter's frames are the fallback; the sentence's own — recorded
        # from the spec R built — win, because a chapter may redefine a name
        # between two sentences and both readings are documented.
        scope.update(tables.get("", {}))
        scope.update(tables.get(sentence["chapter"], {}))
        # An empty R list is `[]` in JSON, not `{}` — the sentence refused
        # before a spec existed, so it has no tables of its own.
        own = sentence.get("tables") or {}
        for name, index in (own.items() if isinstance(own, dict) else ()):
            scope[name] = pool[_index(index) - 1]

        try:
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                plot = eval(python_source, scope)
                # Hashed exactly as rendered — see `extract.R`, which records
                # R's side the same way. The `.rstrip()` that used to be here
                # existed only because R returned one byte less than this does.
                outcome = "SVG " + hashlib.sha256(
                    render_svg(plot).encode("utf-8")).hexdigest()
        except GogError as error:
            outcome = "REFUSED\n" + str(error)
        except NameError as error:
            tally["table or name missing from the corpus"] += 1
            failures.append((sentence["id"], "NameError", str(error), python_source))
            continue
        except Exception as error:  # a binding bug, not a refusal
            tally["CRASHED"] += 1
            failures.append((sentence["id"], type(error).__name__, str(error), python_source))
            continue

        expected = sentence["outcome"]
        if outcome == expected:
            tally["identical plot" if not outcome.startswith("REFUSED") else "identical refusal"] += 1
        elif outcome.startswith("REFUSED") and expected.startswith("REFUSED"):
            tally["refused in both, worded per binding"] += 1
            binding_refusals.append((sentence["id"], expected.split("\n", 1)[1],
                                     outcome.split("\n", 1)[1]))
        else:
            tally["DISAGREED"] += 1
            failures.append((
                sentence["id"],
                "R drew, Python refused" if outcome.startswith("REFUSED") else
                "R refused, Python drew" if expected.startswith("REFUSED") else
                "both drew, different SVG",
                (outcome if outcome.startswith("REFUSED") else expected).split("\n", 1)[-1][:200]
                if outcome.startswith("REFUSED") or expected.startswith("REFUSED")
                else f"R {expected} vs Python {outcome}",
                python_source,
            ))

    print(f"{len(sentences)} sentences from the manual\n")
    for name, count in tally.most_common():
        print(f"  {count:4d}  {name}")

    print("\ntranslation rules that fired:")
    for name, count in rules.most_common():
        print(f"  {count:4d}  {name}")

    if binding_refusals:
        print(f"\n{len(binding_refusals)} refusals worded per binding (expected — a message "
              f"teaches the caller's own syntax):")
        for case_id, r_message, py_message in binding_refusals[:6]:
            print(f"  {case_id}\n      R : {r_message.splitlines()[0][:104]}"
                  f"\n      py: {py_message.splitlines()[0][:104]}")

    if language_specific:
        print(f"\n{len(language_specific)} sentences that do not carry over:")
        for case_id, why, source in language_specific:
            print(f"  {case_id:24s} {source[:64]}\n      {why}")

    if failures:
        print(f"\n{len(failures)} to look at:")
        for case_id, kind, detail, source in failures[:25]:
            print(f"  {case_id:24s} {kind}\n      {detail[:150]}"
                  f"\n      {source.splitlines()[0][:110]}")

    return 1 if tally["DISAGREED"] or tally["CRASHED"] else 0


if __name__ == "__main__":
    sys.exit(main())
