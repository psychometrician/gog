// Every exported name has help text, and every help example runs and draws.
//
//     node --test js-pkg/gog/test/help.test.mjs
//
// The mirror of the block in R's `test_basic.R` that runs every help page's
// example. JavaScript's help is the JSDoc block right above a definition: an
// editor shows it on hover, following a re-export in `index.js` to the
// definition in `src/*.js`. So the blocks are read from there, and the list of
// names comes from `index.js` itself, never from a copy kept here.
//
// Three things are checked, and each can fail on its own:
//
//   1. every export has a block, and the block has an `@example`;
//   2. every argument the function takes is named by an `@param`, and every
//      `@param` names an argument the function takes;
//   3. every example runs, writes no gog warning, and every plot or
//      arrangement it builds draws through `render_svg`.
//
// An example whose caption begins "Not run" is shown and not run. Only
// `gog_table()` has one, because it fetches its table over the network.

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import * as gog from "../src/index.js";
import { Query } from "../src/render.js";

const SRC = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "src");
const EXPORTS = Object.keys(gog).sort();
const IDENT = "[A-Za-z_$][\\w$]*";

// ---------------------------------------------------------------------------
// Reading the sources
// ---------------------------------------------------------------------------

// Every top-level definition in a file, with its text: from its first line to
// the line before the next top-level definition.
function definitions(file) {
  const text = fs.readFileSync(path.join(SRC, file), "utf8");
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

const FILES = fs.readdirSync(SRC).filter((f) => f.endsWith(".js"));
const DEFS = FILES.flatMap(definitions);

// The JSDoc block directly above a definition: nothing but white space may
// stand between its `*/` and the definition, or an editor does not attach it.
function docBlock(def) {
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
function parseBlock(block) {
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
  const params = tags.filter((t) => t.tag === "param").map((t) => paramName(t.lines[0]));
  const examples = tags.filter((t) => t.tag === "example").map((t) => {
    const caption = t.lines[0].match(/<caption>(.*)<\/caption>/)?.[1] ?? "";
    const code = t.lines.slice(caption ? 1 : 0).join("\n").trim();
    return { caption, code };
  });
  const returns = tags.some((t) => t.tag === "returns");
  return { description, params, examples, returns };
}

// The name an `@param` documents: what follows its `{type}`, without the
// brackets that mark it optional or a default after `=`. The type is read to
// its matching brace, since a type may hold braces of its own.
function paramName(line) {
  if (!line.startsWith("{")) return null;
  let depth = 0;
  let end = -1;
  for (let i = 0; i < line.length; i += 1) {
    if (line[i] === "{") depth += 1;
    else if (line[i] === "}" && --depth === 0) {
      end = i;
      break;
    }
  }
  const m = end < 0 ? null : line.slice(end + 1).match(/^\s+\[?([\w$.]+)/);
  return m ? m[1] : null;
}

// The definition of an exported name. A name re-exported from another file is
// followed there, so this finds the `export const|function|class` itself.
function definitionOf(name) {
  return DEFS.find((d) => d.exported && d.name === name) ?? null;
}

// The local helpers a definition calls: the file's top-level functions and
// constants that are not themselves public, followed a few levels down. They
// are where `x()` reaches `readArgs`, and where `across()` reads `wrap`.
function helperTexts(def, depth = 0, seen = new Set([def.name])) {
  if (depth > 4) return [];
  const local = DEFS.filter((d) => d.file === def.file && !EXPORTS.includes(d.name));
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
  const text = fs.readFileSync(path.join(SRC, file), "utf8");
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
  const text = fs.readFileSync(path.join(SRC, file), "utf8");
  const object = text.match(new RegExp(`^const\\s+${name}\\s*=\\s*\\{([\\s\\S]*?)^\\};`, "m"));
  if (!object) return [];
  return [...object[1].matchAll(new RegExp(`^\\s*(${IDENT})\\s*:`, "gm"))].map((m) => m[1]);
}

// The text inside the parentheses that open at `open`.
function balanced(text, open) {
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

function splitParams(list) {
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

// ---------------------------------------------------------------------------
// Running an example
// ---------------------------------------------------------------------------

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;

// Runs one example with every export in scope, as `import { … } from
// "grammar-of-graphics"` would put it, and returns each plot and arrangement it
// built. Each export is wrapped so that what it returns is kept; the wrapper is
// a proxy, so a bare atom keeps the atom it carries and a class keeps its
// prototype for `instanceof`.
async function runExample(code, printed) {
  const built = [];
  const keep = (value) => {
    if (value instanceof gog.Plot || value instanceof gog.Arrangement) built.push(value);
    return value;
  };
  const values = EXPORTS.map((name) => {
    const value = gog[name];
    if (typeof value !== "function") return value;
    return new Proxy(value, {
      apply: (target, self, args) => keep(Reflect.apply(target, self, args)),
    });
  });
  const quiet = {
    log: (...args) => printed.push(args.map(String).join(" ")),
    error: (...args) => printed.push(args.map(String).join(" ")),
    warn: (...args) => printed.push(args.map(String).join(" ")),
  };
  const fn = new AsyncFunction(...EXPORTS, "console", code);
  await fn(...values, quiet);
  return built;
}

// Everything written to stderr while `body` runs, which is where the binding
// and the engine both put a warning (`gog: …`).
async function stderrOf(body) {
  const write = process.stderr.write;
  let said = "";
  process.stderr.write = (chunk) => {
    said += chunk;
    return true;
  };
  try {
    await body();
  } finally {
    process.stderr.write = write;
  }
  return said;
}

// A plot whose table is a `query()` is read at render, and its example closes
// the connection after drawing, as R's does.
const readsQuery = (figure) =>
  Object.values(figure.frames).some((table) => table instanceof Query);

// ---------------------------------------------------------------------------
// The checks
// ---------------------------------------------------------------------------

test("the export list comes from the package, and is not empty", () => {
  assert.ok(EXPORTS.length > 90, `only ${EXPORTS.length} exports`);
  for (const name of ["plot", "layer", "point", "x", "render_svg", "col", "gog_table"]) {
    assert.ok(EXPORTS.includes(name), name);
  }
});

test("every export has a JSDoc block with an example", () => {
  const missing = [];
  for (const name of EXPORTS) {
    const def = definitionOf(name);
    const block = def && docBlock(def);
    if (!block) {
      missing.push(`${name}: no JSDoc block directly above its definition`);
      continue;
    }
    const doc = parseBlock(block);
    if (!doc.description) missing.push(`${name}: no description`);
    if (!doc.examples.length) missing.push(`${name}: no @example`);
    // A class, `col` and a bare mark or transform are values rather than calls,
    // so there is nothing for them to return.
    const value = def.kind === "class" || /=\s*(new\s+Proxy|bareAtom)\(/.test(def.text.split("\n")[0]);
    if (!value && !doc.returns) missing.push(`${name}: no @returns`);
  }
  assert.deepEqual(missing, []);
});

test("every argument a function takes is named by an @param, and no other", () => {
  const wrong = [];
  for (const name of EXPORTS) {
    const def = definitionOf(name);
    const block = def && docBlock(def);
    if (!block) continue; // reported by the test above
    const documented = parseBlock(block).params;
    const declared = declaredParams(def);
    const options = optionNames(def);
    for (const p of declared) {
      if (!documented.includes(p)) wrong.push(`${name}: argument \`${p}\` has no @param`);
    }
    for (const key of options) {
      if (!documented.some((d) => d === key || d?.endsWith(`.${key}`))) {
        wrong.push(`${name}: option \`${key}\` has no @param`);
      }
    }
    // The other direction: an @param for an argument the code does not take is
    // a claim about the function that is not true.
    for (const d of documented) {
      if (d === null) {
        wrong.push(`${name}: an @param has no {type} or no name`);
        continue;
      }
      const [root, ...rest] = d.split(".");
      const leaf = rest.at(-1);
      const rootKnown = declared.includes(root) || options.includes(root) || root === "options";
      const leafKnown = leaf === undefined || options.includes(leaf);
      if (!rootKnown || !leafKnown) wrong.push(`${name}: @param \`${d}\` is not an argument it takes`);
    }
  }
  assert.deepEqual(wrong, []);
});

test("every help example runs, warns of nothing, and draws every plot it builds", async (t) => {
  const here = process.cwd();
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "gog-help-"));
  // `show()` writes to the system's folder for temporary files, so that folder
  // is this one while the examples run, and is removed with it.
  const env = { TMPDIR: process.env.TMPDIR, TMP: process.env.TMP, TEMP: process.env.TEMP };
  let examples = 0;
  let drawn = 0;
  const failures = [];
  try {
    process.chdir(dir);
    Object.assign(process.env, { TMPDIR: dir, TMP: dir, TEMP: dir });
    for (const name of EXPORTS) {
      const def = definitionOf(name);
      const block = def && docBlock(def);
      if (!block) continue;
      let ran = 0;
      let figures = 0;
      for (const [i, example] of parseBlock(block).examples.entries()) {
        if (/^Not run/.test(example.caption)) continue;
        ran += 1;
        const where = `${name}, example ${i + 1}`;
        const printed = [];
        let built = [];
        const said = await stderrOf(async () => {
          try {
            built = await runExample(example.code, printed);
          } catch (error) {
            failures.push(`${where} stopped: ${error.message}`);
            return;
          }
          figures += built.length;
          for (const figure of built) {
            if (readsQuery(figure)) continue;
            try {
              const svg = gog.render_svg(figure);
              assert.match(svg, /^<svg/);
              drawn += 1;
            } catch (error) {
              failures.push(`${where}: ${figure} does not draw: ${error.message}`);
            }
          }
        });
        // The engine reports each file it wrote with a `gog: wrote` line, which
        // is a note of what was done rather than a warning.
        const warned = said.split("\n").filter((line) => /gog:/.test(line) && !/^gog: wrote /.test(line));
        if (warned.length) failures.push(`${where} warned: ${warned.join("\n")}`);
        examples += 1;
      }
      // Help for a name shows it at work in a plot, so the examples a name
      // runs build at least one, as R's do.
      if (ran && !figures) failures.push(`${name}: its examples build no plot`);
    }
  } finally {
    process.chdir(here);
    for (const [key, value] of Object.entries(env)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    fs.rmSync(dir, { recursive: true, force: true });
  }
  assert.deepEqual(failures, []);
  assert.ok(examples >= EXPORTS.length, `only ${examples} examples ran`);
  t.diagnostic(`${examples} help examples ran, and ${drawn} plots drew`);
});
