#!/usr/bin/env bash
#
# Publish the rendered book to https://psychometrician.github.io/gog-book/
#
# The site repository holds *only* the rendered HTML. The book's sources live in
# the main repository beside the engine, and stay there: every plot is drawn by
# `gog-cli` at render time, so one engine change invalidates every page at once.
# Splitting them would turn that into a version-matching problem.
#
# Two things this script exists to get right, both of which have bitten before:
#
#   * `cargo test` passing does not mean `target/release/gog-cli` was rebuilt.
#     Every plot in the book is drawn by that binary, so the build is explicit.
#   * `quarto render` exits 0 with broken links, and warns with `WARN:` rather
#     than `warning:`. The exit code proves nothing, so the log is graded.
#
# Usage:  book/publish.sh                 render, verify, publish
#         book/publish.sh --dry           render and verify, but do not push
#         book/publish.sh --publish-only  verify an existing render and publish it
#
# `--publish-only` skips the build and the render. Every check below still runs,
# so it cannot publish an unverified tree; it exists for the case where a render
# has just been graded by hand and repeating it would cost twenty minutes.

set -euo pipefail

# rustup and quarto only edit an interactive shell's profile.
export PATH="$HOME/.cargo/bin:$PATH"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOOK="$REPO_ROOT/book"
STAGE="$(dirname "$REPO_ROOT")/gog-book-site"
REMOTE="https://github.com/psychometrician/gog-book.git"
LOG="$BOOK/.publish-render.log"

DRY_RUN=0
SKIP_RENDER=0
case "${1:-}" in
  --dry)          DRY_RUN=1 ;;
  --publish-only) SKIP_RENDER=1 ;;
  "")             ;;
  *)              echo "publish: unknown option $1" >&2; exit 1 ;;
esac

say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
die() { printf '\n\033[31mpublish: %s\033[0m\n' "$1" >&2; exit 1; }

# --------------------------------------------------------------------------
# A running `quarto preview` renders the book too, on every save, and writes the
# same files this render does. The two race: preview moves `book/index.html` into
# `_book/` first, and the render then dies with `NotFound … rename` after it has
# already done twenty minutes of work. That happened on the first run of this
# script, so the condition is checked before the work rather than discovered
# after it.
if pgrep -f "quarto.*preview" >/dev/null 2>&1; then
  die "a \`quarto preview\` is running — stop it first, or it will race this render
        find it with:  pgrep -fl 'quarto.*preview'"
fi

if [[ $SKIP_RENDER -eq 1 ]]; then
  say "Skipping the build and render (--publish-only)"
else

# --------------------------------------------------------------------------
say "Building the engine"
# The book shells out to this binary once per plot, roughly 700 times.
cargo build --release -p gog-cli --manifest-path "$REPO_ROOT/Cargo.toml"
[[ -x "$REPO_ROOT/target/release/gog-cli" ]] || die "gog-cli did not build"

# --------------------------------------------------------------------------
say "Rendering the whole book to HTML"
# The *whole* book, never a subset: one engine change invalidates every plot at
# once, but Quarto tracks .qmd dependencies rather than the binary.
#
# `--to html` is also what keeps the 816-page PDF off the public site. Quarto
# links a download only for a format it finds rendered, and the free edition is
# HTML while print is the publisher's.
cd "$BOOK"
set +e
quarto render --to html 2>&1 | tee "$LOG"
RENDER_STATUS=${PIPESTATUS[0]}
set -e
[[ $RENDER_STATUS -eq 0 ]] || die "quarto render exited $RENDER_STATUS"

# --------------------------------------------------------------------------
say "Grading the render log"
# The usual advice for reading a render by eye is the broad grep
# `-inE "warn|error|unable|cannot|fail|not found"`. That is right for a human and
# wrong for an automatic gate *on this book*, which is largely about refusals: it
# has 138 `error: true` chunks, so the words appear in ordinary healthy output.
#
# Quarto's own markers are what a gate can trust. It writes `ERROR:` and `WARN:`
# at the start of a line — note `WARN:`, not `warning:` — and a stack trace when
# it dies. That is what caught the `rename` race on the first run of this script.
if grep -nE "(^|\[[0-9;]*m)(ERROR|WARN):|^Stack trace:" "$LOG"; then
  die "the render log has complaints (above) — read them before publishing"
fi
echo "log is clean"

fi  # end of the build-and-render block

# --------------------------------------------------------------------------
# From here down, every mode runs the same checks. `--publish-only` skips the
# work above, never this.
say "Checking the rendered output"
[[ -f "$BOOK/_book/index.html" ]] || die "_book/index.html is missing"

# The placeholder repo-url shipped "Edit this page" buttons pointing at a repo
# that never existed. Never again, and this is the check that says so.
if grep -rq "your-org" "$BOOK/_book/"; then
  die "the placeholder repo-url 'your-org' is still in the rendered output"
fi

# A PDF in _book/ means Quarto will offer the whole book as a download.
if find "$BOOK/_book" -name '*.pdf' -print -quit | grep -q .; then
  die "a PDF is in _book/ — it would appear as a download link on the free site"
fi

grep -q "site_libs" "$BOOK/_book/index.html" \
  || die "index.html does not reference site_libs — the render looks wrong"

echo "output looks right ($(du -sh "$BOOK/_book" | cut -f1))"

if [[ $DRY_RUN -eq 1 ]]; then
  say "Dry run — stopping before publish"
  exit 0
fi

# --------------------------------------------------------------------------
say "Staging the site"
# The staging tree is rebuilt from nothing each time and pushed as **one commit
# with no parent**, replacing whatever was there. The public repository is a
# freshly generated site and nothing else: no development history, no record of
# what changed between publishes, one commit on one branch forever.
#
# Three reasons, and any one of them would be enough. The author asked for it.
# History of generated output answers no question a reader has. And a fresh
# history is what pins the repository at ~25 MB — every engine change rewrites
# every inlined SVG, so an accumulating history would add a near-complete copy
# of the whole site per publish.
[[ "$(basename "$STAGE")" == "gog-book-site" ]] || die "refusing to remove $STAGE"
rm -rf "$STAGE"
mkdir -p "$STAGE"

rsync -a --exclude '.git' "$BOOK/_book/" "$STAGE/"

# Required. The output contains `site_libs/`, and GitHub Pages runs Jekyll by
# default, which strips underscore-prefixed paths. Without this file every
# stylesheet and script 404s and the site serves unstyled.
touch "$STAGE/.nojekyll"

cp "$BOOK/LICENSE.md" "$STAGE/LICENSE.md"

# --------------------------------------------------------------------------
say "Publishing"
SOURCE_REV="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
if ! git -C "$REPO_ROOT" diff --quiet -- book/; then
  SOURCE_REV="$SOURCE_REV+dirty"
fi

cd "$STAGE"
git init -q -b main
git add -A

# The message carries a date and nothing else. It used to name the monorepo
# revision the render came from, which was the one thing in the public repository
# pointing back at the private one — a commit that does not exist in any repo a
# reader can see. The provenance is still useful to the author, so it is printed
# to the terminal below instead of published.
git -c user.name="$(git -C "$REPO_ROOT" config user.name)" \
    -c user.email="$(git -C "$REPO_ROOT" config user.email)" \
    commit -q -m "GOG: A Grammar of Graphics — $(date -u '+%Y-%m-%d')"
git remote add origin "$REMOTE"
git push -q --force origin main

# One commit, no parent, one branch — checked rather than assumed, because the
# whole point is that this cannot drift.
COMMITS="$(git rev-list --count HEAD)"
[[ "$COMMITS" == "1" ]] || die "the site commit has $COMMITS ancestors, expected 1"

say "Published"
echo "  source    $SOURCE_REV"
echo "  site      https://psychometrician.github.io/gog-book/"
echo "  repo      https://github.com/psychometrician/gog-book"
