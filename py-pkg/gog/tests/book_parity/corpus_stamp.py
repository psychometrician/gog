"""What the corpus was recorded *against*, so a stale one says so.

`run.py` compares Python's output with hashes R recorded earlier. That is only
a comparison between two bindings while both halves describe the same world —
and nothing in the three JSON files said which world that was. Two ways it can
drift apart, and this file exists because the second one happened:

  * **the engine changed** — every recorded hash is an SVG a particular
    `gog-cli` drew, so a rebuilt engine that draws differently invalidates all
    of them at once. Loud on its own (hundreds of disagreements), but it reads
    as *the bindings disagree* when the truth is *the corpus is old*, which
    sends the reader hunting in the wrong file.

  * **the book changed** — and this one is silent. `run.py` iterates the
    corpus, so a sentence the manual gained after the last recording is not a
    failure, it is **invisible**. Coverage shrinks against the book and the
    report keeps saying what it said last time. That is exactly what happened:
    `extract.R` broke, six sentences from the 3-D histogram chapter never
    entered the corpus, and the harness went on reporting a clean run over the
    481 it still knew about.

So the corpus carries a manifest of what it was recorded against, and `run.py`
refuses to report a pass when the world has moved. **One implementation, two
callers** — `extract.R` shells out to `write` here rather than re-deriving the
same hashes in R, because two spellings of one hash is the drift this is
supposed to catch.

    python3 corpus_stamp.py write <project-root> <gog-cli>   # extract.R calls this
"""

import glob
import hashlib
import json
import os
import re
import sys

MANIFEST = "manifest.json"


def engine_hash(cli_path):
    """The identity of the binary that drew the corpus.

    The file's own SHA-256, because `gog-cli` has no `--version` to ask (it
    reads a spec on stdin and nothing else) and adding one to identify a build
    would be a CLI surface invented for a test. Measured rather than assumed:
    a release rebuild from unchanged source reproduces the same bytes, so this
    does not cry stale every time someone runs `cargo build`.
    """
    with open(cli_path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


# A knitr chunk that *runs*. `{r}` executes and ```` ```r ```` does not, which is
# the book's own live-chunk rule, and the same distinction that
# decides whether a chunk can hold a sentence at all.
CHUNK = re.compile(r"^```\{r[^}]*\}\s*$(.*?)^```\s*$", re.M | re.S)


def chapter_hashes(book_dir):
    """Each chapter keyed to the content of its live chunks, and nothing else.

    Hashing the whole `.qmd` would be simpler and would cry stale on every
    prose edit, which in this book is most edits — a harness that is usually
    wrong about being stale gets re-recorded reflexively or ignored, and both
    defeat it. Only the code decides what sentences exist, so only the code is
    hashed. A chapter whose prose changed and whose chunks did not is not
    stale, because the corpus would come out identical.
    """
    out = {}
    for path in sorted(glob.glob(os.path.join(book_dir, "**", "*.qmd"), recursive=True)):
        name = os.path.splitext(os.path.relpath(path, book_dir))[0]
        with open(path, encoding="utf-8") as f:
            code = "\n".join(m.group(1) for m in CHUNK.finditer(f.read()))
        out[name.replace(os.sep, "/")] = hashlib.sha256(code.encode("utf-8")).hexdigest()
    return out


def write(root, cli_path, corpus_dir, sentences):
    manifest = {
        "engine": engine_hash(cli_path),
        "chapters": chapter_hashes(os.path.join(root, "book")),
        "sentences": sentences,
    }
    with open(os.path.join(corpus_dir, MANIFEST), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=1, sort_keys=True)
        f.write("\n")
    return manifest


def check(root, cli_path, corpus_dir, sentences):
    """What is out of date, as a list of complaints. Empty means current.

    Each complaint names the one command that fixes it, per §12's rule for
    every other diagnostic in this project: say what to do, not only what went
    wrong.
    """
    path = os.path.join(corpus_dir, MANIFEST)
    if not os.path.exists(path):
        return ["the corpus carries no manifest, so what it was recorded "
                "against is unknown — re-record it: "
                "`Rscript py-pkg/gog/tests/book_parity/extract.R`"]
    with open(path, encoding="utf-8") as f:
        manifest = json.load(f)

    out = []
    if os.path.exists(cli_path) and manifest.get("engine") != engine_hash(cli_path):
        out.append(
            "the corpus was recorded by a different `gog-cli` than the one "
            "this run would compare against, so every hash in it describes "
            "another engine — the disagreements below would be its age, not "
            "the bindings. Re-record: "
            "`Rscript py-pkg/gog/tests/book_parity/extract.R`")

    was = manifest.get("chapters", {})
    now = chapter_hashes(os.path.join(root, "book"))
    changed = sorted(k for k in now if k in was and now[k] != was[k])
    added = sorted(k for k in now if k not in was)
    gone = sorted(k for k in was if k not in now)
    if changed or added or gone:
        parts = []
        if changed:
            parts.append("changed: " + ", ".join(changed))
        if added:
            parts.append("new: " + ", ".join(added))
        if gone:
            parts.append("removed: " + ", ".join(gone))
        out.append(
            "the manual's live chunks have moved since the corpus was "
            "recorded (" + "; ".join(parts) + "). Sentences a chapter gained "
            "are not failures here, they are **absent** — this run would "
            "check the book as it was. Re-record: "
            "`Rscript py-pkg/gog/tests/book_parity/extract.R`")

    if manifest.get("sentences") != sentences:
        out.append(
            f"the manifest counts {manifest.get('sentences')} sentences and "
            f"the corpus holds {sentences} — the two were written by "
            "different runs. Re-record: "
            "`Rscript py-pkg/gog/tests/book_parity/extract.R`")
    return out


if __name__ == "__main__":
    # Three callers now, and still one implementation: `extract.R` writes the
    # stamp, `run.py` imports `check`, and `run.mjs` — which has no way to import
    # Python — runs `check` here. Re-deriving these hashes in a third language
    # would be two spellings of one hash, which is the drift this file exists to
    # catch. A complaint per line on stdout, exit 1 when the corpus is stale.
    if len(sys.argv) != 4 or sys.argv[1] not in ("write", "check"):
        sys.exit("usage: corpus_stamp.py write|check <project-root> <gog-cli>")
    mode, root, cli = sys.argv[1], sys.argv[2], sys.argv[3]
    corpus = os.path.join(root, "py-pkg", "gog", "tests", "book_parity", "corpus")
    with open(os.path.join(corpus, "sentences.json"), encoding="utf-8") as f:
        n = len(json.load(f))

    if mode == "check":
        complaints = check(root, cli, corpus, n)
        for complaint in complaints:
            print(complaint)
        sys.exit(1 if complaints else 0)

    m = write(root, cli, corpus, n)
    print(f"stamped: engine {m['engine'][:12]} | "
          f"{len(m['chapters'])} chapters | {m['sentences']} sentences")
