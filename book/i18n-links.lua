-- book/i18n-links.lua
-- In a translated edition, a link to another chapter goes to that chapter's
-- translation.
--
-- A translated chapter keeps every link target exactly as the English writes
-- it, `selection.qmd#pointing-at-a-mark`, so a check can compare the two files
-- link by link. The edition sits beside the English chapters with its language
-- before the extension, `selection.ko.qmd`, and in a translated render every
-- chapter has one (a chapter not translated yet is rendered from its English
-- text under that name). So when the document's `lang` is set and is not
-- English, a relative link to `P.qmd` becomes `P.<lang>.qmd` wherever that file
-- exists, keeping its `#id`, which is the same in every language.
--
-- The English book sets no `lang`, and there this filter changes nothing.

local lang = nil

local function exists(path)
  local f = io.open(path, "r")
  if f then f:close() return true end
  return false
end

local function dir_of(path)
  return path:match("^(.*)/[^/]*$") or "."
end

function Meta(meta)
  if meta.lang then lang = pandoc.utils.stringify(meta.lang) end
end

function Link(link)
  if not lang or lang == "" or lang:match("^en") then return nil end
  local code = lang:match("^(%a%a)")
  local target = link.target
  if target:match("^%a[%w+.-]*:") then return nil end          -- a URL
  local page, anchor = target:match("^([^#]+)%.qmd(#?.*)$")
  if not page or page:match("%.%a%a$") then return nil end       -- already translated
  local here = quarto and quarto.doc and quarto.doc.input_file or ""
  local candidate = page .. "." .. code .. ".qmd"
  if exists(dir_of(here) .. "/" .. candidate) then
    link.target = candidate .. anchor
    return link
  end
  return nil
end

return { { Meta = Meta }, { Link = Link } }
