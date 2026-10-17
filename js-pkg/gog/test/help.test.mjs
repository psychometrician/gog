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

import * as gog from "../src/index.js";
import { Query } from "../src/render.js";
import { docBlock, parseBlock, sources } from "../tools/jsdoc.mjs";

const EXPORTS = Object.keys(gog).sort();

// ---------------------------------------------------------------------------
// Reading the sources
// ---------------------------------------------------------------------------

// The blocks and the code are read by `tools/jsdoc.mjs`, which `tools/types.mjs`
// reads them with as well, so the help this checks and the TypeScript
// declarations written from it can never be two readings of one block.
const { definitionOf, declaredParams, optionNames } = sources(EXPORTS);

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
