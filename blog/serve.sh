#!/usr/bin/env bash
#
# Read the rendered blog locally, over a plain static server.
#
# The companion to `book/serve.sh`, for the same reason and against the same
# trap. Use this when the job is *reading* the site rather than writing it.
#
# `quarto preview` re-renders a post the first time you open it, and about a
# second into that render it tells every open page to reload. The browser is
# still sitting on the old page waiting for the new one, so the reload arrives
# first and cancels the navigation the click just started. Every link behaves
# that way, and it reads as a broken page rather than a slow one. There is no
# freeze cache here on purpose: every plot is drawn by `gog-cli` at render time,
# so a cache keyed on the `.qmd` would serve stale plots after an engine change.
#
# Served as plain files, none of that happens, and everything the pages fetch is
# already in `_site/`: `gog.wasm` for the turnable plots, `view.js` for a plot's
# controls, and the search index.
#
# A plain file:// open is enough to read the *syntax coloring*, since the
# highlighter is inlined in the page and the stylesheets are relative. It is not
# enough for a plot's controls: those load as a module, and a browser refuses a
# module over file://. That is the case this script exists for.
#
# Usage:  blog/serve.sh          serve on http://127.0.0.1:8898/
#         blog/serve.sh 9000     serve on another port
#
# 8898 rather than the book's 8899, so both can be open at once and a link from
# a post to the book does not land on whichever was started last.
#
# This serves the last render. Edits to a `.qmd`, to `blog.css` or to
# `gog-syntax.html` do not appear until you run `quarto render` again.

set -euo pipefail

PORT="${1:-8898}"
BLOG="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ ! -f "$BLOG/_site/index.html" ]]; then
  printf '\033[31mserve: _site/index.html is missing — there is nothing to read yet.
  Render it first:  cd blog && quarto render\033[0m\n' >&2
  exit 1
fi

printf '\n\033[1m==> Serving the rendered blog\033[0m\n'
printf '  http://127.0.0.1:%s/\n' "$PORT"
printf '  showing the render of %s\n' \
  "$(date -r "$BLOG/_site/index.html" '+%Y-%m-%d %H:%M')"
printf '  stop with Ctrl-C\n\n'

exec python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$BLOG/_site"
