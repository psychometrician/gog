-- Coloring a gog specification in the PDF, which is the browser pass done again
-- in the one place a browser cannot reach.
--
-- `gog-syntax.html` colors a specification by kind in HTML: a table, a mark, a
-- channel, a transform, and one quiet ink for the six that refine. The reason it
-- is needed at all is written there, and it is the same here. Pandoc's four
-- highlighters disagree about the same specification, and every one of them
-- reads `point` as a bare name, so no color scheme can reach it.
--
-- LaTeX has the same shape and none of the same freedom. Every token arrives
-- wrapped, `\FunctionTok{data}\NormalTok{(t) }`, so the structure is there to
-- work with. What is missing is a browser, so this runs at render time and
-- rewrites the block outright.
--
-- Rewriting is safe here in a way it would not be in HTML. There, replacing the
-- block would cost the panel, the copy button and the line anchors that Quarto
-- builds around it. A PDF has none of those, so re-emitting the same
-- `Shaded`/`Highlighting` pair is the whole of it.
--
-- Two things this must get exactly right, both verified rather than reasoned
-- about. The escaping has to match Pandoc's character for character, which
-- `check_latex_escaping.R` settles by rendering the same probes both ways and
-- comparing what is left. And `\begin{Highlighting}[]` has to be re-emitted
-- verbatim, because `_quarto.yml` defines that environment with `fvextra`'s
-- `breaklines`, and a long specification that stops wrapping runs off the page.

if not FORMAT:match("latex") then
  return {}
end

-- ---------------------------------------------------------------------------
-- The vocabulary, read from the chapter that declares it
--
-- The HTML side carries a copy, because a highlighter that waits on a request
-- paints the page twice. Nothing here is in a hurry, so this reads `grammar.qmd`
-- and there is no copy to drift. A new mark in the kernel table is colored by
-- the PDF on the next render with no second edit.
-- ---------------------------------------------------------------------------

-- Four inks for the four kinds that build a plot and are words. The six that
-- refine or arrange share one, because none of them is required. The operator
-- is the fifth builder and takes weight rather than a color, since in three of
-- the four languages it is punctuation and already reads as itself.
local INK = {
  table = "Table", mark = "Mark", channel = "Channel", transform = "Transform",
  scale = "Refine", space = "Refine", setting = "Refine",
  label = "Refine", facet = "Refine", selection = "Refine",
  operator = "Operator",
}

-- JavaScript cannot overload `+ * | /` and spells them as six words. They are
-- absent from the kernel table, which writes the operators as the symbols they
-- are, so they are named here and checked against the package's own exports by
-- `R/check-kernel.R`.
local SPELLED = "plot layer across down beside below"

local function read_kernel()
  local kinds = {}
  -- Two candidates, because half the book's chapters live one directory down in
  -- `marks/`, `parts/`, `bindings/` and `cookbook/`, and it is Quarto's business
  -- rather than ours which directory pandoc is invoked from. Failing loudly is
  -- the point of the error below: a filter that quietly found no words would
  -- print a whole chapter in one ink and look like a decision.
  local path, handle
  for _, candidate in ipairs({ "grammar.qmd", "../grammar.qmd" }) do
    handle = io.open(candidate, "r")
    if handle then path = candidate break end
  end
  if not handle then
    error("gog: gog-syntax.lua cannot find grammar.qmd, so it does not know " ..
          "the kernel. It looked in the working directory and one above it.")
  end
  local inside = false
  for line in handle:lines() do
    if line:match("^## The kernel") then
      inside = true
    elseif inside and line:match("^##") then
      break
    elseif inside and line:match("^|") then
      -- `| **Marks** | `point` `line` ... |`, one row per kind. An atom is
      -- delimited by backticks rather than inferred from the spacing, so a note
      -- written into a cell cannot be mistaken for a name.
      local label = line:match("%*%*(%a+)%*%*")
      if label then
        local kind = label:lower():gsub("s$", "")
        for word in line:gmatch("`([^`]+)`") do
          if word:match("^%l[%l%d_]*$") then
            kinds[kind] = kinds[kind] or {}
            table.insert(kinds[kind], word)
          end
        end
      end
    end
  end
  handle:close()
  if not kinds.mark or not kinds.transform then
    error("gog: gog-syntax.lua could not read the kernel block in " .. path)
  end
  return kinds
end

local ink_of = {}
local words = {}
for kind, list in pairs(read_kernel()) do
  if INK[kind] then
    for _, word in ipairs(list) do
      if not ink_of[word] then
        ink_of[word] = INK[kind]
        table.insert(words, word)
      end
    end
  end
end
for word in SPELLED:gmatch("%S+") do
  if not ink_of[word] then
    ink_of[word] = INK.operator
    table.insert(words, word)
  end
end
-- Longest first, so `x_label` is found before the `x` inside it.
table.sort(words, function(a, b) return #a > #b end)

-- ---------------------------------------------------------------------------
-- Escaping, which has to be Pandoc's own
--
-- Measured from Pandoc's output rather than recalled. Inside `Highlighting` the
-- environment is a `Verbatim` with `commandchars=\\\{\}`, so strictly only
-- backslash and the two braces are special. Pandoc escapes more than that, and
-- the table below is what it actually emits.
--
-- `$` is deliberately absent: Pandoc leaves it alone, because the environment
-- gives it no special meaning, and escaping it here would put a `\$` in the
-- output where Pandoc puts a bare one. The differential test catches exactly
-- that kind of near-miss.
-- ---------------------------------------------------------------------------

local ESCAPE = {
  ["\\"] = "\\textbackslash{}",
  ["{"] = "\\{",
  ["}"] = "\\}",
  ["_"] = "\\_",
  ["#"] = "\\#",
  ["%"] = "\\%",
  ["&"] = "\\&",
  ["^"] = "\\^{}",
  ["~"] = "\\textasciitilde{}",
  ["'"] = "\\textquotesingle{}",
  ["<"] = "\\textless{}",
  [">"] = "\\textgreater{}",
}

local function escape(text)
  -- The backslash has to go first or its own replacement would be re-escaped,
  -- so the pattern matches every special character in one pass instead.
  return (text:gsub("[\\{}_#%%&%^~'<>]", ESCAPE))
end

-- ---------------------------------------------------------------------------
-- Which blocks are read, and where a word counts
--
-- Every word in the kernel is a common English word, which is the grammar's
-- best property and this file's only hazard: `title`, `count`, `mean`, `text`
-- and `group` appear in anyone's code. A block has to name a table before
-- anything in it is read, which every gog specification does and no other
-- library's code does.
-- ---------------------------------------------------------------------------

local LANGUAGES = {
  r = true, python = true, julia = true, js = true, javascript = true,
}

local function is_specification(code)
  return code:find("data%s*%(") ~= nil or code:find("query%s*%(") ~= nil
end

-- A word counts only when it stands alone. `col.count` and `:count` name a
-- column called count, `counted` is somebody's variable, and a word written as
-- an argument name is not an atom: `style(color = "tomato")` sets a property
-- whose name happens to be a channel's.
local function glued_before(char)
  return char ~= "" and char:match("[%w_.$:@]") ~= nil
end

local function glued_after(char)
  return char ~= "" and char:match("[%w_]") ~= nil
end

local function argument_name(rest)
  return rest:match("^%s*[:=]") ~= nil
end

-- ---------------------------------------------------------------------------
-- Painting one line
--
-- A line is scanned for every kernel word, longest first, and the winner at each
-- position is whichever match starts earliest. Pandoc's own tokens are thrown
-- away and replaced: what a host language calls each word is the thing this
-- exists to stop showing.
--
-- A comment or a string is left in the body's ink rather than colored, so a
-- refusal quoted in a comment does not arrive looking like a specification.
-- ---------------------------------------------------------------------------

local function paint_line(line)
  local out = {}
  local at = 1
  while at <= #line do
    local best_from, best_to, best_word
    for _, word in ipairs(words) do
      local from, to = line:find(word, at, true)
      while from do
        local before = from > 1 and line:sub(from - 1, from - 1) or ""
        local after = line:sub(to + 1, to + 1)
        if not glued_before(before) and not glued_after(after)
           and not argument_name(line:sub(to + 1)) then
          break
        end
        from, to = line:find(word, from + 1, true)
      end
      if from and (not best_from or from < best_from
                   or (from == best_from and to > best_to)) then
        best_from, best_to, best_word = from, to, word
      end
    end
    if not best_from then break end
    if best_from > at then
      table.insert(out, "\\NormalTok{" .. escape(line:sub(at, best_from - 1)) .. "}")
    end
    table.insert(out, "\\gog" .. ink_of[best_word] .. "{" ..
                      escape(line:sub(best_from, best_to)) .. "}")
    at = best_to + 1
  end
  if at <= #line then
    table.insert(out, "\\NormalTok{" .. escape(line:sub(at)) .. "}")
  end
  return table.concat(out)
end

function CodeBlock(block)
  local language
  for _, class in ipairs(block.classes) do
    if LANGUAGES[class] then language = class end
  end
  if not language then return nil end
  if not is_specification(block.text) then return nil end

  local lines = {}
  for line in (block.text .. "\n"):gmatch("([^\n]*)\n") do
    table.insert(lines, paint_line(line))
  end
  -- `\begin{Highlighting}[]` exactly as Pandoc writes it, brackets included:
  -- `_quarto.yml` redefines that environment with `fvextra`'s `breaklines`, and
  -- a specification long enough to need wrapping is precisely the one a reader
  -- would lose the end of.
  return pandoc.RawBlock("latex",
    "\\begin{Shaded}\n\\begin{Highlighting}[]\n" ..
    table.concat(lines, "\n") ..
    "\n\\end{Highlighting}\n\\end{Shaded}")
end
