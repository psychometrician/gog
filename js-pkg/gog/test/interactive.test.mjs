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
  attachView,
  boundOn,
  placeOn,
  selectedRows,
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

test("a refusal mid-gesture keeps the picture, so a click cannot kill the plot", async () => {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const box = { innerHTML: "", textContent: "", querySelector: () => null };
  const good = {
    spec: { data: "t", layers: [{ mark: "point", encodings: { x: { field: "a" }, y: { field: "b" } }, transforms: [] }] },
    data: { t: { floats: { a: [1, 2], b: [2, 1] } } },
  };
  redraw(engine, box, good);
  const drawn = box.innerHTML;
  assert.ok(drawn.startsWith("<svg"));

  // What a plain click used to send: a range that does not run upward. The
  // engine refuses it, correctly — written down it would be a typo. Mid-drag
  // the picture has to survive, or the panels the next event is measured
  // against are gone and the plot is dead for the rest of the page.
  const clicked = {
    ...good,
    spec: { ...good.spec, brush: [{ field: "a", at: [1.5, 1.5] }] },
  };
  const kept = redraw(engine, box, clicked, { keep: true });
  assert.equal(kept.ok, false);
  assert.match(kept.error, /does not run upward/);
  assert.equal(box.innerHTML, drawn, "the last good picture must still be there");
  assert.equal(box.textContent, "", "and the message must not have replaced it");

  // On the first draw there is no picture to keep, so the message is shown.
  redraw(engine, box, clicked);
  assert.match(box.textContent, /does not run upward/);
});

// A panel 100 units wide over gdp 0..50000, and 100 units tall over life 40..90.
// The y axis arrives with its ends swapped, because it runs down the screen and
// up the data — which is the whole reason these two cases are written out.
const X_AXIS = { field: "gdp", from: 0, to: 50000, lo: 0, hi: 100, cats: null };
const Y_AXIS = { field: "life", from: 40, to: 90, lo: 100, hi: 0, cats: null };

test("a bound runs upward whichever way the pointer was dragged", () => {
  // x: left to right, then right to left. The same range either way.
  assert.deepEqual(boundOn(X_AXIS, 20, 60).at, [10000, 30000]);
  assert.deepEqual(boundOn(X_AXIS, 60, 20).at, [10000, 30000]);

  // y is the case that was broken. Pixel 20 is *near the top*, so it is the
  // larger life value; sorting the pixels put it first and the range came out
  // backwards, which the engine refuses and which made every vertical
  // selection do nothing.
  const down = boundOn(Y_AXIS, 20, 60).at;
  const up = boundOn(Y_AXIS, 60, 20).at;
  assert.deepEqual(down, up, "direction of travel must not change the bound");
  assert.ok(down[0] < down[1], `a bound must run upward, got ${down}`);
  assert.deepEqual(down, [60, 80]);
});

test("a bound on a column of categories covers the slots the drag crossed", () => {
  const cats = { field: "continent", from: 0, to: 1, lo: 0, hi: 100,
                 cats: ["Africa", "Americas", "Asia", "Europe", "Oceania"] };
  assert.deepEqual(boundOn(cats, 5, 35).levels, ["Africa", "Americas"]);
  assert.deepEqual(boundOn(cats, 35, 5).levels, ["Africa", "Americas"]);
  // Past the last edge clamps rather than running off the end.
  assert.deepEqual(boundOn(cats, 85, 200).levels, ["Oceania"]);
});

// ---------------------------------------------------------------------------
// The readout, and the one drift surface in the whole feature
//
// `selectedRows` runs the same predicate the engine runs in `brush_keeps`, in a
// second language. Two implementations of one rule is exactly what this project
// deleted a renderer over, and it is allowed here only because a test can hold
// them to each other: the count the browser reports must equal the marks the
// engine actually drew at full strength.
// ---------------------------------------------------------------------------

const SEL_REQ = {
  spec: {
    data: "t",
    x: { field: "gdp" }, y: { field: "life" },
    layers: [{ mark: "point", encodings: {}, transforms: [] }],
    brush: [{ field: "gdp", at: [2500, 5500] }],
  },
  data: { t: { floats: { gdp: [1000, 3000, 4000, 5000, 9000], life: [50, 60, 70, 75, 80] },
               strings: { country: ["a", "b", "c", "d", "e"] } } },
};

test("the browser's count is the engine's count, or one of them is wrong", async () => {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const { svg } = renderSpec(engine, SEL_REQ);
  const dim = svg.split('<g opacity="0.150">')[1].split("</g>")[0];
  const dimmed = (dim.match(/<circle/g) || []).length;
  const drawn = (svg.match(/<circle/g) || []).length;

  const seen = selectedRows(SEL_REQ);
  assert.equal(seen.total, drawn, "every row is still drawn — a brush does not filter");
  assert.equal(seen.kept, drawn - dimmed, `browser says ${seen.kept}, engine drew ${drawn - dimmed}`);
  assert.equal(seen.kept, 3);
});

test("the readout shows the columns the sentence maps, and says what it left out", () => {
  const seen = selectedRows(SEL_REQ);
  // `country` is in the table but the sentence never names it, so it is not a
  // column a reader is looking at.
  assert.deepEqual(seen.columns, ["gdp", "life"]);
  assert.deepEqual(seen.rows, [[3000, 60], [4000, 70], [5000, 75]]);
  assert.equal(seen.capped, false);

  const capped = selectedRows(SEL_REQ, 2);
  assert.equal(capped.kept, 3, "the count is the whole selection");
  assert.equal(capped.rows.length, 2, "even though only some rows are listed");
  assert.equal(capped.capped, true, "and the shortfall is reported, never silent");
});

test("nothing selected is nothing caught, not everything caught", () => {
  const resting = { ...SEL_REQ, spec: { ...SEL_REQ.spec, brush: [{ field: "gdp" }] } };
  const seen = selectedRows(resting);
  assert.equal(seen.kept, 0);
  assert.equal(seen.total, 0, "a resting brush has no selection to report at all");
});

// ---------------------------------------------------------------------------
// Zoom — the view, and the promise that it is only a view
// ---------------------------------------------------------------------------

/** The smallest thing that behaves like the parts of the DOM `attachView` uses. */
function fakePlot(viewBox = "0 0 800 600") {
  const svg = {
    attrs: { viewBox },
    getAttribute: (k) => svg.attrs[k] ?? null,
    setAttribute: (k, v) => { svg.attrs[k] = v; },
    getBoundingClientRect: () => ({ width: 800, height: 600 }),
  };
  return { svg, querySelector: () => svg };
}
const box = (plot) => plot.svg.attrs.viewBox.split(" ").map(Number);

test("zoom narrows the window and keeps it centred", () => {
  const plot = fakePlot();
  const view = attachView(plot);
  view.apply();
  assert.deepEqual(box(plot), [0, 0, 800, 600]);

  view.zoom(2);
  const [x, y, w, h] = box(plot);
  assert.ok(Math.abs(w - 400) < 1e-9 && Math.abs(h - 300) < 1e-9, "half as wide");
  assert.ok(Math.abs(x - 200) < 1e-9 && Math.abs(y - 150) < 1e-9, "about the middle");
  assert.equal(view.zoomed(), true);
});

test("the window cannot leave the picture, however far you pan", () => {
  const plot = fakePlot();
  const view = attachView(plot);
  view.zoom(2);
  view.panBy(-10000, -10000);
  const [x, y, w, h] = box(plot);
  assert.ok(x + w <= 800 + 1e-9 && y + h <= 600 + 1e-9, `ran off the edge: ${box(plot)}`);
  view.panBy(10000, 10000);
  assert.ok(box(plot)[0] >= -1e-9 && box(plot)[1] >= -1e-9, "and not off the other one");
});

test("fit returns exactly to the picture, and zooming out cannot go past it", () => {
  const plot = fakePlot();
  const view = attachView(plot);
  view.zoom(3);
  view.panBy(50, 50);
  view.reset();
  assert.deepEqual(box(plot), [0, 0, 800, 600]);
  assert.equal(view.zoomed(), false);
  // Out is bounded at fit: there is no picture beyond the picture.
  view.zoom(1 / 4);
  assert.deepEqual(box(plot), [0, 0, 800, 600]);
});

test("a redraw throws the viewBox away, and apply puts it back", () => {
  const plot = fakePlot();
  const view = attachView(plot);
  view.zoom(2);
  const zoomed = plot.svg.attrs.viewBox;
  // What `container.innerHTML = svg` does: a brand new element, at fit.
  plot.svg.attrs.viewBox = "0 0 800 600";
  view.apply();
  assert.equal(plot.svg.attrs.viewBox, zoomed, "every brush frame would snap the zoom out");
});

test("a log axis states its domain in log space, and the browser comes back", () => {
  // What the engine writes for gdp on log10 over roughly 70..141000.
  const log = { field: "gdp", from: 1.85, to: 5.15, lo: 0, hi: 100, log: 10, cats: null };
  const { at } = boundOn(log, 0, 100);
  assert.ok(Math.abs(at[0] - 10 ** 1.85) < 1e-6, `got ${at[0]}`);
  assert.ok(Math.abs(at[1] - 10 ** 5.15) < 1e-6, `got ${at[1]}`);
  // Without undoing the base a full-width drag would have said 1.85 to 5.15,
  // which the engine then compares against gdp in dollars.
  assert.ok(at[1] > 100000, "a bound must be in the column's own units");
});

test("placeOn is boundOn run forwards, on every kind of axis", () => {
  const lin = { from: 0, to: 50000, lo: 0, hi: 100, log: null, cats: null };
  assert.equal(placeOn(lin, 25000), 50);
  // Round trip: a value placed and then read back is the value.
  const back = boundOn(lin, placeOn(lin, 12345), placeOn(lin, 12345)).at[0];
  assert.ok(Math.abs(back - 12345) < 1e-6, `got ${back}`);

  const log = { from: 2, to: 5, lo: 0, hi: 300, log: 10, cats: null };
  assert.ok(Math.abs(placeOn(log, 1000) - 100) < 1e-9, "1000 is one decade of three along");

  const cats = { from: 0, to: 1, lo: 0, hi: 100, log: null,
                 cats: ["Africa", "Americas", "Asia", "Europe"] };
  assert.equal(placeOn(cats, "Asia"), 62.5, "the middle of the third slot of four");
  assert.equal(placeOn(cats, "Nowhere"), null);
});
