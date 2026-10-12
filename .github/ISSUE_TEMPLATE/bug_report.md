---
name: A plot that is drawn wrong
about: A plot drawn differently from what you expected, or a refusal you think is wrong
labels: bug
---

**Language and version.** Run the line for your language, paste what it prints,
and delete the other three lines:

- R: `packageVersion("gog")`
- Python: `import gog; print(gog.__version__)`
- Julia: `import Pkg; Pkg.status("GrammarOfGraphics")`
- JavaScript: `npm ls grammar-of-graphics`

**The sentence.** Paste the code that builds the plot.

```

```

**The file.** Save your plot with `save_json(p, "plot.json")`, where `p` is the
plot, and attach `plot.json` here. The engine draws your plot again from this
file alone, exactly as you saw it. The file holds every value of each column
your sentence names. Read it before you attach it. If a column holds private
values, replace them in your table with invented ones, check that the plot still
shows the problem, and save the file again.

**What you expected.**

**What was drawn.** Describe it, or paste the refusal's whole message.
