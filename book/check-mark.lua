-- The check mark on the combination grids is one mark in both editions.
--
-- The source writes ✅, and the guards read it there. But ✅ is a color emoji,
-- and no font in TeX Live carries it, so the PDF could not draw it: every ✅ cell
-- printed empty, which on a legality grid reads as a verdict. A PDF that swapped
-- in a plain ✓ would still disagree with the web edition's green box. So both
-- editions draw the same thing, a ✓ in the ink of the text, and the source stays
-- as it is. The mark carries no color because the grid must not depend on
-- telling one hue from another: a green reads as gray to many readers.

local CHECK = "✅"
local MARK = "✓"

local function mark()
  if FORMAT:match("latex") then
    return pandoc.Str(MARK)
  end
  -- U+FE0E asks for the text form, so a browser never swaps in an emoji face.
  return pandoc.Span({ pandoc.Str(MARK .. "\u{FE0E}") }, { class = "gog-check" })
end

-- Pandoc splits text on whitespace, so a `Str` can be `✅` or `✅,`.
function Str(el)
  local s = el.text
  if not s:find(CHECK, 1, true) then return nil end
  local out, i = {}, 1
  while true do
    local j = s:find(CHECK, i, true)
    if not j then break end
    if j > i then out[#out + 1] = pandoc.Str(s:sub(i, j - 1)) end
    out[#out + 1] = mark()
    i = j + #CHECK
  end
  if i <= #s then out[#out + 1] = pandoc.Str(s:sub(i)) end
  return out
end
