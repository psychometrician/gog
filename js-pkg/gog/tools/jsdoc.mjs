// jsdoc.mjs — the help blocks in `src/*.js`, read once for two readers.
//
// JavaScript's help is the JSDoc block right above a definition. Two things
// read those blocks, and they read them with this one module so that they can
// never disagree about what a block says:
//
//   * `test/help.test.mjs`, which checks that every export has a block, that
//     every `@param` names an argument the function takes, and that every
//     `@example` runs;
//   * `tools/types.mjs`, which writes `index.d.ts` from the same blocks, so a
//     TypeScript reader gets the help a JavaScript reader gets.
//
// It reads source text and runs nothing, so it needs nothing installed.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const PACKAGE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const SRC = path.join(PACKAGE, "src");
export const IDENT = "[A-Za-z_$][\\w$]*";

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

// Every top-level definition in a file, with its text: from its first line to
// the line before the next top-level definition.
export function definitions(file, src = SRC) {
  const text = fs.readFileSync(path.join(src, file), "utf8");
  const starts = [];
  const head = new RegExp(
    `^(export\\s+)?(?:async\\s+)?(function|const|let|class)\\s+(${IDENT})`, "gm");
  for (const m of text.matchAll(head)) {
    starts.push({ at: m.index, exported: Boolean(m[1]), kind: m[2], name: m[3] });
  }
  return starts.map((start, i) => {
    const end = i + 1 < starts.length ? starts[i + 1].at : text.length;
    // The code alone: the comments after a definition belong to the next one,
    // and the next one's examples must not be read as this one's arguments.
    const code = text.slice(start.at, end)
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/^\s*\/\/.*$/gm, "");
    return { ...start, file, text: code, before: text.slice(0, start.at) };
  });
}

// The names `index.js` exports, in the order it lists them.
export function exportOrder(src = SRC) {
  const text = fs.readFileSync(path.join(src, "index.js"), "utf8");
  const names = [];
  for (const m of text.matchAll(/^export\s*\{([^}]*)\}\s*from\s*"([^"]+)"/gm)) {
    const list = m[1].replace(/\/\/.*$/gm, "");
    for (const item of list.split(",").map((s) => s.trim()).filter(Boolean)) {
      names.push(item.split(/\s+as\s+/).at(-1));
    }
  }
  return names;
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

// The JSDoc block directly above a definition: nothing but white space may
// stand between its `*/` and the definition, or an editor does not attach it.
export function docBlock(def) {
  const before = def.before.trimEnd();
  if (!before.endsWith("*/")) return null;
  const open = before.lastIndexOf("/**");
  if (open < 0) return null;
  const body = before.slice(open + 3, -2);
  if (body.includes("*/")) return null;
  return body.split("\n").map((line) => line.replace(/^\s*\* ?/, "")).join("\n");
}

// A block's description and its tags. A tag starts a line; everything up to
// the next tag is its text, so an example keeps its own line breaks.
export function parseBlock(block) {
  const tags = [];
  let description = [];
  let current = null;
  for (const line of block.split("\n")) {
    const tag = line.match(/^@(\w+)\s?(.*)$/);
    if (tag) {
      current = { tag: tag[1], lines: [tag[2]] };
      tags.push(current);
    } else if (current) {
      current.lines.push(line);
    } else {
      description.push(line);
    }
  }
  description = description.join("\n").trim();
  const paramTags = tags.filter((t) => t.tag === "param").map((t) => readParam(t.lines));
  const params = paramTags.map((p) => p.name);
  const examples = tags.filter((t) => t.tag === "example").map((t) => {
    const caption = t.lines[0].match(/<caption>(.*)<\/caption>/)?.[1] ?? "";
    const code = t.lines.slice(caption ? 1 : 0).join("\n").trim();
    return { caption, code };
  });
  const returnsTag = tags.find((t) => t.tag === "returns");
  const returns = Boolean(returnsTag);
  const returnsType = returnsTag ? splitType(returnsTag.lines[0])?.type ?? null : null;
  const typeTag = tags.find((t) => t.tag === "type");
  const type = typeTag ? splitType(typeTag.lines[0])?.type ?? null : null;
  return { description, params, paramTags, examples, returns, returnsType, type, tags };
}

// A tag's leading `{type}`, read to its matching brace, since a type may hold
// braces of its own; and the text after it.
export function splitType(line) {
  if (!line.startsWith("{")) return null;
  let depth = 0;
  for (let i = 0; i < line.length; i += 1) {
    if (line[i] === "{") depth += 1;
    else if (line[i] === "}" && --depth === 0) {
      return { type: line.slice(1, i).trim(), rest: line.slice(i + 1) };
    }
  }
  return null;
}

// The name an `@param` documents: what follows its `{type}`, without the
// brackets that mark it optional or a default after `=`.
export function paramName(line) {
  const split = splitType(line);
  const m = split?.rest.match(/^\s+\[?([\w$.]+)/);
  return m ? m[1] : null;
}

// Everything an `@param` says: its type, its name, whether it may be left out,
// and its description, which runs on over the lines after the first.
export function readParam(lines) {
  const name = paramName(lines[0]);
  const split = splitType(lines[0]);
  if (!split || !name) return { name, type: null, optional: false, rest: false, text: "" };
  const head = split.rest.match(/^\s+(\[([\w$.]+)(?:=([^\]]*))?\]|[\w$.]+)\s*(.*)$/);
  const optional = Boolean(head?.[2]);
  const first = head?.[4] ?? "";
  const text = [first, ...lines.slice(1)].map((l) => l.trim()).join("\n").trim();
  const rest = split.type.startsWith("...");
  return {
    name,
    type: rest ? split.type.slice(3).trim() : split.type,
    optional,
    rest,
    text,
  };
}

// ---------------------------------------------------------------------------
// Code
// ---------------------------------------------------------------------------

// The text inside the parentheses that open at `open`.
export function balanced(text, open) {
  let depth = 0;
  for (let i = open; i < text.length; i += 1) {
    if (text[i] === "(") depth += 1;
    else if (text[i] === ")") {
      depth -= 1;
      if (depth === 0) return text.slice(open + 1, i);
    }
  }
  return "";
}

export function splitParams(list) {
  const out = [];
  let depth = 0;
  let current = "";
  for (const c of list) {
    if ("([{".includes(c)) depth += 1;
    if (")]}".includes(c)) depth -= 1;
    if (c === "," && depth === 0) {
      out.push(current);
      current = "";
    } else {
      current += c;
    }
  }
  out.push(current);
  return out.map((p) => p.trim()).filter(Boolean).map((p) => p.split("=")[0].trim());
}

// What the definitions in `src` say, given the names the package exports.
// The exports matter because a helper is a definition that is not one.
export function sources(exportNames, src = SRC) {
  const files = fs.readdirSync(src).filter((f) => f.endsWith(".js"));
  const defs = files.flatMap((file) => definitions(file, src));

  // The definition of an exported name. A name re-exported from another file is
  // followed there, so this finds the `export const|function|class` itself.
  const definitionOf = (name) => defs.find((d) => d.exported && d.name === name) ?? null;

  // The local helpers a definition calls: the file's top-level functions and
  // constants that are not themselves public, followed a few levels down. They
  // are where `x()` reaches `readArgs`, and where `across()` reads `wrap`.
  function helperTexts(def, depth = 0, seen = new Set([def.name])) {
    if (depth > 4) return [];
    const local = defs.filter((d) => d.file === def.file && !exportNames.includes(d.name));
    const out = [];
    for (const m of def.text.matchAll(new RegExp(`\\b(${IDENT})\\s*\\(`, "g"))) {
      const helper = local.find((d) => d.name === m[1] && !seen.has(d.name));
      if (!helper) continue;
      seen.add(helper.name);
      out.push(helper, ...helperTexts(helper, depth + 1, seen));
    }
    return out;
  }

  // The strings of a top-level `const NAME = [ ... ]` in a file, following
  // `...OTHER` and `...Object.keys(OTHER)` inside it.
  function constList(file, name) {
    const text = fs.readFileSync(path.join(src, file), "utf8");
    const array = text.match(new RegExp(`^(?:export\\s+)?const\\s+${name}\\s*=\\s*\\[([^\\]]*)\\]`, "m"));
    if (array) {
      const out = [];
      for (const item of array[1].split(",").map((s) => s.trim()).filter(Boolean)) {
        const keys = item.match(new RegExp(`^\\.\\.\\.Object\\.keys\\((${IDENT})\\)$`));
        const spread = item.match(new RegExp(`^\\.\\.\\.(${IDENT})$`));
        if (keys) out.push(...constKeys(file, keys[1]));
        else if (spread) out.push(...constList(file, spread[1]));
        else if (/^["']/.test(item)) out.push(item.slice(1, -1));
      }
      return out;
    }
    return [];
  }

  function constKeys(file, name) {
    const text = fs.readFileSync(path.join(src, file), "utf8");
    const object = text.match(new RegExp(`^const\\s+${name}\\s*=\\s*\\{([\\s\\S]*?)^\\};`, "m"));
    if (!object) return [];
    return [...object[1].matchAll(new RegExp(`^\\s*(${IDENT})\\s*:`, "gm"))].map((m) => m[1]);
  }

  // The parameters a definition declares, as a caller writes them. A function
  // takes what its signature says; a class, what its constructor says. An atom
  // made by `callableAtom` or a channel helper takes what its arrow function
  // says, and `...raw` there is only the carrier for `readArgs`, whose names are
  // the real arguments (`optionNames`).
  function declaredParams(def) {
    let list = null;
    if (def.kind === "function") {
      list = balanced(def.text, def.text.indexOf("("));
    } else if (def.kind === "class") {
      const at = def.text.search(/\bconstructor\s*\(/);
      list = at < 0 ? "" : balanced(def.text, def.text.indexOf("(", at));
    } else {
      const init = def.text.slice(def.text.indexOf("=") + 1).trimStart();
      const arrow = def.text.match(/\(([^()]*)\)\s*=>/);
      const call = init.match(new RegExp(`^(${IDENT})\\s*\\(`));
      const helper = call && helperTexts(def).find((h) => h.name === call[1]);
      if (/^new\s+Proxy\(|^bareAtom\(/.test(init)) list = "";
      else if (arrow) list = arrow[1];
      else if (helper) list = helper.text.match(/\(([^()]*)\)\s*=>/)?.[1] ?? "";
      else list = "";
    }
    return splitParams(list).filter((p) => p !== "...raw").map((p) => p.replace(/^\.\.\./, ""));
  }

  // The names `readArgs` reads for a definition, in the order it reads them
  // positionally, or `null` when the definition does not read its arguments
  // through `readArgs`. A list handed over in the definition itself wins over
  // the default its helper falls back to: `play` hands `channelAtom` its own.
  function readArgsList(def) {
    for (const part of [def, ...helperTexts(def)]) {
      const text = part.text;
      const params = new Map();
      if (part.kind === "function") {
        for (const p of balanced(text, text.indexOf("(")).split(",")) {
          const [name, value] = p.split("=").map((s) => s?.trim());
          if (value) params.set(name, value);
        }
      }
      const handed = text.match(/,\s*raw\s*,\s*(\[[^\]]*\])/);
      const read = text.match(/readArgs\(\s*raw\s*,[^,]+,\s*(\[[^\]]*\]|[\w$]+)\s*\)/);
      const list = handed?.[1] ?? read?.[1];
      if (!list) continue;
      if (list.startsWith("[")) return [...list.matchAll(/["']([\w$]+)["']/g)].map((s) => s[1]);
      return constList(part.file, params.get(list) ?? list);
    }
    return null;
  }

  // The names a function accepts besides its declared parameters: the lists
  // handed to `readArgs`, or passed on to it after `raw`; the keys a function
  // takes out of its options object (`const { wrap } = options`, `options.scale`);
  // and a list of accepted keys kept in a constant named `*_ARGS` or `*_PROPS`.
  function optionNames(def) {
    const out = new Set();
    for (const part of [def, ...helperTexts(def)]) {
      const text = part.text;
      const params = new Map();
      if (part.kind === "function") {
        for (const p of balanced(text, text.indexOf("(")).split(",")) {
          const [name, value] = p.split("=").map((s) => s?.trim());
          if (value) params.set(name, value);
        }
      }
      const lists = [
        ...text.matchAll(/readArgs\(\s*raw\s*,[^,]+,\s*(\[[^\]]*\]|[\w$]+)\s*\)/g),
        ...text.matchAll(/,\s*raw\s*,\s*(\[[^\]]*\])/g),
      ].map((m) => m[1]);
      for (const list of lists) {
        if (list.startsWith("[")) {
          for (const s of list.matchAll(/["']([\w$]+)["']/g)) out.add(s[1]);
        } else {
          const name = params.get(list) ?? list;
          for (const s of constList(part.file, name)) out.add(s);
        }
      }
      for (const m of text.matchAll(/const\s*\{([^}]*)\}\s*=\s*(?:options|opts|settings)\b/g)) {
        for (const key of splitParams(m[1])) if (!key.startsWith("...")) out.add(key);
      }
      for (const m of text.matchAll(/\b(?:options|opts|settings)\.([\w$]+)/g)) out.add(m[1]);
      for (const m of text.matchAll(/\b([A-Z][A-Z_]*_(?:ARGS|PROPS))\b/g)) {
        for (const s of constList(part.file, m[1])) out.add(s);
      }
    }
    return [...out];
  }

  return { defs, definitionOf, helperTexts, declaredParams, optionNames, readArgsList };
}
