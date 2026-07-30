# Book parity — does the manual's R say the same thing in Python?

The book is 47 chapters of live R: every plot in it comes from the compiled
engine as the page builds, which makes it the R binding's integration test and
the largest body of real gog sentences that exists. This harness points that
corpus at the second binding.

```bash
Rscript py-pkg/gog/tests/book_parity/extract.R   # re-record the corpus (needs R + the book)
python3 py-pkg/gog/tests/book_parity/run.py      # compare (needs neither)
```

`extract.R` re-runs the chapters, and records a sentence whenever an
expression's *value* is a `gog_spec` — not when its text looks like one, which
is what lets it pick up `p <- data(df) + …` and skip `knitr::kable(…)` with no
list of exceptions. Each sentence is stored with the SHA-256 of the SVG R got
for it (or the refusal, verbatim) and the tables it resolved against, taken from
the spec object so that a chapter redefining a name between two sentences gives
each the table it actually used. `run.py` translates each sentence with
`translate.py` and asks whether Python said the same thing.

The corpus is committed (about 900 KB, plots as hashes) so the comparison runs
in an environment with no R in it. Re-record it after any engine change, since
every hash is an SVG the engine produced.

## What the outcomes mean

| Outcome | Reading |
|---|---|
| **identical plot** | byte-identical SVG: the engine received the same specification |
| **identical refusal** | word-identical diagnostic: an *engine* refusal, which must not depend on who asked |
| **refused in both, worded per binding** | a *binding* refusal. The texts differ on purpose — an R message teaches `x(gdp)` and a Python one teaches `x(col.gdp)`, and a message that taught the other language's syntax would be the bug |
| **language-specific** | the sentence is about the host language, not the grammar |
| **DISAGREED / CRASHED** | a real defect. `run.py` exits non-zero |

## Where it stands

493 sentences, as of 2026-07-25:

```
 386  identical plot
  93  identical refusal
   7  refused in both, worded per binding
   7  language-specific (not translated)
   0  disagreements
```

The jump from 481 is thirteen added and one replaced, from two sessions, and the
second half is the part worth knowing about. Seven are new manual: the
two-dimensional group-by's plots and its refusals. **The other six had been
missing** — they are the 3-D histogram chapter's, written and merged a session
earlier, which the corpus never saw because `extract.R` had already stopped
working. It broke the moment `setup.R` began sourcing `python.R` and `tabs.R`:
those run in the global environment, while `extract.R` deliberately loads
`setup.R` into one of its own, so `proj_root` was set in one place and looked for
in another. So the corpus froze holding a whole engine feature's worth of
sentences it had never been shown — which is exactly what parity is for, and
exactly what a frozen corpus cannot say. Fixed by making setup.R's nested sources
`local = TRUE`, which is what they always meant; nothing about the book's own
rendering changes.

**`bindings/python.qmd` has no sentences here, and that is correct.** Its chunks
are Python, run through `book/R/python.R`, so no R expression in it ever
evaluates to a `gog_spec` — the recorder's own test for what counts. The chapter
is checked the other way round, by being the thing the translator's output is
compared against.

## What stops it freezing again

The corpus carries a `manifest.json` (`corpus_stamp.py`), and `run.py` refuses to
report at all when it is out of date. Two things are recorded, because there are
two ways to drift and only one of them is loud on its own:

- **the engine** — the SHA-256 of the `gog-cli` that drew every hash in the
  corpus. A rebuilt engine that draws differently already produces hundreds of
  disagreements, so this does not *detect* anything new; it stops those
  disagreements reading as *the bindings disagree* when the truth is *the corpus
  is old*, which sends the reader into the wrong file. Measured rather than
  assumed: a release rebuild from unchanged source reproduces the same bytes.
- **the manual's live chunks**, hashed per chapter. This is the one that catches
  the silent case. `run.py` iterates the corpus, so a sentence a chapter gained
  since the last recording is not a failure — it is **absent**, and the run
  reports a clean pass over a book that no longer exists. Only `{r}` chunk bodies
  are hashed, never prose: a harness that cried stale on every paragraph edit
  would be re-recorded reflexively or ignored, and both defeat it.

One implementation of those hashes, called from both sides — `extract.R` shells
out to `corpus_stamp.py write` rather than spelling them again in R, since two
spellings of one hash is the drift the manifest is supposed to catch.

All seven language-specific sentences are in `bindings/r.qmd`, the chapter whose
subject *is* R's host-language questions — five pipes and two `%>%` cases. The
grammar chapters carry over without exception.

## The translation rules, and why they are the point

An R user moving to Python is applying a handful of mechanical rewrites, not
learning a second library. `translate.py` writes them out, and the run reports
how often each fired across the corpus:

| Rule | Fired | Example |
|---|---|---|
| a column takes the accessor | 483 | `x(gdp)` → `x(col.gdp)` |
| a multi-line sentence takes parentheses | 328 | `a +\n b` → `(a +\n b)` |
| R's literals become Python's | 17 | `desc = TRUE` → `desc=True` |
| a vector becomes a list | 6 | `c("a", "b")` → `["a", "b"]` |
| an assignment prefix is dropped | 3 | `p <- …` → `…` |
| `e` comes from the library | 1 | `exp(1)` → `math.e` |
| an inline table becomes a dict | 1 | `data.frame(life = 50)` → `{"life": [50]}` |

That is the whole list, over every sentence the manual teaches. A sixth common
rule appearing here would be evidence that the two bindings had begun to
diverge, which is what this harness exists to notice.
