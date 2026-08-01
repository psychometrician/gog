#!/usr/bin/env bash
#
# Read the rendered book locally, over a plain static server.
#
# Use this instead of `quarto preview` when the job is *reading* the book rather
# than writing it. Preview cannot be navigated here, and the reason is worth
# stating because it looks like a broken page rather than a slow one:
#
#   * Preview re-renders a chapter the first time you open it. This book has no
#     freeze cache, on purpose — every plot is drawn by `gog-cli` at render time,
#     so a cache keyed on the `.qmd` would serve stale plots after an engine
#     change. That makes a first visit cost 2 to 16 seconds per chapter.
#   * About a second into that render, the preview server tells every open page
#     to reload.
#   * Your browser is still sitting on the old page, waiting for the new one. The
#     reload arrives first, and `window.location.reload()` cancels the navigation
#     you started. You land back where you were.
#
# So every link fails the same way: the chapter list, and the previous and next
# links at the foot of the page. Nothing is wrong with the rendered book, and
# clicking a second time does not help, because the second click re-renders too.
#
# Served as plain files, none of that happens. Every page arrives in a few
# milliseconds, and search and the turnable 3-D plots work, because `search.json`,
# `gog.wasm` and `interactive.js` are all in `_book/` already.
#
# `quarto preview` is still the right tool while writing a chapter, where
# re-rendering on save is the whole point. It is the wrong tool for reading.
#
# Usage:  book/serve.sh          serve on http://127.0.0.1:8899/
#         book/serve.sh 9000     serve on another port
#
# This serves the last render. Edits to a `.qmd` do not appear until you run
# `quarto render --to html` again.

set -euo pipefail

PORT="${1:-8899}"
BOOK="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ ! -f "$BOOK/_book/index.html" ]]; then
  printf '\033[31mserve: _book/index.html is missing — there is nothing to read yet.
  Render it first:  cd book && quarto render --to html\033[0m\n' >&2
  exit 1
fi

printf '\n\033[1m==> Serving the rendered book\033[0m\n'
printf '  http://127.0.0.1:%s/\n' "$PORT"
printf '  showing the render of %s\n' \
  "$(date -r "$BOOK/_book/index.html" '+%Y-%m-%d %H:%M')"
printf '  stop with Ctrl-C\n\n'

exec python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$BOOK/_book"
