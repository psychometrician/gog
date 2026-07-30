// Run the manual's sentences through the JavaScript binding and compare with R.
//
//     node js-pkg/gog/test/book_parity/run.mjs
//
// The counterpart of `py-pkg/gog/tests/book_parity/run.py`, and held to the same
// bar. The book is 48 chapters of R that the engine draws live, so it is the
// best corpus of real gog sentences in existence; `extract.R` records each one
// with the SVG R got for it, and this runs the same sentence in JavaScript and
// asks whether the two bindings said the same thing.
//
// Three outcomes count as agreement, and they are reported apart because they
// mean different things:
//
//   * the same plot          byte-identical SVG — the engine saw the same spec
//   * the same refusal       word-identical diagnostic — an engine refusal,
//                            which must not depend on who asked
//   * the same refusal,
//     said in JavaScript     a *binding* refusal, whose message teaches the
//                            caller's own syntax; the two texts differ on purpose
//
// This is a stronger claim than the design study that chose this surface made.
// That study asked whether the surface can *express* every sentence; this one
// runs two real bindings and compares what the engine drew. It was removed once
// this harness existed, because a spelling that renders identically subsumes a
// spelling that merely parses.

import { spawnSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import * as gog from "../../src/index.js";
import { GogError } from "../../src/errors.js";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..", "..", "..", "..");
// The corpus is R's recording and language-neutral; it lives beside the Python
// harness because that is where `extract.R` was built, and the JavaScript
// spelling study already read it from there. Three consumers is the point at
// which it should move somewhere neutral. That is an open thread, not done here.
const CORPUS = path.join(ROOT, "py-pkg", "gog", "tests", "book_parity", "corpus");
const STAMP = path.join(ROOT, "py-pkg", "gog", "tests", "book_parity", "corpus_stamp.py");
const TRANSLATOR = path.join(HERE, "translate.R");
const TRANSLATIONS = path.join(HERE, "translations.json");

const read = (file) => JSON.parse(fs.readFileSync(file, "utf8"));

// jsonlite writes a scalar as a one-element array unless told otherwise.
const scalar = (value) => (Array.isArray(value) ? value[0] : value);

// ---------------------------------------------------------------------------
// A table in JavaScript, from the wire form R sent for it
// ---------------------------------------------------------------------------

function rebuild(wire) {
  const table = {};
  const dates = wire.dates || {};
  for (const [name, values] of Object.entries(wire.floats || {})) {
    const unit = scalar(dates[name]);
    table[name] = unit
      ? values.map((v) => (v === null ? null : new Date(v * 1000)))
      : [...values];
  }
  for (const [name, values] of Object.entries(wire.strings || {})) {
    const levels = (wire.levels || {})[name];
    table[name] = levels ? gog.ordered(values, levels) : [...values];
  }
  return table;
}

// ---------------------------------------------------------------------------
// Evaluating one translated sentence
//
// The sentence is an expression, which is what a sentence *is*. It is compiled
// with the grammar's words and the chapter's tables as named parameters rather
// than leaked onto the global object, so two chapters that use one table name
// for different tables stay apart — which the manual does, deliberately.
// ---------------------------------------------------------------------------

// A JavaScript identifier is Unicode, not ASCII: `지역별` is a perfectly legal
// variable name and the manual has a chapter that uses one. An ASCII-only test
// here silently dropped that table from scope and reported it as missing from
// the corpus, which blamed the recording for what this file lost.
const IDENTIFIER = /^[\p{ID_Start}$_][\p{ID_Continue}$‌‍]*$/u;
const RESERVED = new Set(
  ("await break case catch class const continue debugger default delete do else enum export " +
    "extends false finally for function if implements import in instanceof interface let new " +
    "null package private protected public return static super switch this throw true try " +
    "typeof var void while with yield").split(" ")
);
const nameable = (name) => IDENTIFIER.test(name) && !RESERVED.has(name);

const VOCABULARY = Object.keys(gog).filter(nameable);

function evaluate(source, tables) {
  const names = Object.keys(tables).filter(nameable);
  const parameters = [...VOCABULARY, ...names.filter((n) => !VOCABULARY.includes(n))];
  const values = parameters.map((name) =>
    Object.prototype.hasOwnProperty.call(tables, name) ? tables[name] : gog[name]
  );
  // eslint-disable-next-line no-new-func
  const compiled = new Function(...parameters, `"use strict"; return (\n${source}\n);`);
  return compiled(...values);
}

function outcomeOf(source, tables) {
  let plot;
  try {
    plot = evaluate(source, tables);
  } catch (error) {
    if (error instanceof GogError) return { kind: "refused", text: `REFUSED\n${error.message}` };
    if (error instanceof ReferenceError) return { kind: "missing", text: error.message };
    return { kind: "crash", text: `${error.name}: ${error.message}` };
  }
  try {
    const svg = gog.render_svg(plot);
    const hash = crypto.createHash("sha256").update(svg.replace(/\s+$/, ""), "utf8").digest("hex");
    return { kind: "drew", text: `SVG ${hash}` };
  } catch (error) {
    if (error instanceof GogError) return { kind: "refused", text: `REFUSED\n${error.message}` };
    return { kind: "crash", text: `${error.name}: ${error.message}` };
  }
}

// ---------------------------------------------------------------------------

function main() {
  const cli = (() => {
    try {
      return gog.find_gog_cli();
    } catch {
      return "";
    }
  })();

  // **Is the corpus still about this book and this engine?** Asked before any
  // comparison, because a stale corpus does not fail here — it *narrows*. This
  // loop iterates the corpus, so a sentence the manual gained since the last
  // recording is not a disagreement, it is absent, and the run reports a clean
  // pass over a book that no longer exists. One implementation of that question
  // lives in `corpus_stamp.py`; re-deriving its hashes here would be the drift
  // it exists to catch.
  const stamp = spawnSync("python3", [STAMP, "check", ROOT, cli], { encoding: "utf8" });
  if (stamp.status !== 0) {
    console.log("The corpus is not current, so this run would not mean what it says:\n");
    for (const complaint of (stamp.stdout || stamp.stderr).trim().split("\n")) {
      console.log(`  * ${complaint}\n`);
    }
    return 1;
  }

  // One R process for the whole corpus. The emitter is R because the variadic
  // form has to reassociate R's operator tree, so the precedence is R's own.
  const translated = spawnSync(
    "Rscript",
    [
      "-e",
      `source(${JSON.stringify(TRANSLATOR)}); ` +
        `r <- write_js_translations(${JSON.stringify(CORPUS)}, ${JSON.stringify(TRANSLATIONS)}); ` +
        `cat(paste(r$gaps, collapse = "\\n"))`,
    ],
    { encoding: "utf8" }
  );
  if (translated.status !== 0) {
    console.log("The translator failed, so there is nothing to compare:\n");
    console.log(translated.stderr);
    return 1;
  }
  const gaps = (translated.stdout || "").trim();

  const sentences = read(path.join(CORPUS, "sentences.json"));
  const wireTables = read(path.join(CORPUS, "tables.json"));
  const pool = read(path.join(CORPUS, "pool.json")).map(rebuild);
  const translations = new Map(read(TRANSLATIONS).map((t) => [scalar(t.id), t]));

  // `chapter/name` first, then the shared `/name` — the same nearest-wins order
  // the chapters themselves resolve in.
  const byChapter = {};
  for (const [key, index] of Object.entries(wireTables)) {
    const cut = key.lastIndexOf("/");
    const chapter = cut === -1 ? "" : key.slice(0, cut);
    const name = key.slice(cut + 1);
    (byChapter[chapter] ||= {})[name] = pool[scalar(index) - 1];
  }

  const tally = new Map();
  const bump = (name) => tally.set(name, (tally.get(name) || 0) + 1);
  const failures = [];
  const bindingRefusals = [];
  const languageSpecific = [];
  const untranslated = [];

  for (const sentence of sentences) {
    const id = scalar(sentence.id);
    const translation = translations.get(id);

    if (translation?.blocked) {
      bump("language-specific (not translated)");
      languageSpecific.push([id, scalar(translation.blocked), sentence.source.split("\n")[0]]);
      continue;
    }
    if (!translation?.js) {
      bump("THE SURFACE COULD NOT EXPRESS");
      untranslated.push([id, sentence.source.split("\n")[0]]);
      continue;
    }

    const tables = {
      ...(byChapter[""] || {}),
      ...(byChapter[scalar(sentence.chapter)] || {}),
    };
    // An empty R list is `[]` in JSON, not `{}` — the sentence refused before a
    // spec existed, so it has no tables of its own.
    const own = sentence.tables;
    if (own && !Array.isArray(own)) {
      for (const [name, index] of Object.entries(own)) {
        tables[name] = pool[scalar(index) - 1];
      }
    }

    const got = outcomeOf(scalar(translation.js), tables);
    if (got.kind === "missing") {
      bump("table or name missing from the corpus");
      failures.push([id, "ReferenceError", got.text, scalar(translation.js)]);
      continue;
    }
    if (got.kind === "crash") {
      bump("CRASHED");
      failures.push([id, "crash", got.text, scalar(translation.js)]);
      continue;
    }

    const expected = scalar(sentence.outcome);
    if (got.text === expected) {
      bump(got.kind === "refused" ? "identical refusal" : "identical plot");
    } else if (got.kind === "refused" && expected.startsWith("REFUSED")) {
      bump("refused in both, worded per binding");
      bindingRefusals.push([id, expected.slice(8), got.text.slice(8)]);
    } else {
      bump("DISAGREED");
      const how =
        got.kind === "refused"
          ? "R drew, JavaScript refused"
          : expected.startsWith("REFUSED")
            ? "R refused, JavaScript drew"
            : "both drew, different SVG";
      const detail =
        got.kind === "refused"
          ? got.text.slice(8)
          : expected.startsWith("REFUSED")
            ? expected.slice(8)
            : `R ${expected} vs JavaScript ${got.text}`;
      failures.push([id, how, detail, scalar(translation.js)]);
    }
  }

  console.log(`${sentences.length} sentences from the manual\n`);
  for (const [name, count] of [...tally].sort((a, b) => b[1] - a[1])) {
    console.log(`  ${String(count).padStart(4)}  ${name}`);
  }

  if (gaps) {
    console.log("\nconstructs the emitter does not handle:");
    for (const gap of gaps.split("\n")) console.log(`  * ${gap}`);
  }

  if (bindingRefusals.length) {
    console.log(
      `\n${bindingRefusals.length} refusals worded per binding (expected — a message ` +
        "teaches the caller's own syntax):"
    );
    for (const [id, r, js] of bindingRefusals.slice(0, 6)) {
      console.log(`  ${id}\n      R : ${r.split("\n")[0].slice(0, 104)}`);
      console.log(`      js: ${js.split("\n")[0].slice(0, 104)}`);
    }
  }

  if (languageSpecific.length) {
    console.log(`\n${languageSpecific.length} sentences that do not carry over:`);
    for (const [id, why, source] of languageSpecific) {
      console.log(`  ${id.padEnd(24)} ${source.slice(0, 64)}\n      ${why}`);
    }
  }

  if (untranslated.length) {
    console.log(`\n${untranslated.length} the surface could not express:`);
    for (const [id, source] of untranslated) console.log(`  ${id.padEnd(24)} ${source.slice(0, 78)}`);
  }

  if (failures.length) {
    console.log(`\n${failures.length} to look at:`);
    for (const [id, kind, detail, source] of failures.slice(0, 25)) {
      console.log(`  ${id.padEnd(24)} ${kind}\n      ${detail.slice(0, 200)}`);
      console.log(`      ${source.split("\n")[0].slice(0, 110)}`);
    }
  }

  return (tally.get("DISAGREED") || 0) + (tally.get("CRASHED") || 0) +
    (tally.get("THE SURFACE COULD NOT EXPRESS") || 0)
    ? 1
    : 0;
}

process.exit(main());
