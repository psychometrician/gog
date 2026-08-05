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
  attachBrush,
  attachDrag,
  attachView,
  boundOn,
  holdsIn,
  PAGE_ROWS,
  placeOn,
  valueOn,
  selectedRows,
  hasBrush,
  isSpatial,
  loadEngine,
  redraw,
  renderSpec,
} from "../src/interactive.js";
import { pngSize } from "../src/view.js";

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

test("a composed page of cubes is spatial, and its cells keep their own angles", async () => {
  // A page has no coordinate of its own — each cell keeps its space — so asking
  // the top level said "flat" for a page of cubes and the drag was never
  // attached. `hasBrush` had recursed all along, which is what made one file
  // answer the same shape of question two ways.
  const page = {
    arrange: "beside",
    cells: [cube(60, 40).spec, cube(30, 10).spec],
  };
  assert.equal(isSpatial(page), true, "a page of cubes has an angle to drag");
  assert.equal(isSpatial({ arrange: "beside", cells: [] }), false, "an empty page has none");

  // A page holding one cube and one flat plot is still draggable: the cube turns
  // and the flat cell has no angle to turn.
  const flat = cube().spec;
  flat.coord = "flat";
  delete flat.layers[0].encodings.z;
  assert.equal(isSpatial({ arrange: "beside", cells: [flat, cube().spec] }), true);
  assert.equal(isSpatial({ arrange: "beside", cells: [flat, flat] }), false);

  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const container = stubContainer();
    const req = { spec: page, data: cube().data };
    const handle = attachDrag(engine, container, req, { degreesPerPixel: 1 });

    // The drag works on a copy, so the caller's spec is never rotated under it.
    assert.deepEqual(req.spec.cells.map((c) => c.coord.space),
                     [{ turn: 60, tilt: 40 }, { turn: 30, tilt: 10 }],
                     "the sentence the caller wrote is left alone");
    assert.deepEqual(handle.view(), { turn: 60, tilt: 40 },
                     "the readout opens on the first cell's own angle");

    // **The drag carries a change, not an angle**, and this is the assertion that
    // says so: after one gesture the page draws *exactly* as one whose cells
    // were written at 40/70 and 10/40. Comparing the picture rather than
    // reading state back is what proves it reached **every** cell — one absolute
    // angle across the page would have collapsed both onto one pair, and a
    // per-cell readout could not tell the difference.
    // The signs are the second claim, and they are the half nothing watched: the
    // gesture moves the **object**, so dragging right (+20) carries the near face
    // right, which walks the camera the other way and drops `turn`; dragging down
    // (+30) tips that face down and opens the top, which lifts the camera and
    // raises `tilt`. Both are inverted from the angles they set. The old drag
    // moved x alone, so the tilt sign was never pinned at all and the turn sign
    // was pinned backwards.
    container.send("pointerdown", 100, 100);
    container.send("pointermove", 120, 130);
    container.send("pointerup", 120, 130);
    assert.deepEqual(handle.view(), { turn: 40, tilt: 70 },
                     "the cube follows the pointer, both ways at once");

    const turned = renderSpec(engine, {
      spec: { arrange: "beside", cells: [cube(40, 70).spec, cube(10, 40).spec] },
      data: cube().data,
    });
    assert.equal(container.innerHTML, turned.svg,
                 "each cell turned by the same delta, from its own angle");

    handle.reset();
    assert.deepEqual(handle.view(), { turn: 60, tilt: 40 });
    const home = renderSpec(engine, { spec: page, data: cube().data });
    assert.equal(container.innerHTML, home.svg,
                 "reset returns every cell to the angle its own sentence named");
  } finally {
    undo();
  }
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

// The engine draws a fixed canvas and knows nothing about the column it lands
// in, so a redraw has to re-tell the picture to fit — the bindings do it to the
// *static* SVG on the way into the page, and the swap threw that away. Nothing
// could see it: the render exits 0 and only a plot the engine touches is
// affected, so a flat plot shrank and a cube beside it on the same page did not.
test("a redraw tells the new picture to fit its column", async () => {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const svg = { style: {} };
  const box = { innerHTML: "", textContent: "", querySelector: () => svg };
  const out = redraw(engine, box, {
    spec: { data: "t", layers: [{ mark: "point", encodings: { x: { field: "a" }, y: { field: "b" } }, transforms: [] }] },
    data: { t: { floats: { a: [1, 2], b: [2, 1] } } },
  });
  assert.equal(out.ok, true);
  assert.equal(svg.style.maxWidth, "100%");
  assert.equal(svg.style.height, "auto");
});

// The camera's one decision, pinned. Everything else it does is conversion —
// the browser rasterizes the SVG already on screen, so the file cannot disagree
// with the plot — but *how large* is a judgment, and it is a judgment about
// journals: 300 DPI is the usual requirement, so 2400px is 8 inches and clears
// the 7.2-inch double-column figure. At 2x the same plot is 5.3 inches and no
// longer covers one. That is invisible in a screenshot and would be found by a
// reader whose figure was rejected, so it is asserted here instead.
test("a saved plot is large enough for a journal figure", () => {
  const at = (w, h) => pngSize({ getAttribute: (k) => ({ width: w, height: h }[k]) });
  assert.deepEqual(at("800", "600"), { width: 2400, height: 1800 });
  assert.equal(at("800", "600").width / 300, 8); // inches at 300 DPI
  assert.ok(at("800", "600").width / 300 >= 7.2); // a double-column figure fits
  // A plot given its own size scales the same way rather than being left out.
  assert.deepEqual(at("620", "300"), { width: 1860, height: 900 });
  // Nothing to measure is not a crash.
  assert.equal(pngSize(null), null);
  assert.equal(pngSize({ getAttribute: () => null }), null);
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

// A categorical domain runs from half a slot below the first category to half a
// slot above the last, so five categories span -0.5 to 4.5. Writing anything
// else here is writing an axis the engine cannot produce, and these fixtures
// said `0 1` for years: the arithmetic under test ignored both numbers, so the
// filler was never wrong until it was the only thing that could have caught the
// axis being read against the wrong domain.
test("a bound on a column of categories covers the slots the drag crossed", () => {
  const cats = { field: "continent", from: -0.5, to: 4.5, lo: 0, hi: 100,
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

// A selection has no upper size, so `show rows` shows a page of it. The rows a
// page leaves out have to be reachable rather than merely counted — a reader who
// selects forty countries in order to read them is not helped by being told
// there are forty.
const many = (n) => ({
  spec: {
    data: "t", x: { field: "v" }, y: { field: "w" },
    layers: [{ mark: "point", encodings: {}, transforms: [] }],
    brush: [{ field: "v", at: [-1, n + 1] }],
  },
  data: { t: { floats: { v: [...Array(n).keys()], w: [...Array(n).keys()] }, strings: {} } },
});

test("show rows pages through the whole selection, every row once", () => {
  // Not a whole number of pages, so the last one is short — the case where an
  // off-by-one drops a row or invents one.
  const n = 25;
  const req = many(n);
  const seen = [];
  let page = 0;
  for (;;) {
    const s = selectedRows(req, PAGE_ROWS, page * PAGE_ROWS);
    assert.equal(s.kept, n, "the count is the whole selection on every page");
    assert.equal(s.from, page * PAGE_ROWS + 1, "and the page says where it starts");
    assert.equal(s.to, page * PAGE_ROWS + s.rows.length);
    seen.push(...s.rows.map((r) => r[0]));
    if (s.to >= s.kept) break;
    page++;
    assert.ok(page < 10, "paging must terminate");
  }
  assert.equal(page, 2, "twenty-five rows is three pages of ten, the last one short");
  assert.deepEqual(seen, [...Array(n).keys()],
    "every selected row appears exactly once, in order, across the pages");

  // And a selection that *is* a whole number of pages stops on the last full
  // one instead of offering an empty page after it.
  const last = selectedRows(many(PAGE_ROWS * 2), PAGE_ROWS, PAGE_ROWS);
  assert.equal(last.to, last.kept, "twenty rows is two pages of ten, and no third");
});

test("a page past the end of the selection is empty rather than wrong", () => {
  const s = selectedRows(many(30), PAGE_ROWS, 999);
  assert.equal(s.rows.length, 0);
  assert.equal(s.from, 0, "no first row to name");
  assert.equal(s.kept, 30, "and the count is still the whole selection");
});

test("a selection that fits on one page says so, and has no page to turn", () => {
  const s = selectedRows(many(PAGE_ROWS), PAGE_ROWS, 0);
  assert.equal(s.capped, false, "ten rows is not more than ten");
  assert.equal(s.to, s.kept, "so the first page is the last page");
  assert.equal(selectedRows(many(PAGE_ROWS + 1), PAGE_ROWS, 0).capped, true,
    "and eleven is");
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
    style: {},
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

  const cats = { from: -0.5, to: 3.5, lo: 0, hi: 100, log: null,
                 cats: ["Africa", "Americas", "Asia", "Europe"] };
  assert.equal(placeOn(cats, "Asia"), 62.5, "the middle of the third slot of four");
  assert.equal(placeOn(cats, "Nowhere"), null);
});

// ---------------------------------------------------------------------------
// The traced shape — a selection that is not a rectangle
// ---------------------------------------------------------------------------

test("valueOn is placeOn run backwards, and says nothing about a category", () => {
  const lin = { from: 0, to: 50000, lo: 0, hi: 100, log: null, cats: null };
  assert.ok(Math.abs(valueOn(lin, placeOn(lin, 12345)) - 12345) < 1e-6);

  // The trap a bound already fell into once: a log axis states its domain in
  // log space, so a pixel read without undoing that is a logarithm.
  const log = { from: 2, to: 5, lo: 0, hi: 300, log: 10, cats: null };
  assert.ok(Math.abs(valueOn(log, placeOn(log, 1000)) - 1000) < 1e-6);

  // A category has no half, so a free shape has nothing to say about one.
  const cats = { from: -0.5, to: 1.5, lo: 0, hi: 100, log: null, cats: ["a", "b"] };
  assert.equal(valueOn(cats, 25), null);
});

const LASSO_REQ = {
  spec: {
    data: "t",
    x: { field: "gdp" }, y: { field: "life" },
    layers: [{ mark: "point", encodings: {}, transforms: [] }],
    brush: [{ field: "gdp" }],
    // A trapezoid: its top edge slopes, so it holds two of the three rows its
    // own bounding rectangle would hold.
    region: { x: "gdp", y: "life", path: [[2500, 55], [5500, 55], [5500, 72], [2500, 78]] },
  },
  data: SEL_REQ.data,
};

test("a traced shape catches what no rectangle could, and both engines agree", async () => {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const { svg } = renderSpec(engine, LASSO_REQ);
  const dim = svg.split('<g opacity="0.150">')[1].split("</g>")[0];
  const dimmed = (dim.match(/<circle/g) || []).length;
  const drawn = (svg.match(/<circle/g) || []).length;

  const seen = selectedRows(LASSO_REQ);
  assert.equal(seen.total, drawn, "every row is still drawn — tracing does not filter either");
  assert.equal(seen.kept, drawn - dimmed, `browser says ${seen.kept}, engine drew ${drawn - dimmed}`);
  assert.equal(seen.kept, 2);

  // The rectangle around that same shape catches a third row. That difference
  // is the entire reason the gesture exists, so it is asserted rather than
  // assumed.
  const boxed = { ...LASSO_REQ, spec: { ...LASSO_REQ.spec,
    region: { x: "gdp", y: "life", path: [[2500, 55], [5500, 55], [5500, 78], [2500, 78]] } } };
  assert.equal(selectedRows(boxed).kept, 3, "a rectangle cannot exclude the third row");
});

test("an outline that encloses nothing selects nothing", () => {
  const open = { ...LASSO_REQ, spec: { ...LASSO_REQ.spec,
    region: { x: "gdp", y: "life", path: [[2500, 55], [5500, 55]] } } };
  const seen = selectedRows(open);
  assert.equal(seen.kept, 0, "two vertices enclose no area");
  assert.equal(seen.total, 0, "and a plot with nothing selected reports nothing caught");
});

// ---------------------------------------------------------------------------
// The gesture itself, driven through a stub DOM
//
// Every defect this feature has shipped was in the browser layer, and not one
// was caught by a test: the engine suite, four binding suites and three parity
// harnesses were green through all of them, because none of them can hold a
// pointer. So this one does. The stub is deliberately thin — a panel element
// that answers `getAttribute`, an identity screen transform so client
// coordinates *are* user coordinates, and a synchronous animation frame — and
// the engine underneath it is the real one.
//
// The assertion that makes it worth the stub is the last one: the browser plumbs
// a path of screen positions into a region in the columns' own units, hands it
// to the engine, and the engine dims exactly the rows the browser says it
// caught. Nothing short of running the gesture can check that.
// ---------------------------------------------------------------------------

function stubDom() {
  const made = [];
  const el = () => {
    const node = {
      style: {}, children: [], isConnected: false, firstChild: null,
      setAttribute() {}, removeAttribute() {},
      appendChild(c) { node.children.push(c); node.firstChild ??= c; return c; },
      remove() { node.isConnected = false; },
    };
    made.push(node);
    return node;
  };
  const body = el();
  body.appendChild = (c) => { c.isConnected = true; return c; };
  globalThis.document = { body, createElement: el, createElementNS: el };
  globalThis.requestAnimationFrame = (fn) => { fn(); return 0; };
  return () => { delete globalThis.document; delete globalThis.requestAnimationFrame; };
}

/** The one `<g data-gog-panel .../>` the engine writes, as something to query. */
function panelFrom(svg) {
  const tag = svg.match(/<g data-gog-panel[^>]*\/>/);
  if (!tag) return [];
  const attrs = {};
  for (const [, k, v] of tag[0].matchAll(/([\w-]+)="([^"]*)"/g)) attrs[k] = v;
  const identity = { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0, inverse: () => identity };
  const point = () => ({
    x: 0, y: 0,
    matrixTransform(m) { return { x: this.x * m.a + m.e, y: this.y * m.d + m.f }; },
  });
  return [{
    getAttribute: (n) => attrs[n] ?? null,
    ownerSVGElement: { createSVGPoint: point },
    getScreenCTM: () => identity,
  }];
}

function stubContainer() {
  const listeners = new Map();
  let html = "";
  let panels = [];
  return {
    style: {},
    listeners,
    set innerHTML(v) { html = v; panels = panelFrom(v); },
    get innerHTML() { return html; },
    set textContent(v) { html = v; panels = []; },
    // `style` because every real element has one, and `redraw` tells the
    // incoming picture to fit its column through it. A double without it lets
    // production code look wrong when it is the double that is thin.
    querySelector: (sel) => (sel === "svg" ? { style: {} } : null),
    querySelectorAll: (sel) => (sel === "[data-gog-panel]" ? panels : []),
    addEventListener(type, fn) { listeners.set(type, fn); },
    removeEventListener(type) { listeners.delete(type); },
    setPointerCapture() {},
    send(type, x, y) { listeners.get(type)?.({ clientX: x, clientY: y, pointerId: 1 }); },
  };
}

test("a traced drag becomes a region in data units, and the engine agrees", async () => {
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const req = {
      spec: {
        data: "t",
        x: { field: "gdp" }, y: { field: "life" },
        layers: [{ mark: "point", encodings: {}, transforms: [] }],
        brush: [{ field: "" }],
      },
      data: SEL_REQ.data,
    };
    const container = stubContainer();
    const handle = attachBrush(engine, container, req);

    // Where the engine put the axes, read the way the browser reads them.
    const g = container.querySelectorAll("[data-gog-panel]")[0];
    const span = (n) => g.getAttribute(`data-${n}`).split(" ").map(Number);
    const [x0, y0, x1, y1] = g.getAttribute("data-gog-panel").split(" ").map(Number);
    const [xf, xt] = span("x");
    const [yf, yt] = span("y");
    const ax = { from: xf, to: xt, lo: x0, hi: x1, log: null, cats: null };
    const ay = { from: yf, to: yt, lo: y1, hi: y0, log: null, cats: null };

    // A small triangle around the poorest, shortest-lived country alone.
    const px = placeOn(ax, 1000);
    const py = placeOn(ay, 50);
    handle.setMode("lasso");
    container.send("pointerdown", px - 9, py - 9);
    for (const [dx, dy] of [[9, -9], [0, 12], [-9, 0], [-9, -3]]) {
      container.send("pointermove", px + dx, py + dy);
    }
    container.send("pointerup", px, py);

    const caught = handle.selection();
    assert.equal(caught.kept, 1, "the one country inside the traced shape");
    assert.equal(caught.total, 5, "and every row is still drawn");

    // The engine's answer, read off the picture the gesture left behind.
    const dim = container.innerHTML.split('<g opacity="0.150">')[1].split("</g>")[0];
    assert.equal((dim.match(/<circle/g) || []).length, 4,
      "the engine dimmed the four the browser did not catch");

    // A click clears the shape, exactly as it clears a bound. It has to land
    // *inside* the panel to mean anything, which is why this is the middle of it
    // rather than a nudge from the traced corner.
    container.send("pointerdown", (x0 + x1) / 2, (y0 + y1) / 2);
    container.send("pointerup", (x0 + x1) / 2, (y0 + y1) / 2);
    assert.equal(handle.selection().kept, 0, "nothing selected after a click");
    assert.ok(!container.innerHTML.includes('<g opacity="0.150">'),
      "and the picture goes back to one undimmed pass");
    handle.destroy();
  } finally {
    undo();
  }
});

test("a free shape is not offered where an axis carries categories", async () => {
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const req = {
      spec: {
        data: "t",
        x: { field: "place" }, y: { field: "life" },
        layers: [{ mark: "point", encodings: {}, transforms: [] }],
        brush: [{ field: "" }],
      },
      data: { t: { floats: { life: [50, 60, 70, 75, 80] },
                   strings: { place: ["a", "b", "c", "d", "e"] } } },
    };
    const container = stubContainer();
    const handle = attachBrush(engine, container, req);
    const g = container.querySelectorAll("[data-gog-panel]")[0];
    const [x0, y0, x1, y1] = g.getAttribute("data-gog-panel").split(" ").map(Number);

    handle.setMode("lasso");
    container.send("pointerdown", x0 + 5, y0 + 5);
    container.send("pointermove", (x0 + x1) / 2, (y0 + y1) / 2);
    container.send("pointerup", (x0 + x1) / 2, (y0 + y1) / 2);

    // The drag stayed a rectangle and selected whole slots, which is what a
    // category can answer. A shape would have had to cut one in half.
    const caught = handle.selection();
    assert.ok(caught.kept > 0 && caught.kept < caught.total,
      `the drag still selected slots: ${caught.kept} of ${caught.total}`);
    handle.destroy();
  } finally {
    undo();
  }
});

// A categorical axis can be either one, and the gesture has to find its slots on
// both. This is the case the earlier categorical bug never had an example for:
// the manual put the column on `x`, so nothing in the book or the suite ever
// dragged a *vertical* list of slots.
//
// Two claims, and the first is the one that could rot silently. The engine states
// the category list **in the axis's own order**, which on `y` runs bottom to top
// and is therefore the reverse of `x`'s. If that list and the labels it draws
// ever disagreed, every vertical slot selection would be mirrored — the reader
// would drag over one category and select the one opposite it — and no engine
// test would notice, because both halves would still be internally consistent.
test("a drag finds the slots when the categories are on the vertical axis", async () => {
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const req = {
      spec: {
        data: "t",
        x: { field: "life" }, y: { field: "place" },
        layers: [{ mark: "point", encodings: {}, transforms: [] }],
        brush: [{ field: "place" }],
      },
      data: { t: { floats: { life: [50, 60, 70, 75, 80] },
                   strings: { place: ["a", "b", "c", "d", "e"] } } },
    };
    const container = stubContainer();
    const handle = attachBrush(engine, container, req);
    const g = container.querySelectorAll("[data-gog-panel]")[0];
    const [x0, y0, , y1] = g.getAttribute("data-gog-panel").split(" ").map(Number);
    const cats = g.getAttribute("data-y-cats").split("|");

    // ① What the engine *says* against what the engine *draws*. The axis labels
    // are the ones left of the panel; the legend's copies are inside it.
    const drawn = [...container.innerHTML.matchAll(
      /<text[^>]*\bx="([\d.]+)"[^>]*\by="([\d.]+)"[^>]*>([a-e])<\/text>/g)]
      .map((m) => ({ x: +m[1], y: +m[2], name: m[3] }))
      .filter((t) => t.x < x0)
      .sort((p, q) => q.y - p.y)          // bottom of the panel first
      .map((t) => t.name);
    assert.deepEqual(cats, drawn,
      `the list the browser reads (${cats}) must be the order the reader sees (${drawn})`);

    // ② And the gesture, dragged across the bottom slot of five.
    const slot = (y1 - y0) / cats.length;
    container.send("pointerdown", (x0 + 40), y1 - 4);
    container.send("pointermove", (x0 + 40), y1 - slot + 6);
    container.send("pointerup", (x0 + 40), y1 - slot + 6);

    const caught = handle.selection();
    assert.equal(caught.kept, 1, `one slot of five: ${caught.kept}`);
    assert.deepEqual(caught.rows.map((r) => r[caught.columns.indexOf("place")]), [cats[0]],
      "and it is the slot drawn at the bottom, not the one opposite it");
    handle.destroy();
  } finally {
    undo();
  }
});

// A category sits where its axis says, not where counting the categories
// guesses. The two are the same number until an axis is wider than its own
// slots, and `density(reach = )` past half a slot makes one: the domain widens
// to leave room for shapes that lean out of their slots, and the engine states
// the wider one. Both halves of the browser's axis arithmetic read the count
// instead, so every category was placed short and every drag came back with the
// wrong slot — the reader dragged over one category and selected another.
//
// Both are checked here because both were wrong, and the second is the one a
// reader meets. Neither had an example anywhere: every categorical plot in the
// book leaves its axis exactly as wide as its slots, where the two readings
// agree exactly.
test("a widened categorical axis is read where the engine drew it", async () => {
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const req = {
      spec: {
        data: "t",
        x: { field: "place" }, y: { field: "v" },
        layers: [
          { mark: "area", encodings: {}, transforms: ["density"], density: { reach: 3.0 } },
          { mark: "point", encodings: {}, transforms: [] },
        ],
        brush: [{ field: "place" }],
      },
      data: { t: { floats: { v: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12] },
                   strings: { place: ["a", "a", "a", "a", "b", "b", "b", "b",
                                      "c", "c", "c", "c"] } } },
    };
    const container = stubContainer();
    const handle = attachBrush(engine, container, req);
    const g = container.querySelectorAll("[data-gog-panel]")[0];
    const [x0, , x1] = g.getAttribute("data-gog-panel").split(" ").map(Number);
    const [from, to] = g.getAttribute("data-x").split(" ").map(Number);
    const cats = g.getAttribute("data-x-cats").split("|");

    // The axis states more room than it has slots. That is the whole case, and
    // without it this test passes against either reading.
    assert.ok(to - from > cats.length,
      `the reach widened the axis: ${from} to ${to}, for ${cats.length} slots`);

    // ① Where the browser puts each category against where the engine drew it.
    const ax = { from, to, lo: x0, hi: x1, log: null, cats };
    const drawn = [...new Set([...container.innerHTML.matchAll(/<circle cx="([\d.]+)"/g)]
      .map((m) => +m[1]))].sort((p, q) => p - q);
    assert.equal(drawn.length, cats.length, "one column of points per category");
    cats.forEach((name, i) => {
      assert.ok(Math.abs(placeOn(ax, name) - drawn[i]) < 1,
        `${name} is placed at ${placeOn(ax, name)} and drawn at ${drawn[i]}`);
    });

    // ② And the drag, over the middle slot. Counting the categories put this
    // pointer most of the way along an axis that reaches half again as far, so
    // it used to come back with the first category rather than the second.
    const at = (v) => x0 + ((v - from) / (to - from)) * (x1 - x0);
    container.send("pointerdown", at(0.7), 300);
    container.send("pointermove", at(1.3), 300);
    container.send("pointerup", at(1.3), 300);

    const caught = handle.selection();
    assert.deepEqual(
      [...new Set(caught.rows.map((r) => r[caught.columns.indexOf("place")]))],
      [cats[1]],
      "the drag caught the slot it was drawn over");
    handle.destroy();
  } finally {
    undo();
  }
});

test("holdsIn counts a vertex on the ray once, not twice", () => {
  const diamond = [[0, 1], [1, 2], [2, 1], [1, 0]];
  assert.equal(holdsIn(diamond, 1, 1), true, "the middle is inside");
  assert.equal(holdsIn(diamond, -1, 1), false, "and a point level with two vertices is not");
  assert.equal(holdsIn(diamond, 3, 1), false);
  assert.equal(holdsIn([[0, 0], [1, 1]], 0.5, 0.5), false, "two vertices enclose nothing");
});

// The interactive block must reach the browser intact. Not reachable by
// comparing SVG: that path is the CLI's and is perfect, while the browser gets a
// separate payload nothing checked. A `data:` module import is refused by a
// content-security policy — silently, because a blocked module import throws
// nothing a page can catch, so the plot draws and every control is missing.
test("the interactive block names no URL a policy can refuse", async () => {
  const R = await import("../src/render.js");
  const { plot, data, point, x, y, col, brush } = await import("../src/index.js");
  const t = { gdp: [1000, 20000, 40000], life: [50, 70, 80] };
  const p = plot(data(t, "t"), point, x(col.gdp), y(col.life),
                 brush(col.gdp, { at: [2000, 30000] }));
  const block = R.htmlBlock(p);

  // No script means the browser engine was never built, which is the normal
  // state in CI. There is nothing to assert about a block that does not exist.
  if (!block.includes("<script")) {
    console.log("SKIP: browser engine not built, so the block cannot be checked");
    return;
  }
  assert.ok(!block.includes("data:text/javascript"));
  assert.ok(!block.includes("data:application/wasm"));
  assert.ok(!block.includes('from "./view.js"'));
  assert.ok(block.includes("function mountView"));  // the module is here, inline
  assert.ok(block.includes("atob("));               // the engine travels as bytes
});

