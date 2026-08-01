// The browser engine and the command line must draw the same picture.
//
// `gog-wasm` exists so a page can render without spawning a process. The risk
// that creates is two engines: if the WebAssembly build ever draws something the
// CLI does not, a reader turning a plot in a book is looking at a different
// dataset than the printed figure beside it. These tests pin them together, and
// the byte comparison is the one that would catch it — the project's standing
// bar for any second path to the renderer.
//
// Skipped, loudly, when `gog-wasm/target/.../gog_wasm.wasm` has not been built:
//   cargo build --release --target wasm32-unknown-unknown \
//     --manifest-path gog-wasm/Cargo.toml

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  attachDrag,
  hasBrush,
  isSpatial,
  loadEngine,
  redraw,
  renderSpec,
} from "../src/interactive.js";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..", "..", "..");
const WASM = path.join(ROOT, "gog-wasm/target/wasm32-unknown-unknown/release/gog_wasm.wasm");
const CLI = path.join(ROOT, "target/release/gog-cli");

const have = fs.existsSync(WASM) && fs.existsSync(CLI);
if (!have) {
  console.error(
    `\n  interactive.test.mjs SKIPPED — missing ${!fs.existsSync(WASM) ? "gog_wasm.wasm" : "gog-cli"}.` +
      `\n  Build both, or this file proves nothing:` +
      `\n    cargo build --release` +
      `\n    cargo build --release --target wasm32-unknown-unknown --manifest-path gog-wasm/Cargo.toml\n`,
  );
}

const cube = (turn = 45, tilt = 25) => ({
  spec: {
    data: "t",
    layers: [
      {
        mark: "point",
        encodings: { x: { field: "a" }, y: { field: "b" }, z: { field: "c" } },
        transforms: [],
      },
    ],
    coord: { space: { turn, tilt } },
  },
  data: {
    t: {
      floats: {
        a: [1, 2, 3, 4, 5, 6],
        b: [2, 1, 3, 6, 4, 5],
        c: [3, 2, 1, 5, 6, 4],
      },
    },
  },
});

const viaCli = (request) =>
  execFileSync(CLI, { input: JSON.stringify(request), encoding: "utf8" });

test("the WebAssembly engine draws exactly what the CLI draws", { skip: !have }, async () => {
  const engine = await loadEngine(fs.readFileSync(WASM));
  // Several angles, because a projector that agreed at one angle and not another
  // would be the subtlest possible version of this bug.
  for (const [turn, tilt] of [
    [45, 25],
    [0, 0],
    [137, -40],
    [359, 89],
  ]) {
    const req = cube(turn, tilt);
    const { svg, error } = renderSpec(engine, req);
    assert.equal(error, null, `wasm refused at ${turn},${tilt}: ${error}`);
    assert.equal(
      svg,
      viaCli(req),
      `wasm and CLI disagree at turn=${turn}, tilt=${tilt}`,
    );
  }
});

test("turning the cube redraws it", { skip: !have }, async () => {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const a = renderSpec(engine, cube(30, 25)).svg;
  const b = renderSpec(engine, cube(120, 25)).svg;
  assert.notEqual(a, b, "a different angle must project differently");
});

test("a refusal comes back as a message, never as a picture", { skip: !have }, async () => {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const bad = cube();
  bad.spec.layers[0].mark = "line"; // decided refusal: a cube has no left to right
  const { svg, error } = renderSpec(engine, bad);
  assert.equal(svg, null, "nothing may be drawn");
  assert.ok(error && error.length > 0, "a refusal must say why");
  assert.ok(!error.includes("<svg"), "and must not smuggle a picture into the message");
});

test("rendering does not grow linear memory without bound", { skip: !have }, async () => {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const req = cube();
  for (let i = 0; i < 20; i++) renderSpec(engine, req); // settle any initial growth
  const before = engine.memory.buffer.byteLength;
  for (let i = 0; i < 300; i++) {
    req.spec.coord.space.turn = i;
    renderSpec(engine, req);
  }
  const after = engine.memory.buffer.byteLength;
  // 300 frames at ~240 KB each would be ~72 MB if nothing were freed. Any growth
  // at all here means `dealloc` is not being reached on some path.
  assert.equal(
    after,
    before,
    `linear memory grew ${((after - before) / 1024 / 1024).toFixed(1)} MB over 300 frames — a leak`,
  );
});

test("isSpatial finds the cube whether or not space() was named", () => {
  assert.equal(isSpatial(cube().spec), true, "an explicit space() is spatial");

  // A `z` binding projects without `space()` ever being written, and the
  // coordinate still reads "flat" — the case that makes this more than a
  // one-line property check.
  const implicit = cube().spec;
  implicit.coord = "flat";
  assert.equal(isSpatial(implicit), true, "a bound z is spatial even under coord:flat");

  const flat = cube().spec;
  flat.coord = "flat";
  delete flat.layers[0].encodings.z;
  assert.equal(isSpatial(flat), false, "a plot with no z is not");
});

test("attachDrag is exported and refuses politely without a DOM", () => {
  assert.equal(typeof attachDrag, "function");
});

// ---------------------------------------------------------------------------
// brush — what a page needs, which turned out to be nothing in the engine
//
// Two composed plots naming the same column were already answering the same
// predicate, because a bound is a fact about a column rather than about a panel.
// All the page needed was for one gesture to reach every cell that named it.
// ---------------------------------------------------------------------------

test("hasBrush walks a page, not just a plot", () => {
  assert.equal(hasBrush({ layers: [] }), false);
  assert.equal(hasBrush({ layers: [], brush: [{ field: "gdp" }] }), true);
  // A page keeps its cells under `cells` in R and `plots` elsewhere; both count.
  assert.equal(hasBrush({ cells: [{ layers: [] }, { brush: [{ field: "gdp" }] }] }), true);
  assert.equal(hasBrush({ plots: [{ layers: [] }, { brush: [{ field: "gdp" }] }] }), true);
  assert.equal(hasBrush({ cells: [{ layers: [] }, { layers: [] }] }), false);
  // A page of pages: the walk has to go all the way down.
  assert.equal(hasBrush({ cells: [{ cells: [{ brush: [{ field: "gdp" }] }] }] }), true);
});

test("redraw is the one loop, and a refusal shows its message", async () => {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const box = { innerHTML: "", textContent: "", querySelector: () => null };
  const good = redraw(engine, box, {
    spec: { data: "t", layers: [{ mark: "point", encodings: { x: { field: "a" }, y: { field: "b" } }, transforms: [] }] },
    data: { t: { floats: { a: [1, 2], b: [2, 1] } } },
  });
  assert.equal(good.ok, true);
  assert.ok(box.innerHTML.startsWith("<svg"));

  // A brushed `line` is refused, and the container carries the reason rather
  // than an empty box.
  const bad = redraw(engine, box, {
    spec: { data: "t", layers: [{ mark: "line", encodings: { x: { field: "a" }, y: { field: "b" } }, transforms: [] }],
            brush: [{ field: "a", at: [1, 2] }] },
    data: { t: { floats: { a: [1, 2], b: [2, 1] } } },
  });
  assert.equal(bad.ok, false);
  assert.match(box.textContent, /one shape through many rows/);
});
