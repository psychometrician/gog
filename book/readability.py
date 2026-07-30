#!/usr/bin/env python3
"""Report how hard the book's prose is to read. Book law 8, spec §20.

This is a **report, not a gate**. It is deliberately not wired into
`test_basic.R` beside `check_vocabulary.R` and `check_refusals.R`, because
those two check facts that are either true or false — a name is in the kernel
block or it isn't, a refusal refuses or it doesn't — while a sentence length is
a judgment. A threshold here would be enforced by splitting sentences in half
rather than by rewriting them, which is the metric improving while the prose
gets worse.

Use it the way a scale is used during a diet: to catch the impression that a
pass "reads more simply now" when the numbers say it does not.

    python3 book/readability.py            # every chapter, worst first
    python3 book/readability.py transforms # one chapter, with its long sentences

What it measures, and what it cannot. Words per sentence and the share of long
sentences are the two things that correlate with difficulty for a reader
working in a second language, and both are countable. The things that matter
just as much — idiom, metaphor, a term used before it is defined, the point
buried after a dash — are not countable, and a good score does not mean the
prose is plain. Read the long sentences the report prints; that list is the
useful half of the output.

Prose only: code chunks, tables, headings, list bullets and YAML are stripped,
because a bullet fragment is not a sentence and a table cell is not prose. The
baseline on 2026-07-28, before any editing pass, was 20.4 words per sentence
across 3,991 sentences, with 122 sentences of 45 words or more.
"""

import os
import re
import statistics
import sys
from glob import glob

BOOK = os.path.dirname(os.path.abspath(__file__))
TARGET = 18.0  # spec §20 book law 8: 15-18 words, one idea per sentence
LONG = 30      # a sentence a reader has to hold open while parsing it
VERY_LONG = 45 # a sentence to rewrite rather than trim


def prose(path):
    """Strip everything that is not a sentence a reader reads."""
    t = open(path, encoding="utf-8").read()
    t = re.sub(r"\A---\n.*?\n---\n", "", t, flags=re.S)        # YAML front matter
    t = re.sub(r"```.*?```", "\n\nBREAK.\n\n", t, flags=re.S)  # chunks end a sentence
    t = re.sub(r"^\s*\|.*$", "", t, flags=re.M)                # table rows
    t = re.sub(r"^\s*:::.*$", "\nBREAK.\n", t, flags=re.M)     # fenced divs
    t = re.sub(r"^\s*#+ .*$", "\nBREAK.\n", t, flags=re.M)     # headings
    t = re.sub(r"^\s*[-*+]\s|^\s*\d+\.\s", "\nBREAK. ", t, flags=re.M)  # list items
    t = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", t)             # links keep their text
    t = re.sub(r"\[@[^\]]*\]", "", t)                          # citations
    return t


def sentences(text):
    """Split into sentences. A colon before a code chunk ends one, which is
    the book's most common shape and the one a naive splitter runs through."""
    out = []
    for para in re.split(r"\n\s*\n", text):
        p = re.sub(r"\s+", " ", para).strip()
        for s in re.split(r'(?<=[.!?:])\s+(?=[A-Z"“(*\'])|(?<=[.!?])\s+', p):
            s = s.strip()
            if s and not s.startswith("BREAK") and len(s.split()) >= 4:
                out.append(s)
    return out


def chapters():
    for path in sorted(glob(os.path.join(BOOK, "**", "*.qmd"), recursive=True)):
        if os.sep + "_" in path:
            continue
        yield path


def measure(path):
    ss = sentences(prose(path))
    if len(ss) < 5:
        return None
    lens = [len(s.split()) for s in ss]
    return {
        "file": os.path.relpath(path, BOOK),
        "sentences": ss,
        "n": len(ss),
        "words": sum(lens),
        "mean": statistics.mean(lens),
        "long": 100.0 * sum(1 for x in lens if x > LONG) / len(lens),
        "very": [s for s in ss if len(s.split()) >= VERY_LONG],
    }


def main():
    rows = [r for r in (measure(p) for p in chapters()) if r]
    if len(sys.argv) > 1:
        want = sys.argv[1].lower()
        rows = [r for r in rows if want in r["file"].lower()]
        if not rows:
            sys.exit(f"no chapter matching {want!r}")

    rows.sort(key=lambda r: -r["mean"])
    print(f"{'chapter':32}{'sent':>6}{'words':>7}{'avg':>7}{'>%dw' % LONG:>7}")
    print("-" * 59)
    for r in rows:
        flag = "  <-- over target" if r["mean"] > TARGET else ""
        print(f"{r['file']:32}{r['n']:6d}{r['words']:7d}"
              f"{r['mean']:7.1f}{r['long']:6.0f}%{flag}")

    n = sum(r["n"] for r in rows)
    w = sum(r["words"] for r in rows)
    very = sum(len(r["very"]) for r in rows)
    print("-" * 59)
    print(f"{'TOTAL':32}{n:6d}{w:7d}{w / n:7.1f}")
    print(f"\nTarget is {TARGET:.0f} words per sentence (spec §20, book law 8). "
          f"{very} sentences of {VERY_LONG}+ words.")

    if len(sys.argv) > 1:
        for r in rows:
            for s in sorted(r["very"], key=lambda s: -len(s.split())):
                print(f"\n[{len(s.split())}w] {r['file']}\n  {s}")


if __name__ == "__main__":
    main()
