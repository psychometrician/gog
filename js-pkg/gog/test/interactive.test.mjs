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
  const el = () => {
    const node = {
      style: {}, children: [], isConnected: false, firstChild: null,
      // The tooltip measures itself to stay inside the window. Zero is a
      // truthful width for an element nothing laid out, and it keeps the
      // arithmetic that reads it from producing `NaN`.
      offsetWidth: 0, offsetHeight: 0,
      listeners: new Map(),
      // Kept rather than dropped, for the reason the body's children are kept:
      // the leader tying a stamp to its point is an `<svg>` whose whole state is
      // in its attributes, so "where does the line run?" has to be answerable.
      attrs: {},
      setAttribute(k, v) { node.attrs[k] = String(v); },
      getAttribute(k) { return node.attrs[k] ?? null; },
      removeAttribute(k) { delete node.attrs[k]; },
      addEventListener(type, fn) { node.listeners.set(type, fn); },
      removeEventListener(type) { node.listeners.delete(type); },
      appendChild(c) { node.children.push(c); node.firstChild ??= c; return c; },
      remove() {
        node.isConnected = false;
        const kin = node.parent?.children;
        if (kin) kin.splice(kin.indexOf(node), 1);
      },
    };
    return node;
  };
  const body = el();
  // Recorded rather than discarded. This used to drop the child on the floor,
  // which was fine while nothing on `document.body` outlived a gesture and is
  // not fine now: the readout is parented here, so "what is on the page?" has to
  // be a question the stub can answer.
  body.appendChild = (c) => { c.isConnected = true; c.parent = body; body.children.push(c); return c; };
  globalThis.document = { body, createElement: el, createElementNS: el };
  // Synchronous, so a scheduled redraw has happened by the time a test looks —
  // but **never re-entrant**, which is the part that matters now that something
  // on the page asks for a frame from inside one. A real browser runs the next
  // callback on the next frame; a double that runs it on the stack turns the
  // clock watcher into infinite recursion. A frame asked for from inside a
  // frame is queued instead, and `frames()` below is how a test steps it.
  let inFrame = false;
  const queued = [];
  globalThis.requestAnimationFrame = (fn) => {
    if (inFrame) return queued.push(fn);
    inFrame = true;
    try { fn(); } finally { inFrame = false; }
    return queued.length;
  };
  globalThis.cancelAnimationFrame = () => { queued.length = 0; };
  /** Run up to `n` queued frames, and say how many there were to run. The count
   *  is what lets a test assert that something *stopped* asking for them. */
  globalThis.__frames = (n = 1) => {
    let ran = 0;
    for (let i = 0; i < n && queued.length; i++) {
      const fn = queued.shift();
      inFrame = true;
      try { fn(); } finally { inFrame = false; }
      ran++;
    }
    return ran;
  };
  globalThis.window = {
    innerWidth: 1200, innerHeight: 800,
    addEventListener() {}, removeEventListener() {},
  };
  return () => {
    delete globalThis.document;
    delete globalThis.requestAnimationFrame;
    delete globalThis.cancelAnimationFrame;
    delete globalThis.__frames;
    delete globalThis.window;
  };
}

/** Whatever is parented to `document.body` and still attached. */
const onPage = (cls) =>
  globalThis.document.body.children.filter((n) => n.isConnected && n.className === cls);

/** Every `<g data-gog-panel .../>` the engine wrote, as something to query.
 *
 *  All of them, not the first: a faceted plot writes one per panel, and reading
 *  only the first is how a test of faceted behavior would quietly become a test
 *  of one panel's. */
function panelFrom(svg) {
  // The panel's place on the screen. Identity by default, so client coordinates
  // *are* user coordinates and a pointer test reads as arithmetic. A test that
  // cares whether something re-reads the transform rather than merely staying
  // put moves `SHIFT` and asks again.
  const ctm = () => {
    const m = { a: 1, b: 0, c: 0, d: 1, e: SHIFT.x, f: SHIFT.y };
    m.inverse = () => ({ a: 1, b: 0, c: 0, d: 1, e: -SHIFT.x, f: -SHIFT.y, inverse: () => m });
    return m;
  };
  const point = () => ({
    x: 0, y: 0,
    matrixTransform(m) { return { x: this.x * m.a + m.e, y: this.y * m.d + m.f }; },
  });
  // One clock for the whole picture, which is what the document has. A test sets
  // it to choose a moment, the way a reader's browser advances it.
  const owner = { createSVGPoint: point, getCurrentTime: () => CLOCK.t };
  return [...svg.matchAll(/<g data-gog-panel[^>]*\/>/g)].map((tag) => {
    const attrs = {};
    for (const [, k, v] of tag[0].matchAll(/([\w-]+)="([^"]*)"/g)) attrs[k] = v;
    return {
      getAttribute: (n) => attrs[n] ?? null,
      ownerSVGElement: owner,
      getScreenCTM: ctm,
    };
  });
}

/** Where the animation has got to, in seconds. */
const CLOCK = { t: 0 };
/** Where the panel sits on the screen, for testing that something re-reads it. */
const SHIFT = { x: 0, y: 0 };

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
    querySelector: (sel) =>
      (sel === "svg" ? { style: {}, getCurrentTime: () => CLOCK.t } : null),
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

// ---------------------------------------------------------------------------
// Pointing at a row
//
// The readout never asks the picture what lies under the pointer. It re-derives
// every row's position from the row's value and the two numbers the panel
// states, and keeps the nearest. Nothing exercised that until now, which is why
// it could be wrong on a faceted plot and on an animated one for as long as it
// existed: it walked the whole table against whichever panel the pointer was
// over, and every moment of a played plot including the hidden ones.
//
// Wrong here does not look wrong. The reader is handed a plausible row at a
// plausible position, and with shared scales the two panels' coordinates line up
// exactly, so the answer from the wrong panel lands where an answer belongs.
// ---------------------------------------------------------------------------

/** A plot, mounted, with its first panel and an axis pair to place values on. */
async function hoverFixture(spec, data) {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const container = stubContainer();
  const handle = attachBrush(engine, container, { spec, data });
  const on = (g) => {
    const [x0, y0, x1, y1] = g.getAttribute("data-gog-panel").split(" ").map(Number);
    const num = (n) => g.getAttribute(`data-${n}`).split(" ").map(Number);
    const [xf, xt] = num("x");
    const [yf, yt] = num("y");
    return {
      x0, y0, x1, y1,
      x: { from: xf, to: xt, lo: x0, hi: x1, log: null, cats: null },
      y: { from: yf, to: yt, lo: y1, hi: y0, log: null, cats: null },
      place: g.getAttribute("data-gog-place"),
    };
  };
  return { handle, container, panels: container.querySelectorAll("[data-gog-panel]").map(on) };
}

const POINTS = {
  spec: {
    data: "t",
    x: { field: "g" }, y: { field: "v" },
    layers: [{ mark: "point", encodings: {}, transforms: [] }],
    brush: [{ field: "g" }],
  },
  data: { t: { floats: { g: [10, 50, 90], v: [10, 50, 90] } } },
};

test("pointing at a mark names the row under it", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(POINTS.spec, POINTS.data);
    const p = panels[0];
    container.send("pointermove", placeOn(p.x, 50), placeOn(p.y, 50));

    const [tip] = onPage("gog-tip");
    assert.ok(tip, "a readout appeared");
    assert.match(tip.innerHTML, /\b50\b/, "and it names the row that is there");
    assert.ok(!/\b90\b/.test(tip.innerHTML), "and not one of its neighbors");
    handle.destroy();
  } finally {
    undo();
  }
});

test("pointing at nothing says nothing", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(POINTS.spec, POINTS.data);
    const p = panels[0];
    // Between two marks, well past the glyph's reach from either.
    container.send("pointermove", placeOn(p.x, 30), placeOn(p.y, 70));
    assert.equal(onPage("gog-tip").length, 0);
    handle.destroy();
  } finally {
    undo();
  }
});

// The one that could not be caught any other way. Both panels share their scales,
// so the position where the second panel's row was drawn is a real position
// inside the first panel, and the first panel drew nothing there.
test("a faceted panel answers with its own rows and no others", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(
      { data: "t",
        x: { field: "g" }, y: { field: "v" },
        layers: [{ mark: "point", encodings: {}, transforms: [] }],
        brush: [{ field: "g" }],
        facet: { col: "c" } },
      { t: { floats: { g: [10, 90], v: [10, 90] },
             strings: { c: ["left", "right"] } } });
    assert.equal(panels.length, 2, "one panel per level");
    const left = panels[0];

    // Where the *right* panel's row sits, pointed at inside the *left* one.
    container.send("pointermove", placeOn(left.x, 90), placeOn(left.y, 90));
    assert.equal(onPage("gog-tip").length, 0,
      "the row drawn in the other panel is not under this pointer");

    // And the panel still answers for what it did draw, so the silence above is
    // the filter working rather than the readout being broken.
    container.send("pointermove", placeOn(left.x, 10), placeOn(left.y, 10));
    assert.equal(onPage("gog-tip").length, 1, "its own row is still named");
    handle.destroy();
  } finally {
    undo();
  }
});

// Every moment is in the document at once and the clock chooses which one is
// displayed, so the table on the page is always larger than the picture in front
// of the reader. This shape ships in the manual.
test("a played plot answers only for the moment showing", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(
      { data: "t",
        x: { field: "g" }, y: { field: "v" },
        layers: [{ mark: "point", encodings: { play: { field: "yr" } }, transforms: [] }],
        brush: [{ field: "g" }] },
      { t: { floats: { g: [10, 90], v: [10, 90], yr: [1952, 1957] } } });
    const p = panels[0];
    const early = [placeOn(p.x, 10), placeOn(p.y, 10)];
    const late = [placeOn(p.x, 90), placeOn(p.y, 90)];

    CLOCK.t = 0;
    container.send("pointermove", ...early);
    assert.equal(onPage("gog-tip").length, 1, "the first moment's row, while it shows");
    container.send("pointermove", ...late);
    assert.equal(onPage("gog-tip").length, 0, "and not the row from a moment yet to come");

    // Halfway through the second frame. One frame is 0.8s by default.
    CLOCK.t = 1.2;
    container.send("pointermove", ...late);
    assert.equal(onPage("gog-tip").length, 1, "the second moment's row, once it shows");
    container.send("pointermove", ...early);
    assert.equal(onPage("gog-tip").length, 0, "and not the one it replaced");
    handle.destroy();
    CLOCK.t = 0;
  } finally {
    CLOCK.t = 0;
    undo();
  }
});

// A disc bends both axes and the readout reads a value back along straight ones,
// so there is no answer to give. Going quiet is the honest half; the bar says why
// once the reader has asked, which is the other half.
test("a plot that cannot place a row says nothing, and says why", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(
      { ...POINTS.spec, coord: { polar: {} } }, POINTS.data);
    const p = panels[0];
    assert.equal(p.place, "polar", "the engine stated what it did to the position");

    container.send("pointermove", placeOn(p.x, 50), placeOn(p.y, 50));
    assert.equal(onPage("gog-tip").length, 0, "no readout on a panel that cannot place");
    assert.equal(handle.unplaced(), "polar", "and the bar has a reason to give");
    handle.destroy();
  } finally {
    undo();
  }
});

test("nobody has asked, so there is nothing to explain", async () => {
  const undo = stubDom();
  try {
    const { handle } = await hoverFixture(
      { ...POINTS.spec, coord: { polar: {} } }, POINTS.data);
    assert.equal(handle.unplaced(), null,
      "the reason waits for a reader rather than sitting under every such plot");
    handle.destroy();
  } finally {
    undo();
  }
});

// ---------------------------------------------------------------------------
// Stamping a row
//
// A stamp is the readout, kept. The gesture that asks for one is a click, which
// already meant *clear the selection*, so the two are told apart by what the
// click landed on: a mark stamps, empty space clears. Both readings are here,
// because the risk is not that stamping fails but that it eats the clear.
// ---------------------------------------------------------------------------

const cardOf = (stamp) => stamp.children.find((c) => c.className === "gog-stamp-card");
/** The rows a card names. They sit beside its close control rather than being
 *  the whole of the card, which is why this is not `card.innerHTML`. */
const rowsOf = (card) => card.children[0]?.innerHTML ?? "";
const shutOf = (card) => card.children.find((c) => c.className === "gog-stamp-close");
const byClass = (node, cls) =>
  node.children.find((c) => c.getAttribute?.("class") === cls);
const leaderOf = (stamp) => {
  const wire = byClass(stamp, "gog-stamp-leader");
  return { line: byClass(wire, "gog-stamp-line"), head: byClass(wire, "gog-stamp-head") };
};

/** A pointer event aimed at one element. The container has `send`; a card takes
 *  its own events, because it is on `document.body` and not in the plot. */
const fire = (node, type, x, y, target) =>
  node.listeners.get(type)?.({
    clientX: x, clientY: y, pointerId: 1, target: target ?? node,
    preventDefault() {},
  });

/** Carry a card by `(dx, dy)` and put it down. */
const carry = (card, dx, dy) => {
  fire(card, "pointerdown", 0, 0);
  fire(card, "pointermove", dx, dy);
  fire(card, "pointerup", dx, dy);
};

/** Where a card sits relative to its point, as the two numbers it is placed by. */
const offsetOf = (card) => [parseFloat(card.style.left), parseFloat(card.style.top)];

/** Stamp the row at (50, 50) and hand back everything a test needs to poke it. */
async function stampFixture() {
  const fixture = await hoverFixture(POINTS.spec, POINTS.data);
  const p = fixture.panels[0];
  fixture.container.send("pointerdown", placeOn(p.x, 50), placeOn(p.y, 50));
  fixture.container.send("pointerup", placeOn(p.x, 50), placeOn(p.y, 50));
  const [stamp] = onPage("gog-stamp");
  return { ...fixture, p, stamp, card: cardOf(stamp) };
}

test("clicking a mark leaves it named on the picture", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(POINTS.spec, POINTS.data);
    const p = panels[0];
    const [px, py] = [placeOn(p.x, 50), placeOn(p.y, 50)];
    container.send("pointerdown", px, py);
    container.send("pointerup", px, py);

    assert.equal(handle.stamps(), 1);
    const [pinned] = onPage("gog-stamp");
    assert.ok(pinned, "a stamp is on the page");
    assert.match(rowsOf(cardOf(pinned)), /\b50\b/, "and it names the row clicked");
    handle.destroy();
  } finally {
    undo();
  }
});

test("clicking empty space still clears, and stamps nothing", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(POINTS.spec, POINTS.data);
    const p = panels[0];
    // Select two of the three, so there is something for a click to clear.
    container.send("pointerdown", placeOn(p.x, 10), placeOn(p.y, 10));
    container.send("pointermove", placeOn(p.x, 60), placeOn(p.y, 60));
    container.send("pointerup", placeOn(p.x, 60), placeOn(p.y, 60));
    assert.equal(handle.selection().kept, 2, "two are caught");
    assert.equal(handle.stamps(), 0, "and a drag is not a click");

    // Now a click on a part of the panel with no mark near it.
    const [ex, ey] = [placeOn(p.x, 30), placeOn(p.y, 70)];
    container.send("pointerdown", ex, ey);
    container.send("pointerup", ex, ey);
    assert.equal(handle.selection().kept, 0, "the click cleared the selection");
    assert.equal(handle.stamps(), 0, "and left nothing behind");
    handle.destroy();
  } finally {
    undo();
  }
});

// The assertion the anchoring rests on. A redraw replaces the whole picture, so
// the panel the stamp was measured against is gone; moving the screen transform
// underneath proves the stamp went back and read the new one, rather than
// passing because nothing moved.
test("a redraw keeps the stamp and takes it with the picture", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(POINTS.spec, POINTS.data);
    const p = panels[0];
    container.send("pointerdown", placeOn(p.x, 50), placeOn(p.y, 50));
    container.send("pointerup", placeOn(p.x, 50), placeOn(p.y, 50));
    const [before] = onPage("gog-stamp");
    const wasAt = parseFloat(before.style.left);

    // The panel has moved on the screen, so the pointer's own numbers move with
    // it: a reader dragging the same part of the picture is now dragging 100
    // pixels further right. Any drag will do here; the point is that it redraws.
    SHIFT.x = 100;
    const cx = (v) => placeOn(p.x, v) + SHIFT.x;
    const cy = (v) => placeOn(p.y, v) + SHIFT.y;
    container.send("pointerdown", cx(10), cy(10));
    container.send("pointermove", cx(60), cy(60));
    container.send("pointerup", cx(60), cy(60));

    const after = onPage("gog-stamp");
    assert.equal(after.length, 1, "still one stamp, and the drag added none");
    assert.equal(after[0], before, "the same card, not a fresh one");
    assert.equal(parseFloat(after[0].style.left) - wasAt, 100,
      "and it moved exactly as far as the panel did");
    handle.destroy();
  } finally {
    SHIFT.x = 0;
    undo();
  }
});

test("clicking a stamp takes it off", async () => {
  const undo = stubDom();
  try {
    const { handle, card } = await stampFixture();
    // Pressed and released without going anywhere, which is the whole of the
    // gesture now that the card's own face is also a handle.
    fire(card, "pointerdown", 40, 40);
    fire(card, "pointerup", 40, 40);
    assert.equal(handle.stamps(), 0);
    assert.equal(onPage("gog-stamp").length, 0, "and it left the page with it");
    handle.destroy();
  } finally {
    undo();
  }
});

// ---------------------------------------------------------------------------
// Carrying a card, and the three ways to undo one stamp
//
// A card is over the data it was made to read, which is the complaint that
// earned this: four stamps on a crowded scatter hide the crowd. So the card
// moves, and the risk that creates is the one gesture eating the other. A reader
// who nudges a card by two pixels must still find it there, and a reader who
// carries one across the panel must not have it vanish when they let go.
// ---------------------------------------------------------------------------

test("a card is carried where it is put, and is not taken off by the carrying", async () => {
  const undo = stubDom();
  try {
    const { handle, card } = await stampFixture();
    const [x0, y0] = offsetOf(card);
    carry(card, 90, 60);

    assert.equal(handle.stamps(), 1, "still stamped: a carry is not a click");
    assert.deepEqual(offsetOf(card), [x0 + 90, y0 + 60],
      "and it went exactly as far as the pointer did");
    handle.destroy();
  } finally {
    undo();
  }
});

test("a nudge under the threshold is a click, and takes the stamp off", async () => {
  const undo = stubDom();
  try {
    const { handle, card } = await stampFixture();
    const before = offsetOf(card);
    // Two pixels: a hand that did not mean to move. The panel calls this a click
    // and so does the card, because one threshold decides both.
    fire(card, "pointerdown", 0, 0);
    fire(card, "pointermove", 1, 1);
    assert.deepEqual(offsetOf(card), before, "it has not moved yet");
    fire(card, "pointerup", 1, 1);
    assert.equal(handle.stamps(), 0, "and the release reads as a click");
    handle.destroy();
  } finally {
    undo();
  }
});

test("a pointer that wanders out and comes back is still a carry", async () => {
  const undo = stubDom();
  try {
    const { handle, card } = await stampFixture();
    fire(card, "pointerdown", 0, 0);
    fire(card, "pointermove", 80, 80);
    // Back to where it started. Measured end to end this is a click, which is
    // why `moved` latches on the way instead.
    fire(card, "pointermove", 0, 0);
    fire(card, "pointerup", 0, 0);
    assert.equal(handle.stamps(), 1, "the stamp survived a round trip");
    handle.destroy();
  } finally {
    undo();
  }
});

test("the cross takes one stamp off, and leaves the others", async () => {
  const undo = stubDom();
  try {
    const { handle, container, p, card } = await stampFixture();
    container.send("pointerdown", placeOn(p.x, 10), placeOn(p.y, 10));
    container.send("pointerup", placeOn(p.x, 10), placeOn(p.y, 10));
    assert.equal(handle.stamps(), 2, "two rows named");

    const shut = shutOf(card);
    const before = offsetOf(card);
    // A press on the cross is not a press on the card, or a reader aiming at the
    // one thing that unambiguously removes a stamp would start dragging it.
    fire(card, "pointerdown", 0, 0, shut);
    fire(card, "pointermove", 50, 50);
    assert.deepEqual(offsetOf(card), before, "the cross is a control, not a handle");

    shut.listeners.get("click")();
    assert.equal(handle.stamps(), 1, "the one whose cross was clicked, and no other");
    handle.destroy();
  } finally {
    undo();
  }
});

test("unstamp still takes every card off, wherever they were carried", async () => {
  const undo = stubDom();
  try {
    const { handle, container, p, card } = await stampFixture();
    carry(card, 120, -80);
    container.send("pointerdown", placeOn(p.x, 10), placeOn(p.y, 10));
    container.send("pointerup", placeOn(p.x, 10), placeOn(p.y, 10));
    assert.equal(handle.stamps(), 2);

    handle.clearStamps();
    assert.equal(handle.stamps(), 0);
    assert.equal(onPage("gog-stamp").length, 0, "and the page is clear of them");
    handle.destroy();
  } finally {
    undo();
  }
});

// A row on a played plot is one country in one year, so a stamp made in 1972
// names something that is simply not on the screen in 1987. Left showing, its
// dot sits several hundred pixels from the row it names and claims to point at
// it. The rule is the one a stamp already follows when zoom carries its point
// off the panel: wait, and come back when there is something to point at.
const PLAYED = {
  spec: { data: "t",
    x: { field: "g" }, y: { field: "v" },
    layers: [{ mark: "point", encodings: { play: { field: "yr" } }, transforms: [] }],
    brush: [{ field: "g" }] },
  data: { t: { floats: { g: [10, 90], v: [10, 90], yr: [1952, 1957] } } },
};

test("a stamp shows in its own frame and waits out the others", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(PLAYED.spec, PLAYED.data);
    const p = panels[0];

    CLOCK.t = 0;
    container.send("pointerdown", placeOn(p.x, 10), placeOn(p.y, 10));
    container.send("pointerup", placeOn(p.x, 10), placeOn(p.y, 10));
    assert.equal(handle.stamps(), 1, "the row showing now can be stamped");
    const [stamp] = onPage("gog-stamp");
    assert.notEqual(stamp.style.display, "none", "and it is on the picture");

    // Halfway through the second frame. One frame is 0.8s by default.
    CLOCK.t = 1.2;
    globalThis.__frames(1);
    assert.equal(stamp.style.display, "none",
      "the frame moved on, so the stamp has nothing to point at");
    assert.equal(handle.stamps(), 1, "it waits rather than being taken off");

    // Round the loop, back to where it was stamped.
    CLOCK.t = 1.6;
    globalThis.__frames(1);
    assert.notEqual(stamp.style.display, "none",
      "and it comes back on its own point when its frame does");
    handle.destroy();
  } finally {
    CLOCK.t = 0;
    undo();
  }
});

test("a stamp on a plot that does not play is shown whatever the clock says", async () => {
  const undo = stubDom();
  try {
    const { handle, stamp } = await stampFixture();
    CLOCK.t = 5;
    globalThis.__frames(3);
    assert.notEqual(stamp.style.display, "none",
      "no frames to belong to, so no frame to wait for");
    assert.equal(handle.stamps(), 1);
    handle.destroy();
  } finally {
    CLOCK.t = 0;
    undo();
  }
});

test("the clock is watched only while a stamp belongs to a frame", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(PLAYED.spec, PLAYED.data);
    const p = panels[0];
    assert.equal(globalThis.__frames(5), 0,
      "nothing stamped, so nothing is watching the clock");

    container.send("pointerdown", placeOn(p.x, 10), placeOn(p.y, 10));
    container.send("pointerup", placeOn(p.x, 10), placeOn(p.y, 10));
    // Each frame the watcher takes asks for the next, so draining one always
    // leaves another waiting for as long as the stamp is there.
    for (let i = 0; i < 4; i++) {
      assert.equal(globalThis.__frames(1), 1, `frame ${i} was asked for`);
    }

    handle.clearStamps();
    // One frame was already asked for before the stamp went; it runs, finds
    // nothing to watch, and does not ask again.
    assert.equal(globalThis.__frames(5), 1, "the frame already in flight runs");
    assert.equal(globalThis.__frames(5), 0, "and the watcher has stopped");
    handle.destroy();
  } finally {
    CLOCK.t = 0;
    undo();
  }
});

test("a redraw moves the point and leaves the card where the reader put it", async () => {
  const undo = stubDom();
  try {
    const { handle, container, p, stamp, card } = await stampFixture();
    carry(card, 140, -60);
    const placed = offsetOf(card);
    const wasAt = parseFloat(stamp.style.left);

    SHIFT.x = 100;
    const cx = (v) => placeOn(p.x, v) + SHIFT.x;
    const cy = (v) => placeOn(p.y, v) + SHIFT.y;
    container.send("pointerdown", cx(10), cy(10));
    container.send("pointermove", cx(60), cy(60));
    container.send("pointerup", cx(60), cy(60));

    assert.equal(parseFloat(stamp.style.left) - wasAt, 100,
      "the anchor followed the picture");
    assert.deepEqual(offsetOf(card), placed,
      "and the card kept its offset, so it rode along rather than snapping back");
    handle.destroy();
  } finally {
    SHIFT.x = 0;
    undo();
  }
});

// The line is the part a reader watches while they carry a card, so it has to
// end on the card's border rather than under its text, and it has to keep
// pointing at the row. The head is what says which end is the data, and it earns
// its place only once the line is long enough to be ambiguous without it.
test("the line reaches the card's edge, points at the row, and grows a head when long", async () => {
  const undo = stubDom();
  try {
    const { handle, stamp, card } = await stampFixture();
    // The one thing the stub cannot answer for itself. A real browser measures
    // the card; here the test says how big it is, so the geometry has something
    // to meet.
    const [w, h] = [120, 40];
    card.offsetWidth = w;
    card.offsetHeight = h;
    const { line, head } = leaderOf(stamp);

    // Barely off its point, which is where a card starts.
    carry(card, 0, -4);
    assert.equal(head.getAttribute("visibility"), "hidden",
      "no head while the line is too short to be read either way");

    carry(card, 160, -102);
    const [dx, dy] = offsetOf(card);
    const [x1, y1] = [Number(line.getAttribute("x1")), Number(line.getAttribute("y1"))];
    const [x2, y2] = [Number(line.getAttribute("x2")), Number(line.getAttribute("y2"))];

    // On the border: one of the two faces is exactly half a card from the center.
    const [cx, cy] = [dx, dy - h / 2];
    const onFace = Math.abs(Math.abs(x2 - cx) - w / 2) < 0.01 ||
                   Math.abs(Math.abs(y2 - cy) - h / 2) < 0.01;
    assert.ok(onFace, `the far end sits on the card's border, not inside it (${x2}, ${y2})`);
    assert.ok(Math.abs(x2 - cx) <= w / 2 + 0.01 && Math.abs(y2 - cy) <= h / 2 + 0.01,
      "and not beyond it either");

    // Aimed at the row: the point, the near end and the far end are one line.
    assert.ok(Math.abs(x1 * y2 - x2 * y1) < 0.01,
      "the point, the near end and the far end are collinear");
    assert.ok(Math.hypot(x1, y1) >= 5, "the near end clears the dot");
    assert.ok(Math.hypot(x1, y1) < Math.hypot(x2, y2), "and runs toward the card");

    assert.equal(head.getAttribute("visibility"), "visible",
      "carried this far, the line says which end is the data");
    const [ax, ay] = head.getAttribute("points").split(" ")[0].split(",").map(Number);
    assert.ok(Math.hypot(ax, ay) < Math.hypot(x1, y1),
      "and the head's apex is the end nearest the row");
    handle.destroy();
  } finally {
    undo();
  }
});

test("a plot that cannot place a row cannot be stamped either", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(
      { ...POINTS.spec, coord: { polar: {} } }, POINTS.data);
    const p = panels[0];
    const [px, py] = [placeOn(p.x, 50), placeOn(p.y, 50)];
    container.send("pointerdown", px, py);
    container.send("pointerup", px, py);
    assert.equal(handle.stamps(), 0,
      "the same gate that refuses the readout refuses the stamp");
    handle.destroy();
  } finally {
    undo();
  }
});

test("destroy takes the stamps with it", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(POINTS.spec, POINTS.data);
    const p = panels[0];
    container.send("pointerdown", placeOn(p.x, 50), placeOn(p.y, 50));
    container.send("pointerup", placeOn(p.x, 50), placeOn(p.y, 50));
    assert.equal(onPage("gog-stamp").length, 1);
    handle.destroy();
    assert.equal(onPage("gog-stamp").length, 0,
      "nothing of this plot is left on the page");
  } finally {
    undo();
  }
});

test("the view says when it has moved, so anything anchored to it can follow", () => {
  const plot = fakePlot();
  const view = attachView(plot);
  let moves = 0;
  const stop = view.onApply(() => { moves += 1; });

  view.zoom(2);
  view.panBy(10, 10);
  view.reset();
  assert.equal(moves, 3, "zoom, pan and fit each say so");

  stop();
  view.zoom(2);
  assert.equal(moves, 3, "and it stops when it is told to");
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

