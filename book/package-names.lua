-- A package name is set in code font, in every chapter and every appendix.
--
-- The source writes these names as plain words, and `check_naming.R` holds them
-- there: the name is the name, in one case. That rule is about *spelling*. This
-- filter is about *type*, and the two are separate decisions. Nothing in a
-- `.qmd` changes, so a writer keeps typing `gog` and the guard keeps checking
-- the one thing it was written to check.
--
-- **Why a filter, and not 350 pairs of backticks.** `gog` is written 247 times
-- across 45 of the 59 chapters, and `ggplot2` 66 more. Marking them by hand puts
-- the convention in the hands of whoever writes the next sentence, which is
-- exactly how every other convention in this book drifted: by chapter, because a
-- writer holds one file in their head for an afternoon. A filter cannot drift,
-- an appendix cannot be missed, and a chapter written a year from now is covered
-- without anyone remembering that this rule exists.
--
-- `Code` is the one form both formats already agree about: `<code>` in HTML and
-- `\texttt{}` in the PDF. So the site and the printed book say the same thing
-- without a second definition of it anywhere.

-- Libraries, not applications. Quarto, RStudio, Positron, Excel, Tableau, Stata
-- and SPSS are programs a reader runs, and `Quarto`'s would read as something to
-- type. The line this list draws is one question: could a sentence hand it to
-- `library()`, `import` or `Pkg.add`? Then it is code.
--
-- Longest first, so a name can never match inside a longer one.
local NAMES = {
  "matplotlib", "tidyverse", "ggplot2", "magrittr", "jsonlite",
  "lattice", "plotly", "pandas", "dplyr", "knitr", "gog",
}

-- What the boundary test is protecting, all of it real text in this book:
-- `agog`, which is the whole pun and contains the package name; `gog-cli` and
-- `gog-core`, which are filenames; `GOG_STRICT`, which survives on case alone;
-- and the `/gog` ending a URL written as its own link text. A name may still be
-- followed by ordinary punctuation, so `gog.` at the end of a sentence and
-- `ggplot2's` both match, while `gog.dev` does not.
--
-- The classes are written out as ASCII ranges rather than as `%w`, and that is
-- not style. Lua matches bytes, and pandoc has already turned `'` into a curly
-- quote by the time a filter sees it, so `ggplot2's` ends in the first byte of a
-- three-byte character. Under `%w` that byte read as a letter and the name went
-- unmarked, which is the sort of thing that shows up as one plain word in a
-- chapter nobody rereads.
local function boundary_ok(s, i, j)
  local before = i > 1 and s:sub(i - 1, i - 1) or ""
  local after = s:sub(j + 1, j + 1)
  if before ~= "" and before:match("[A-Za-z0-9_/%-%.]") then return false end
  if after ~= "" and after:match("[A-Za-z0-9_/%-]") then return false end
  if after == "." and s:sub(j + 2, j + 2):match("[A-Za-z0-9]") then return false end
  return true
end

-- Pandoc splits text on whitespace, so one `Str` is `gog,` or `(gog)` or
-- `ggplot2's`. Each one is walked character by character and rebuilt as a run of
-- inlines, which is the only way to reach a name with punctuation stuck to it.
local function split(s)
  local out, buf, i, hit_any = {}, {}, 1, false
  while i <= #s do
    local hit = nil
    for _, name in ipairs(NAMES) do
      local j = i + #name - 1
      if s:sub(i, j) == name and boundary_ok(s, i, j) then
        hit = name
        break
      end
    end
    if hit then
      if #buf > 0 then
        out[#out + 1] = pandoc.Str(table.concat(buf))
        buf = {}
      end
      out[#out + 1] = pandoc.Code(hit)
      hit_any = true
      i = i + #hit
    else
      buf[#buf + 1] = s:sub(i, i)
      i = i + 1
    end
  end
  if #buf > 0 then out[#out + 1] = pandoc.Str(table.concat(buf)) end
  if not hit_any then return nil end
  return out
end

return {
  {
    -- Top-down, because two things have to be refused *before* their children
    -- are reached. Bottom-up would rewrite the text first and hand back an
    -- element that had already lost.
    traverse = "topdown",

    -- The slogan is the one place the name is not a package. "Be agog. Use gog."
    -- is a line of copy under the title, and setting half of it in code font
    -- would break the rhyme the whole name rests on. Its printed twin is raw
    -- LaTeX, which no filter walks, so only the HTML div needs saying here.
    Div = function(el)
      if el.classes:includes("gog-slogan") then return el, false end
    end,

    -- Already code. A chunk's source and its output never reach a `Str` filter
    -- at all, so this is only for inline spans a writer marked by hand.
    Code = function(el) return el, false end,

    Str = function(el)
      local out = split(el.text)
      if out == nil then return nil end
      -- `false` stops the walk from re-entering what was just built.
      return out, false
    end,
  },
}
