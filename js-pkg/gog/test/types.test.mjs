// The TypeScript declarations say what the help says, and declare every name.
//
//     node --test js-pkg/gog/test/types.test.mjs
//
// `index.d.ts` is written from the JSDoc blocks in `src/*.js` by
// `tools/types.mjs`, so that a TypeScript reader gets the types and the help a
// JavaScript reader gets. Three things are checked:
//
//   1. the file is what the generator writes from the blocks as they are now,
//      byte for byte, so a change to the help cannot leave it behind;
//   2. it declares every name `src/index.js` exports, and nothing else;
//   3. TypeScript accepts it under `--strict`, and accepts every `@example`
//      written as a TypeScript file that imports the package.
//
// The third needs a TypeScript compiler, which this package does not depend on
// and CI does not install, so it runs only where `GOG_TSC` names one and is
// skipped elsewhere. Any command that runs `tsc` will do:
//
//     GOG_TSC=/path/to/node_modules/.bin/tsc node --test js-pkg/gog/test/types.test.mjs
//     GOG_TSC="npx -y -p typescript@5 tsc" node --test js-pkg/gog/test/types.test.mjs

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import * as gog from "../src/index.js";
import { PACKAGE, docBlock, parseBlock, sources } from "../tools/jsdoc.mjs";
import { COMMAND, OUTPUT, generate } from "../tools/types.mjs";

const EXPORTS = Object.keys(gog).sort();

// The examples TypeScript refuses for a reason that is not a fault in the
// declarations, by name and example number, with the error codes it reports.
// Each says why, so a change in this list is a change someone can judge.
const REFUSED = {
  // Each shows a refusal by writing the call gog refuses, which the
  // declarations already refuse: a string where a column belongs, and
  // `colour()` with an argument it does not take. In TypeScript the mistake is
  // caught before the program runs, which is the point of the types.
  "GogError#1": { codes: ["TS2345", "TS18046"], why: "x(\"wt\") is the refusal it shows; error is unknown in a catch" },
  "colour#1": { codes: ["TS2554", "TS18046"], why: "colour(col.species) is the refusal it shows; error is unknown in a catch" },
  // Under `strict`, a caught value is `unknown`, so `error.message` needs a
  // check first. A JavaScript reader never meets this.
  "facet#1": { codes: ["TS18046"], why: "error is unknown in a catch" },
  "save#1": { codes: ["TS18046"], why: "error is unknown in a catch" },
  // `cars[m]`, with `m` any string, indexes an object whose keys TypeScript
  // knows, which `noImplicitAny` refuses. A JavaScript reader never meets this.
  "cluster#2": { codes: ["TS7053"], why: "cars[m] indexes a known object with a string" },
};

// ---------------------------------------------------------------------------
// The file and the help agree
// ---------------------------------------------------------------------------

test("index.d.ts is what the generator writes from the help as it is now", () => {
  const written = fs.existsSync(OUTPUT) ? fs.readFileSync(OUTPUT, "utf8") : "";
  assert.ok(written === generate(),
    `index.d.ts does not match the JSDoc in src/*.js. Run \`${COMMAND}\` from ` +
      "the repository root to write it again.");
});

test("index.d.ts declares every name the package exports, and no other", () => {
  const text = fs.readFileSync(OUTPUT, "utf8");
  const declared = new Set(
    [...text.matchAll(/^export declare (?:function|const|class) ([\w$]+)/gm)].map((m) => m[1]));
  const missing = EXPORTS.filter((name) => !declared.has(name));
  const extra = [...declared].filter((name) => !EXPORTS.includes(name));
  assert.deepEqual({ missing, extra }, { missing: [], extra: [] });
});

// ---------------------------------------------------------------------------
// TypeScript accepts it, and the examples
// ---------------------------------------------------------------------------

// The names an example reads from the package: every export it mentions and
// does not declare itself.
function importsOf(code) {
  const own = new Set([
    ...[...code.matchAll(/\b(?:const|let|var|function|class)\s+([A-Za-z_$][\w$]*)/g)].map((m) => m[1]),
    ...[...code.matchAll(/\b(?:const|let|var)\s*\{([^}]*)\}/g)]
      .flatMap((m) => m[1].split(",").map((s) => s.split(":").at(-1).trim())),
  ]);
  const used = new Set(code.match(/[A-Za-z_$][\w$]*/g) ?? []);
  return EXPORTS.filter((name) => used.has(name) && !own.has(name));
}

// `node:sqlite` is the one module an example imports, and its types come from
// `@types/node`, which is not needed for anything else here. These are the
// members the example calls.
const SQLITE = `declare module "node:sqlite" {
  export class DatabaseSync {
    constructor(path: string);
    exec(sql: string): void;
    prepare(sql: string): { run(...values: unknown[]): unknown; all(...values: unknown[]): unknown[] };
    close(): void;
  }
}
`;

test("TypeScript accepts index.d.ts and every help example, under --strict", (t) => {
  const tsc = process.env.GOG_TSC;
  if (!tsc) {
    t.skip("no TypeScript compiler: set GOG_TSC to one to run this check");
    return;
  }

  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "gog-types-"));
  try {
    // A project that installs the package the way npm would, so the import is
    // resolved through `package.json`'s `exports` and `types`, as a user's is.
    fs.mkdirSync(path.join(dir, "node_modules"));
    fs.symlinkSync(PACKAGE, path.join(dir, "node_modules", "grammar-of-graphics"), "dir");
    fs.writeFileSync(path.join(dir, "package.json"), '{ "type": "module" }\n');
    fs.writeFileSync(path.join(dir, "tsconfig.json"), JSON.stringify({
      compilerOptions: {
        strict: true,
        noEmit: true,
        target: "es2022",
        module: "nodenext",
        moduleResolution: "nodenext",
        lib: ["es2023", "dom"],
        types: [],
        skipLibCheck: false,
      },
      include: ["*.ts"],
    }, null, 2));
    fs.writeFileSync(path.join(dir, "node-sqlite.d.ts"), SQLITE);
    fs.writeFileSync(path.join(dir, "package.ts"),
      'import * as gog from "grammar-of-graphics";\nexport const names = Object.keys(gog);\n');

    // One file for each example, named after its export and its number.
    const { definitionOf } = sources(EXPORTS);
    const files = new Map();
    for (const name of EXPORTS) {
      const examples = parseBlock(docBlock(definitionOf(name))).examples;
      for (const [i, example] of examples.entries()) {
        const key = `${name}#${i + 1}`;
        const file = `example-${name}-${i + 1}.ts`;
        const names = importsOf(example.code);
        const head = names.length ? `import { ${names.join(", ")} } from "grammar-of-graphics";\n` : "";
        fs.writeFileSync(path.join(dir, file), `${head}${example.code}\nexport {};\n`);
        files.set(file, key);
      }
    }

    const run = spawnSync(`${tsc} -p "${dir}" --pretty false`, { shell: true, encoding: "utf8" });
    if (run.error || (run.status !== 0 && !/error TS\d+/.test(run.stdout))) {
      assert.fail(`could not run \`${tsc}\`: ${run.error?.message ?? (run.stderr || run.stdout)}`);
    }
    const version = spawnSync(`${tsc} --version`, { shell: true, encoding: "utf8" }).stdout.trim();

    // Every error, by the file it is in.
    const errors = new Map();
    for (const m of run.stdout.matchAll(/^(.+?)\((\d+),\d+\): error (TS\d+): (.*)$/gm)) {
      const file = path.basename(m[1]);
      if (!errors.has(file)) errors.set(file, []);
      errors.get(file).push({ line: Number(m[2]), code: m[3], message: m[4] });
    }

    const wrong = [];
    for (const [file, list] of errors) {
      const key = files.get(file);
      if (!key) {
        // The declarations themselves, or the import of the package.
        for (const e of list) wrong.push(`${file}:${e.line} ${e.code} ${e.message}`);
        continue;
      }
      const expected = REFUSED[key]?.codes ?? [];
      for (const e of list) {
        if (!expected.includes(e.code)) wrong.push(`${key} (line ${e.line - 1}): ${e.code} ${e.message}`);
      }
    }
    // An example listed as refused that TypeScript now accepts: the list is out
    // of date, and says something about the declarations that is no longer so.
    for (const [key, { codes }] of Object.entries(REFUSED)) {
      const file = [...files].find(([, k]) => k === key)?.[0];
      const found = new Set((errors.get(file) ?? []).map((e) => e.code));
      for (const code of codes) {
        if (!found.has(code)) wrong.push(`${key}: listed as refused with ${code}, and TypeScript accepts it`);
      }
    }
    assert.deepEqual(wrong, []);
    t.diagnostic(`${version}: index.d.ts and ${files.size} help examples type-check under --strict; ` +
      `${Object.keys(REFUSED).length} are refused for the reasons listed in this file`);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
