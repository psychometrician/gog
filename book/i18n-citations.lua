-- book/i18n-citations.lua
-- In a translated edition, citations and references are written in English.
--
-- A cited work is cited as its authors published it, and a reference list is
-- the same list in every edition: "(Wickham et al., 2026)", never
-- "(Wickham 기타, 2026)". Pandoc's citation step localizes its words and dates
-- by the document's `lang`, and puts a title in sentence case only for English,
-- so this filter runs that step itself with the language set to English, then
-- puts the edition's language back.
--
-- Pandoc keeps each citation it formats, so Quarto's own citation step, which
-- runs after the book's filters, would format it again in the edition's
-- language. Each one is therefore handed on as the span Pandoc writes for a
-- citation, its links marked as Quarto marks them (`doc-biblioref`), so the
-- page is the English book's markup with the edition's prose around it.
--
-- The English book sets no `lang`, and there this filter changes nothing.

function Pandoc(doc)
  local lang = doc.meta.lang and pandoc.utils.stringify(doc.meta.lang) or ""
  if lang == "" or lang:match("^en") then return nil end
  doc.meta.lang = "en-US"
  doc = pandoc.utils.citeproc(doc)
  doc.meta.lang = lang
  return doc:walk({
    Cite = function(c)
      local ids = {}
      for _, cit in ipairs(c.citations) do ids[#ids + 1] = cit.id end
      local content = c.content:walk({
        Link = function(l)
          if l.target:match("^#ref%-") then l.attributes.role = "doc-biblioref" end
          return l
        end,
      })
      return pandoc.Span(content, { class = "citation", ["data-cites"] = table.concat(ids, " ") })
    end,
  })
end
