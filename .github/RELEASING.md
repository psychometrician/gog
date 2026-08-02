# What a push does, and how a release is cut

**Every version number in this repository is a literal string typed into a file.**
Nothing in CI increments one, no bot invents one, and no registry assigns one. The
only thing automated about the numbers is a check that they *agree* — never that
they *move*.

This document says what each push starts, what only a tag can start, and where the
numbers live.

## A push to `main`

Two workflows here, and one service that is not here at all.

| What runs | When | What it publishes |
|---|---|---|
| `tests.yml` | every push to `main`, and every pull request | nothing |
| `book.yml` | a push to `main` touching `book/`, `gog-core/`, `gog-cli/`, `r-pkg/`, `py-pkg/`, or itself | the manual, to `psychometrician/gog-book` |
| **r-universe** | every commit, by polling — no workflow in this repository | the R package, at the version in `DESCRIPTION` |

`tests.yml` has no path filter on purpose: the engine and the four bindings are one
grammar, so there is no change to any of them that cannot break another. Three jobs
— the engine's own tests, the four binding suites in a matrix, and the parity run
that draws every sentence in the manual from Python, JavaScript and Julia and
compares each against R.

`book.yml` does not list `js-pkg/` or `jl-pkg/`, and that is correct rather than an
oversight. The manual executes **R chunks only**; the JavaScript and Julia chapters
show syntax without running it, so a change confined to those bindings cannot alter
a rendered plot. The chapters themselves live under `book/`, so editing their prose
does trigger a rebuild.

The site is a single orphan commit force-pushed over whatever was there, published
cross-repository with a deploy key. The workflow takes a `concurrency` group so that
two pushes in quick succession do not race to be last.

## A push to `main` cannot publish a package

| Workflow | Trigger |
|---|---|
| `python-release.yml` → PyPI | `push: tags: ["py-v*"]` |
| `js-release.yml` → npm | `push: tags: ["js-v*"]` |
| `python-wheels.yml`, `js-packages.yml` | `workflow_call`, `workflow_dispatch`, and pull requests on their own paths |
| `TagBot.yml` | an `issue_comment` from `JuliaTagBot`, or `workflow_dispatch` |

The two `*-wheels`/`*-packages` files build artifacts and stop. A pull request runs
them so that a broken build is caught where it is cheap; only a tag sends anything
anywhere. They are the same jobs the release workflows call, unchanged, so what a
release uploads is what a pull request already proved buildable.

## The eight declarations

One number, written down eight times, in four spellings:

```
r-pkg/gog/DESCRIPTION                    Version: X.Y.Z
py-pkg/gog/pyproject.toml                version = "X.Y.Z"
py-pkg/gog/gog/__init__.py               __version__ = "X.Y.Z"
js-pkg/gog/package.json                  "version": "X.Y.Z"
jl-pkg/GrammarOfGraphics/Project.toml    version = "X.Y.Z"
gog-core/Cargo.toml                      version = "X.Y.Z"
gog-cli/Cargo.toml                       version = "X.Y.Z"
gog-wasm/Cargo.toml                      version = "X.Y.Z"
```

The R suite compares all eight on every run and fails if they disagree, so a
mismatch is caught by `tests.yml` on the push that introduces it. Both release
workflows repeat the same comparison against the tag before building anything.

**Two of the eight were missed once each, and for opposite reasons.**
`py-pkg/gog/gog/__init__.py` is what `gog.__version__` reports to a user;
`pyproject.toml` is what the wheel's metadata says. Those two disagreed through five
built wheels once, and no manifest check could see it, because every *manifest*
agreed. `gog-wasm/Cargo.toml` is a manifest, but it sits outside the Cargo
workspace, so every list that began by reading `Cargo.toml` walked past it. Nothing
publishes that crate, but `r-pkg/gog/.prepare` copies the whole of `gog-wasm/` into
the R source tarball, so the string travels to a user's disk regardless.

JavaScript checks **thirteen**: the eight above, plus the five platform packages
pinned in `optionalDependencies`. A platform package pinned away from the main
package's version is a dependency npm reports as *nothing* — it silently omits an
optional dependency it cannot resolve — so a user gets an install that succeeds and
a binding that cannot find its engine.

Three more files carry the number and are not checked, because a build rewrites
them: `Cargo.lock`, `gog-wasm/Cargo.lock`, and the parity harness's
`jl-pkg/GrammarOfGraphics/test/book_parity/Manifest.toml`. Nothing here builds with
`--locked`, so a stale entry compiles anyway, but `.prepare` ships two of the three
to R users. Regenerate them in the same commit as the bump; see *Cutting a release*.

Move all eight together or not at all.

## The four registries

### r-universe (R) — the only one a plain push reaches

The registry is a separate repository, `psychometrician/psychometrician.r-universe.dev`,
whose `packages.json` names this repository and the subdirectory to build:

```json
[{ "package": "gog", "url": "https://github.com/psychometrician/gog", "subdir": "r-pkg/gog" }]
```

r-universe polls for new commits and rebuilds. It publishes at **exactly** the
version in `DESCRIPTION`, records the commit it built in `RemoteSha`, and replaces
the previous build at the same number. There is no review, no gate, and no
increment.

**The consequence worth knowing before you rely on it:** because the number does not
move, an R user who has already installed `0.0.1` will not get your new commits from
`update.packages()`. R compares version numbers, and `0.0.1` is not newer than
`0.0.1`. They would need to force a reinstall.

The R convention for this is the fourth component — `0.0.1.9000`, `.9001`, and so on
for development builds, with three-component numbers reserved for real releases.
r-universe is built around that convention. Adopting it here means deciding whether
`DESCRIPTION` may drift from the other six (and relaxing the agreement test), or
whether all eight move together and Cargo and npm manifests carry development
numbers nobody is installing. That is a decision, not a default; nothing in the
current setup depends on it either way.

### PyPI (Python) — tag-gated, and rehearsed first

Tag `py-vX.Y.Z`. Five jobs, each able to fail only forward:

1. **`version`** — the tag matches all eight declarations, and PyPI does not already
   have this number. Checked *before* anything is built, because discovering a
   mismatch at upload time spends the number on a failed release.
2. **`wheels`** — five platforms, each wheel carrying its own engine, each installed
   into a bare virtual environment and asked to draw before it counts as built.
3. **`collect`** — five wheels and no sdist. The sdist cannot install for anyone (the
   crates live above its root), so it must never reach an index.
4. **`testpypi`** — the same artifacts against a real index, catching metadata
   problems while the number is still spendable.
5. **`pypi`** — gated on a GitHub Environment with a required reviewer, because the
   upload is the one step that cannot be undone.

The upload uses **Trusted Publishing**: the job asks GitHub for a short-lived OIDC
token, PyPI verifies it came from this repository running this workflow file, and
mints an upload token good for a few minutes. No credential is stored anywhere. It
also means the publisher registered on PyPI names *this filename* — renaming the
file breaks releases until the publisher is updated to match.

**The publisher has to exist on PyPI's side first**, and that half is not in this
repository: it is registered in the project's web interface, naming the owner, this
repository, `python-release.yml`, and the environment (`pypi`, and `testpypi` on
test.pypi.org). Until it is, the workflow runs green through four jobs and fails at
the upload. See *Before the first automated release* below.

### npm (JavaScript) — tag-gated, six packages at once

Tag `js-vX.Y.Z`. Same shape: `version` → `packages` → `collect` → `publish`, with the
publish gated on an Environment and authenticated by Trusted Publishing rather than
a stored token.

Six packages go out together — `grammar-of-graphics` and its five platform packages:

```
grammar-of-graphics-darwin-arm64   grammar-of-graphics-linux-arm64
grammar-of-graphics-darwin-x64     grammar-of-graphics-linux-x64
                                   grammar-of-graphics-win32-x64
```

The `version` job asks the registry whether any of the six already holds that number,
before a single tarball is built.

npm has no counterpart to PyPI's *pending* publisher: a trusted publisher can only be
attached to a package that already exists, so the first release of each name was
published by hand and the workflow owns every release after that.

**All six need it, separately.** A trusted publisher is per-package, so five
configured and one forgotten means the tag publishes five engines and then fails on
the binding — or worse, publishes the binding while a platform package is missing,
which npm reports to the user as nothing at all. See *Before the first automated
release* below.

### Julia General — comment-gated, and not fully ours

Nothing here fires it. You trigger a registration by commenting on a commit:

```
@JuliaRegistrator register subdir=jl-pkg/GrammarOfGraphics
```

The `subdir` is required and is the whole reason it needs care: `GrammarOfGraphics`
is one of four bindings in a monorepo, so its version lives in
`jl-pkg/GrammarOfGraphics/Project.toml` rather than at the root.

Registrator reads the version out of that file — it does not choose one. AutoMerge
then applies the registry's own rules: a new package must be `0.0.1`, `0.1.0` or
`1.0.0`, later versions must be a reasonable bump from the last registered one, and
a new package waits three days before merging. An *existing* package merges without
the hold.

**Never comment on a registration pull request without the literal `[noblock]`.** A
bare comment is read as an objection: it blocks the automatic merge and sends the
registration to manual review, which turns a three-day wait into an indefinite one.

Registrator registers a **tree hash**, not a tag, so a merged registration leaves no
mark in this repository. `TagBot.yml` is meant to close that: when the registry pull
request merges, it writes the git tag and the GitHub release to match. Because
`subdir` is set, the tag it writes carries the package name —
`GrammarOfGraphics-vX.Y.Z`, not a bare `vX.Y.Z`.

**That tag shape is deliberate.** It matches neither `py-v*` nor `js-v*`, so a Julia
release cannot set off a Python or npm publish. Keep it that way if the tag patterns
are ever revised.

**Expect the tag push to be rejected, and know why before it happens.**
`GITHUB_TOKEN` acts as a GitHub App, and GitHub refuses any ref push from an App that
would create or update a file under `.github/workflows/`. A registration points at
the commit Registrator was run on, so as soon as a later commit edits any workflow
file, pushing the tag for that older commit reads as a workflow change and is
refused. There is no fix inside `permissions:` — `workflows` is a personal-token
scope, not one of the keys a workflow can request. The three ways out are an SSH
deploy key on TagBot's `ssh:` input, which is not an App and is not subject to the
rule; a personal token with `workflow` scope; or a person pushing the tag:

```bash
git tag -a GrammarOfGraphics-vX.Y.Z <commit> -m 'GrammarOfGraphics-vX.Y.Z'
git push origin GrammarOfGraphics-vX.Y.Z
```

Nothing about installing the package depends on the tag. Pkg resolves a version by
tree hash, so `Pkg.add` works whether or not the tag exists; the tag is provenance
and a release page. The way to avoid the rejection entirely is to register when the
tree is final, with no workflow edit still to come.

## Before the first automated release

Three GitHub Environments already exist here, and their rules are the gate on the
irreversible steps: `pypi` and `npm` require a reviewer, `testpypi` is open.

The other half of the handshake lives on the indexes and has to be set up through
their websites, once per package. It is invisible from this repository, and its
absence looks like a green build that fails on its last step.

| Where | What to register |
|---|---|
| pypi.org → `gog` → Publishing | owner `psychometrician`, repo `gog`, workflow `python-release.yml`, environment `pypi` |
| test.pypi.org → pending publisher | the same, environment `testpypi` |
| npmjs.com → each of the six → Settings → Trusted Publisher | owner `psychometrician`, repo `gog`, workflow `js-release.yml`, environment `npm` |

**Do not rename either release workflow file.** A trusted publisher binds to the
filename, so a rename silently invalidates every publisher registered against it.

## Cutting a release

`.github/release` does every mechanical step below and refuses to do the rest:

```bash
.github/release --check      # verify the tree, change nothing
.github/release <version>       # steps 2-6: bump, regenerate, test, dispatch
.github/release --tag py     # → PyPI, then approve the `pypi` environment
.github/release --tag js     # → npm, then approve the `npm` environment
.github/release --julia      # → General, which auto-merges for an existing package
```

It stops before the tags on purpose, because the packaging runs it dispatches are
worth reading before a number is spent, and it asks you to type the tag back
before pushing one. It cannot register the trusted publishers; nothing can, from
a terminal. The steps below are what it does, and what to do if you would rather
do them by hand.

1. Decide the number. It is never chosen for you.
2. Move all eight declarations to it — and the five npm pins, if JavaScript is going
   out. Then regenerate the three files that carry the number without being checked,
   in the *same* commit, because the push is what r-universe builds from and there is
   no later chance to correct the tarball it compiles:

   ```bash
   cargo build --release
   cargo build --release --target wasm32-unknown-unknown --manifest-path gog-wasm/Cargo.toml
   julia --project=jl-pkg/GrammarOfGraphics/test/book_parity -e 'using Pkg; Pkg.resolve()'
   ```

3. Run the full local suite (the pull-request checklist in `CONTRIBUTING.md`), which
   includes the agreement check.
4. **Install the four packages somewhere real and look at a plot.** Build the
   artifacts, install each into a clean environment, restart the session, and draw one
   flat plot and one interactive one. This is the only step that sees a defect which
   bypasses `gog-cli`, and such defects exist: two bindings once could not draw an
   interactive plot at all while every check in this repository was green, because the
   static picture is the engine's and was perfect the whole time.
5. **Dispatch the two packaging workflows before tagging anything.** Neither
   publishes; both are otherwise reached only by `workflow_call` from a release tag,
   so a break in them would first show itself on a tag, which is the worst place to
   meet one.

   ```bash
   gh workflow run python-wheels.yml --ref main
   gh workflow run js-packages.yml   --ref main
   ```

6. Commit and push. `tests.yml` and `book.yml` run; nothing is published to an index.
7. Tag what you want to release, and only that:
   - `git tag py-v0.0.2 && git push origin py-v0.0.2` → PyPI. TestPyPI runs first and
     is not gated; then approve the `pypi` environment.
   - `git tag js-v0.0.2 && git push origin js-v0.0.2` → npm. Approve the `npm`
     environment. It publishes the five engines before the binding, because npm omits
     an optional dependency it cannot resolve and reports the gap as nothing.
   - comment `@JuliaRegistrator register subdir=jl-pkg/GrammarOfGraphics` on the
     release commit → Julia.
   - R needs nothing: r-universe rebuilds from `DESCRIPTION` on its own.
8. **Verify each one by installing it.** A green workflow proves an upload happened,
   not that the result works. The bar is the same one each binding was held to at
   `0.0.1`: install from the registry into a clean environment and draw from a
   directory with no `target/` above it and `GOG_CLI_PATH` unset.

   The bar has a second half, and it fails silently. A package carries two engines:
   the one that draws, and the WebAssembly build that lets a 3-D plot turn on a web
   page. Only the first is required, so a package missing the second installs
   cleanly, draws correctly, and its 3-D plots simply do not turn — nothing about
   that looks like a failure. Assert it by hand as well as in CI: the rendered block
   must contain `<script type="module"`. This applies to R, Python and JavaScript.
   Julia ships neither engine, and its documentation says so.

The four routes are independent — tagging Python does not release JavaScript, and
none of them releases R — but the *numbers* are not. All eight declarations move
together, so between bumping them and tagging the last binding, the manifests
describe a release some indexes have not received yet. That window is expected;
what must not happen is the numbers diverging to close it.

## What cannot be taken back

**A published version number is permanently spent** on PyPI, npm and Julia General.
Deleting a release does not free the number, and the next attempt must use a new one.
This is why both release workflows query the index in their *first* job, before a
single artifact is built — a failure there costs a re-run, while a failure at upload
costs a version.

npm has one narrow exception, worth knowing but not worth planning around.
`npm unpublish <package>@<version>` is permitted within **72 hours**, provided no
other package depends on that version — which is exactly the situation after a
partial publish, since the binding is what did not go out. A version cannot be
republished for 24 hours afterward, so it trades three spent numbers for a day.
Treat every number as spent; keep this as the door you did not know was there.

r-universe is the exception in both directions: nothing is spent, and nothing is
final. It rebuilds whatever `main` currently says.
