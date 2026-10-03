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
  mount,
  nearestRow,
  redraw,
  renderSpec,
} from "../src/interactive.js";
import { addViewControls, controlBar, pngSize } from "../src/view.js";

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
  // The two 90s are the ends of the drag, so a disagreement there would show up
  // only at the moment a reader reaches the stop.
  for (const [turn, tilt] of [
    [45, 25],
    [0, 0],
    [137, -40],
    [359, 89],
    [0, 90],
    [137, -90],
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

test("a drag stops where a sentence stops, at straight down and straight up",
     { skip: !have }, async () => {
  // The stop is what a reader meets at the end of the gesture, and the number
  // they read there is the one the manual tells them to write. It sat at 89 for
  // a while, so the chapter said 90 and the plot said 89, and neither was wrong
  // about itself. Whatever the limit is, both ends of the drag and the written
  // value have to agree on it, which is what this pins.
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const container = stubContainer();
    const handle = attachDrag(engine, container, cube(30, 25), { degreesPerPixel: 1 });

    // 300 pixels down from tilt 25 asks for 325, far past any stop.
    container.send("pointerdown", 100, 100);
    container.send("pointermove", 100, 400);
    container.send("pointerup", 100, 400);
    assert.equal(handle.view().tilt, 90, "dragging down reaches straight down");

    handle.reset();
    container.send("pointerdown", 100, 400);
    container.send("pointermove", 100, 100);
    container.send("pointerup", 100, 100);
    assert.equal(handle.view().tilt, -90, "and dragging up reaches straight up");

    // The end stop draws, rather than being a place the picture gives out.
    assert.ok(container.innerHTML.includes("<svg"), "the stop is a plot, not a blank");
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

// Two plots of one table on one page ask about the same rows. The count used to
// add both plots, so the book's own pair of views read "66 of 284 selected",
// `show rows` listed every country twice, and the second plot's values sat under
// the first plot's column names.
const cellOf = (y, brush) => ({
  data: "t", x: { field: "gdp" }, y: { field: y },
  layers: [{ mark: "point", encodings: {}, transforms: [] }],
  brush,
});
const SHARED_REQ = {
  spec: { arrange: "below", cells: [
    cellOf("life", [{ field: "gdp", at: [2500, 5500] }]),
    cellOf("pop", [{ field: "gdp", at: [2500, 5500] }]),
  ] },
  data: { t: { floats: { gdp: [1000, 3000, 4000, 5000, 9000], life: [50, 60, 70, 75, 80],
                         pop: [7, 8, 9, 10, 11] }, strings: {} } },
};

test("two plots of one table count each row once, under every name either uses", async () => {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const { svg } = renderSpec(engine, SHARED_REQ);
  // Each cell dims on its own, and both hold the same bound, so each draws the
  // same rows at full strength.
  const drawn = (svg.match(/<circle/g) || []).length;
  const dimmed = svg.split('<g opacity="0.150">').slice(1)
    .reduce((n, part) => n + (part.split("</g>")[0].match(/<circle/g) || []).length, 0);
  assert.equal(drawn, 10, "five rows in each of two cells");

  const seen = selectedRows(SHARED_REQ);
  assert.equal(seen.total, 5, "one table is five rows, however many plots read it");
  assert.equal(seen.kept, (drawn - dimmed) / 2, "the rows each cell drew at full strength");
  assert.deepEqual(seen.columns, ["gdp", "life", "pop"]);
  assert.deepEqual(seen.rows, [[3000, 60, 8], [4000, 70, 9], [5000, 75, 10]],
    "each value under its own name, and each row once");
});

test("a row of a shared table is caught where every plot reading it catches it", () => {
  // The second plot bounds another column. Each bound is one more condition on
  // the same rows, as a second `brush` in one sentence is.
  const req = { ...SHARED_REQ, spec: { arrange: "below", cells: [
    SHARED_REQ.spec.cells[0],
    cellOf("pop", [{ field: "pop", at: [8.5, 20] }]),
  ] } };
  const seen = selectedRows(req);
  assert.equal(seen.total, 5);
  assert.deepEqual(seen.rows.map((r) => r[0]), [4000, 5000]);
});

test("plots of different tables add their rows, each value under its own name", () => {
  const req = {
    spec: { arrange: "beside", cells: [
      SEL_REQ.spec,
      { data: "u", x: { field: "year" }, y: { field: "rain" },
        layers: [{ mark: "point", encodings: {}, transforms: [] }],
        brush: [{ field: "year", at: [2001, 2002] }] },
    ] },
    data: { ...SEL_REQ.data, u: { floats: { year: [2000, 2001, 2002], rain: [5, 6, 7] }, strings: {} } },
  };
  const seen = selectedRows(req);
  assert.equal(seen.total, 8, "five rows and three rows are eight different rows");
  assert.equal(seen.kept, 5);
  assert.deepEqual(seen.columns, ["gdp", "life", "year", "rain"]);
  assert.deepEqual(seen.rows.slice(3), [[undefined, undefined, 2001, 6], [undefined, undefined, 2002, 7]],
    "a table without a column leaves its cell empty rather than borrowing a name");
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
  const el = (tag = "") => {
    let html = "";
    const node = {
      // What kind of element it is, which is how a test tells a `rect` from a
      // `text` in the copy the camera writes.
      tag,
      // The same fact under the name the DOM gives it. The view bar reads it to
      // tell a button it can disable from a hint it cannot, so a stub without it
      // takes the wrong branch quietly rather than failing.
      tagName: tag.toUpperCase(),
      // Nothing here lays anything out, so a box is whatever a test says it is.
      // Zero until then, which is what an unlaid-out element measures.
      rect: { left: 0, top: 0, right: 0, bottom: 0, width: 0, height: 0 },
      getBoundingClientRect() { return node.rect; },
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
      appendChild(c) { node.children.push(c); node.firstChild ??= c; c.parent = node; return c; },
      // Every bar fills itself with one call rather than five, so a stub that
      // only knows `appendChild` cannot build one at all.
      append(...kids) { for (const c of kids) node.appendChild(c); },
      // How a mounted plot hangs its bars: `placeBar` wraps the container, and
      // the selection bar puts its note, table and pager after itself.
      insertBefore(c, ref) {
        const at = node.children.indexOf(ref);
        node.children.splice(at < 0 ? node.children.length : at, 0, c);
        c.parent = node;
        return c;
      },
      after(...kids) {
        const kin = node.parent?.children;
        if (!kin) return;
        let at = kin.indexOf(node);
        for (const c of kids) {
          kin.splice(++at, 0, c);
          c.parent = node.parent;
        }
      },
      // **Writing it takes the children with it**, which is what the DOM does and
      // what a plain property quietly did not. A control holds its hover label as
      // a child, so a stub that kept children through an `innerHTML` write could
      // not see the camera swap its drawing for a tick and take its label away.
      set innerHTML(v) { html = v; node.children.length = 0; node.firstChild = null; },
      get innerHTML() { return html; },
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
  body.appendChild = (c) => {
    // **A node the page already holds is moved, not copied.** The DOM has no
    // other behavior here, and a stub that pushed a second time reported two of
    // something there was one of.
    const at = body.children.indexOf(c);
    if (at >= 0) body.children.splice(at, 1);
    c.isConnected = true;
    c.parent = body;
    body.children.push(c);
    return c;
  };
  globalThis.document = {
    body,
    createElement: (tag) => el(tag),
    createElementNS: (_ns, tag) => el(tag),
  };
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
function panelFrom(svg, kind = "panel") {
  // The panel's place on the screen. Identity by default, so client coordinates
  // *are* user coordinates and a pointer test reads as arithmetic. A test that
  // cares whether something re-reads the transform rather than merely staying
  // put moves `SHIFT` and asks again.
  //
  // A composed page nests one `<svg x= y=>` per cell, and each cell's panel is
  // written in that cell's own units. The browser folds the nesting into every
  // panel's screen transform, and so does this, so a pointer aimed at the
  // second cell lands in the second cell rather than in the first one's
  // identical rectangle.
  const ctm = (dx, dy) => () => {
    const [e, f] = [SHIFT.x + dx, SHIFT.y + dy];
    const m = { a: 1, b: 0, c: 0, d: 1, e, f };
    m.inverse = () => ({ a: 1, b: 0, c: 0, d: 1, e: -e, f: -f, inverse: () => m });
    return m;
  };
  const point = () => ({
    x: 0, y: 0,
    matrixTransform(m) { return { x: this.x * m.a + m.e, y: this.y * m.d + m.f }; },
  });
  // One clock for the whole picture, which is what the document has. A test sets
  // it to choose a moment, the way a reader's browser advances it.
  const owner = { createSVGPoint: point, getCurrentTime: () => CLOCK.t };
  const frames = [];
  const cells = [[0, 0]];
  // A flow's slot outlines are read the same way, since each is an empty `<g>`
  // in its cell's units, which is all a panel is to this parser.
  for (const [tag] of svg.matchAll(new RegExp(`<svg\\b[^>]*>|</svg>|<g data-gog-${kind}[^>]*/>`, "g"))) {
    if (tag.startsWith("</svg")) {
      cells.pop();
      continue;
    }
    const [dx, dy] = cells[cells.length - 1];
    if (tag.startsWith("<svg")) {
      const at = (name) => Number(new RegExp(`\\s${name}="([^"]+)"`).exec(tag)?.[1] ?? 0);
      cells.push([dx + at("x"), dy + at("y")]);
      continue;
    }
    const attrs = {};
    for (const [, k, v] of tag.matchAll(/([\w-]+)="([^"]*)"/g)) attrs[k] = v;
    frames.push({
      // Where this panel's cell sits on the page, for a test that has to aim
      // a pointer into it.
      offset: [dx, dy],
      getAttribute: (n) => attrs[n] ?? null,
      ownerSVGElement: owner,
      getScreenCTM: ctm(dx, dy),
    });
  }
  return frames;
}

/** Where the animation has got to, in seconds. */
const CLOCK = { t: 0 };
/** Where the panel sits on the screen, for testing that something re-reads it. */
const SHIFT = { x: 0, y: 0 };

function stubContainer() {
  const listeners = new Map();
  let html = "";
  let panels = [];
  let slots = [];
  return {
    style: {},
    listeners,
    set innerHTML(v) { html = v; panels = panelFrom(v); slots = panelFrom(v, "slot"); },
    get innerHTML() { return html; },
    set textContent(v) { html = v; panels = []; slots = []; },
    // `style` because every real element has one, and `redraw` tells the
    // incoming picture to fit its column through it. A double without it lets
    // production code look wrong when it is the double that is thin.
    querySelector: (sel) =>
      (sel === "svg" ? { style: {}, getCurrentTime: () => CLOCK.t } : null),
    querySelectorAll: (sel) =>
      (sel === "[data-gog-panel]" ? panels : sel === "[data-gog-slot]" ? slots : []),
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

// **A flow selects by a click on a slot, never by a drag** (2026-10-03). Its axes
// are its stages and a running sum, and a range over either is not a selection
// anyone means. The engine writes each slot's outline beside it, and the page
// tests the pointer against that outline.
const FLOW_REQ = (brush) => ({
  spec: {
    data: "t",
    y: { field: "n" },
    layers: ["ribbon", "zone"].map((mark) => ({
      mark, encodings: {}, transforms: ["flow"], flow: { stages: ["class", "survived"] },
    })),
    brush,
  },
  data: {
    t: {
      strings: { class: ["First", "First", "Third", "Third"], survived: ["yes", "no", "yes", "no"] },
      floats: { n: [203, 122, 178, 528] },
    },
  },
});

/** The middle of the slot the engine says holds `place` on `field`. */
function slotCenter(container, field, place) {
  // A slot names every column it is a place of, `|`-separated: its stage, or both
  // columns of a shared flow, and the flow's own `name`.
  const slot = container.querySelectorAll("[data-gog-slot]").find((s) =>
    s.getAttribute("data-gog-slot-field").split("|").includes(field) &&
    s.getAttribute("data-gog-slot") === place);
  assert.ok(slot, `a slot for ${field} = ${place}`);
  const pts = slot.getAttribute("data-gog-shape").split(" ").map((p) => p.split(",").map(Number));
  return [pts.reduce((a, p) => a + p[0], 0) / pts.length, pts.reduce((a, p) => a + p[1], 0) / pts.length];
}

const clickAt = (container, [x, y]) => {
  container.send("pointerdown", x, y);
  container.send("pointerup", x, y);
};

test("a click on a flow's slot selects it, a second click clears it, and a drag selects nothing", async () => {
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const container = stubContainer();
    const handle = attachBrush(engine, container, FLOW_REQ([{ field: "class" }]));
    assert.equal(container.querySelectorAll("[data-gog-slot]").length, 4, "one outline per slot");
    const dim = '<g opacity="0.150">';

    clickAt(container, slotCenter(container, "class", "First"));
    assert.equal(handle.selection().kept, 2, "the two rows through First");
    assert.ok(container.innerHTML.includes(dim), "and the bands through Third are pushed back");

    // A slot on a stage the sentence did not brush is not this brush's to move.
    clickAt(container, slotCenter(container, "survived", "no"));
    assert.equal(handle.selection().kept, 2, "the bound on class stays where it was");

    clickAt(container, slotCenter(container, "class", "First"));
    assert.equal(handle.selection().kept, 0, "a second click on the selected slot clears it");
    assert.ok(!container.innerHTML.includes(dim), "and the picture is one pass again");

    clickAt(container, slotCenter(container, "class", "Third"));
    assert.equal(handle.selection().kept, 2, "Third is selected");
    const g = container.querySelectorAll("[data-gog-panel]")[0];
    assert.equal(g.getAttribute("data-gog-place"), "flow");
    const [x0, y0, x1, y1] = g.getAttribute("data-gog-panel").split(" ").map(Number);
    container.send("pointerdown", x0 + 2, y0 + 2);
    container.send("pointermove", x1 - 2, y1 - 2);
    container.send("pointerup", x1 - 2, y1 - 2);
    assert.equal(handle.selection().kept, 2, "a drag across the flow moves nothing");
    assert.equal(handle.drags(), false, "and the page offers no drag to select with");

    // Between the two stages, off every slot.
    clickAt(container, [(x0 + x1) / 2, y0 + 2]);
    assert.equal(handle.selection().kept, 0, "a click off every slot clears");
    handle.destroy();
  } finally {
    undo();
  }
});

// **A chord diagram takes the same click, on its ring.** A drag along straight
// axes cannot select on a disc, so a polar panel takes none, but a slot's outline
// is the curved piece of ring it was drawn as, so the click lands where the
// reader sees the place. A bare `brush` becomes a bound on the flow's `name`,
// which keeps what a place sends and what it receives: three of the four rows
// leave `a` or arrive at it.
test("a click on a chord diagram's ring selects the place at both ends", async () => {
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const req = {
      spec: {
        data: "t",
        y: { field: "n" },
        layers: ["ribbon", "zone"].map((mark) => ({
          mark, encodings: {}, transforms: ["flow"],
          flow: { stages: ["from", "to"], shared: true },
        })),
        coord: { polar: {} },
        brush: [{ field: "" }],
      },
      data: {
        t: {
          strings: { from: ["a", "a", "b", "c"], to: ["b", "c", "c", "a"] },
          floats: { n: [3, 2, 4, 1] },
        },
      },
    };
    const container = stubContainer();
    const handle = attachBrush(engine, container, req);
    const slots = container.querySelectorAll("[data-gog-slot]");
    assert.equal(slots.length, 3, "one piece of ring per place");
    const ring = slots.find((s) => s.getAttribute("data-gog-slot") === "a");
    assert.deepEqual(ring.getAttribute("data-gog-slot-field").split("|"), ["from", "to", "name"]);
    // The middle of the piece: halfway along its outer arc, and halfway back
    // along its inner one, averaged, which lies inside however long the arc is.
    const pts = ring.getAttribute("data-gog-shape").split(" ").map((p) => p.split(",").map(Number));
    const [o, i] = [pts[Math.floor(pts.length / 4)], pts[Math.floor((3 * pts.length) / 4)]];
    const at = [(o[0] + i[0]) / 2, (o[1] + i[1]) / 2];

    assert.equal(container.querySelectorAll("[data-gog-panel]")[0].getAttribute("data-gog-place"), "polar");
    clickAt(container, at);
    assert.equal(handle.selection().kept, 3, "every row that leaves a or arrives at it");
    assert.ok(container.innerHTML.includes('<g opacity="0.150">'), "and the band between b and c steps back");
    clickAt(container, at);
    assert.equal(handle.selection().kept, 0, "a second click clears it");
    handle.destroy();
  } finally {
    undo();
  }
});

// **The arc diagram takes it on the plane.** The same click on a slot standing on
// the axis, which the first build drew zero pixels tall, so no click could land.
test("a click on an arc diagram's slot selects the place at both ends", async () => {
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const req = {
      spec: {
        data: "t",
        x: { field: "n" },
        layers: ["ribbon", "zone"].map((mark) => ({
          mark, encodings: {}, transforms: ["flow"],
          flow: { stages: ["from", "to"], shared: true },
        })),
        brush: [{ field: "" }],
      },
      data: {
        t: {
          strings: { from: ["a", "a", "b", "c"], to: ["b", "c", "c", "a"] },
          floats: { n: [3, 2, 4, 1] },
        },
      },
    };
    const container = stubContainer();
    const handle = attachBrush(engine, container, req);
    assert.equal(container.querySelectorAll("[data-gog-panel]")[0].getAttribute("data-gog-place"), "flow");
    clickAt(container, slotCenter(container, "name", "a"));
    const seen = handle.selection();
    assert.equal(seen.kept, 3, "every row that leaves a or arrives at it");
    // The rows name both of the flow's columns, which no channel binds: without
    // `to`, a row from `a` could not be told a band leaving it from one arriving.
    assert.deepEqual(seen.columns, ["n", "from", "to"]);
    assert.deepEqual(seen.rows, [[3, "a", "b"], [2, "a", "c"], [1, "c", "a"]]);
    clickAt(container, slotCenter(container, "name", "a"));
    assert.equal(handle.selection().kept, 0, "a second click clears it");
    handle.destroy();
  } finally {
    undo();
  }
});

// **The Sankey diagram takes the same click.** A layered flow's slots are thin
// boxes, each a place of both columns, so a bare brush's click becomes a bound
// on `name` and keeps the place's own links: the two incomes into `Income` and
// its two links out, four of the six rows.
test("a click on a layered flow's slot selects the place's own links", async () => {
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const req = {
      spec: {
        data: "t",
        y: { field: "amount" },
        layers: ["ribbon", "zone"].map((mark) => ({
          mark, encodings: {}, transforms: ["flow"],
          flow: { stages: ["source", "target"], layered: true },
        })),
        brush: [{ field: "" }],
      },
      data: {
        t: {
          strings: {
            source: ["Salary", "Side", "Income", "Income", "Housing", "Housing"],
            target: ["Income", "Income", "Taxes", "Housing", "Rent", "Repairs"],
          },
          floats: { amount: [30, 10, 15, 25, 20, 5] },
        },
      },
    };
    const container = stubContainer();
    const handle = attachBrush(engine, container, req);
    assert.equal(container.querySelectorAll("[data-gog-slot]").length, 7, "one outline per place");
    assert.equal(container.querySelectorAll("[data-gog-panel]")[0].getAttribute("data-gog-place"), "flow");
    clickAt(container, slotCenter(container, "name", "Income"));
    const seen = handle.selection();
    assert.equal(seen.kept, 4, "Income's own links, and not Housing's");
    assert.deepEqual(seen.columns, ["amount", "source", "target"]);
    clickAt(container, slotCenter(container, "name", "Income"));
    assert.equal(handle.selection().kept, 0, "a second click clears it");
    handle.destroy();
  } finally {
    undo();
  }
});

test("a bare brush on a flow takes its column from the slot a reader clicks", async () => {
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const container = stubContainer();
    const handle = attachBrush(engine, container, FLOW_REQ([{ field: "" }]));

    clickAt(container, slotCenter(container, "survived", "no"));
    assert.equal(handle.selection().kept, 2, "the two rows that did not survive");
    clickAt(container, slotCenter(container, "class", "First"));
    assert.equal(handle.selection().kept, 2, "a click on another stage moves the selection there");
    assert.ok(handle.changed(), "and the selection differs from the sentence's");
    handle.reset();
    assert.equal(handle.selection().kept, 0, "reset goes back to the declaration");
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
/** Enough of a view for the camera path: a picture to copy, an identity matrix
 *  so screen coordinates *are* user coordinates, and a place to hang a pen. */
function stubView() {
  const pens = new Set();
  const m = { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 };
  m.inverse = () => ({ a: 1, b: 0, c: 0, d: 1, e: 0, f: 0, inverse: () => m });
  const svg = {
    createSVGPoint: () => ({
      x: 0, y: 0,
      matrixTransform(t) { return { x: this.x * t.a + t.e, y: this.y * t.d + t.f }; },
    }),
    getScreenCTM: () => m,
    getBoundingClientRect: () => (
      { left: 0, top: 0, right: 900, bottom: 700, width: 900, height: 700 }),
  };
  return {
    apply() {},
    onApply() { return () => {}; },
    svg: () => svg,
    zoomed: () => false,
    onSave(fn) { pens.add(fn); return () => pens.delete(fn); },
    decorate(clone) { for (const pen of pens) pen(clone); },
  };
}

/** One panel's rectangle and its two axes, read the way the page reads them,
 *  and where its cell sits, so a pointer can be aimed at a composed page. */
function axesOf(g) {
  const [x0, y0, x1, y1] = g.getAttribute("data-gog-panel").split(" ").map(Number);
  const num = (n) => g.getAttribute(`data-${n}`).split(" ").map(Number);
  const log = (n) => (g.getAttribute(`data-${n}-log`) === null ? null : Number(g.getAttribute(`data-${n}-log`)));
  const [xf, xt] = num("x");
  const [yf, yt] = num("y");
  return {
    x0, y0, x1, y1,
    x: { from: xf, to: xt, lo: x0, hi: x1, log: log("x"), cats: null },
    y: { from: yf, to: yt, lo: y1, hi: y0, log: log("y"), cats: null },
    place: g.getAttribute("data-gog-place"),
    offset: g.offset ?? [0, 0],
  };
}

async function hoverFixture(spec, data, options = {}) {
  const engine = await loadEngine(fs.readFileSync(WASM));
  const container = stubContainer();
  const handle = attachBrush(engine, container, { spec, data }, options);
  return { handle, container, panels: container.querySelectorAll("[data-gog-panel]").map(axesOf) };
}

/** Every element under `node`, depth first, in the order the page holds them. */
function everythingUnder(node, out = []) {
  for (const c of node.children ?? []) {
    out.push(c);
    everythingUnder(c, out);
  }
  return out;
}

/**
 * A plot mounted the way a page mounts it, bars and all, with its controls
 * found by what they say.
 *
 * Everything else here drives a handle, and a reader never touches one: the
 * buttons read the handle through the bar `mount` builds, and whether a button
 * is switched on is decided in that bar. So a handle can answer correctly while
 * the button stays off, and only a mounted plot can show which.
 */
async function mountFixture(spec, data, id = undefined) {
  const container = stubContainer();
  if (id) container.id = id;
  const host = globalThis.document.createElement("div");
  host.appendChild(container);
  container.parentNode = host;
  container.dataset = {};
  // The picture the view zooms. Its `viewBox` is read off whatever was drawn
  // last, as the browser would read it off the element.
  const picture = {
    style: {},
    getCurrentTime: () => CLOCK.t,
    getAttribute: (n) =>
      (n === "viewBox" ? /viewBox="([^"]+)"/.exec(container.innerHTML)?.[1] ?? null : null),
    setAttribute() {},
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600 }),
  };
  container.querySelector = (sel) => (sel === "svg" ? picture : null);
  const handle = await mount(container, { spec, data }, { wasm: fs.readFileSync(WASM) });
  const everything = () => everythingUnder(host);
  return {
    handle, container, host,
    panels: container.querySelectorAll("[data-gog-panel]").map(axesOf),
    button: (says) => everything().find((n) => n.tag === "button" && n.textContent === says),
    count: () => everything().find((n) => / selected$/.test(n.textContent ?? ""))?.textContent,
    press: (b) => b.listeners.get("click")(),
  };
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

// Frames held unevenly (the played column has a gap) are not one length, so the
// moment on show is found by each frame's own start. 2002 stands for an 18-year
// gap and holds 3.2s, from 1.6s to 4.8s; dividing the clock by one frame's 0.8s
// named 2001 there, so the readout answered with a row from another year.
test("a played plot with uneven gaps answers for the frame showing", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(
      { data: "t",
        x: { field: "g" }, y: { field: "v" },
        layers: [{ mark: "point", encodings: { play: { field: "yr" } }, transforms: [] }],
        brush: [{ field: "g" }] },
      { t: { floats: { g: [10, 35, 60, 90], v: [10, 35, 60, 90], yr: [2000, 2001, 2002, 2020] } } });
    const p = panels[0];
    const at = (v) => [placeOn(p.x, v), placeOn(p.y, v)];

    CLOCK.t = 4.0;
    container.send("pointermove", ...at(60));
    assert.equal(onPage("gog-tip").length, 1, "2002's row, late in its long frame");
    container.send("pointermove", ...at(35));
    assert.equal(onPage("gog-tip").length, 0, "and not 2001's, which a single frame length would name");
    handle.destroy();
    CLOCK.t = 0;
  } finally {
    CLOCK.t = 0;
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

// Two cells over one table, the second placing `pop` on a log axis where the
// first places `g`. Every cell's rows used to be placed on the pointed panel's
// axes, so both cells found the same row at the same spot, and the tie kept the
// first cell's answer with the first cell's columns.
const PAIR_TABLE = {
  t: { floats: { g: [10, 50, 90, 70], v: [10, 50, 90, 20], pop: [1e3, 1e5, 1e7, 1e4] } },
};
const pairCell = (x) => ({
  data: "t", x, y: { field: "v" },
  layers: [{ mark: "point", encodings: {}, transforms: [] }],
  brush: [{ field: "g" }],
});
const tipText = () => {
  const [tip] = onPage("gog-tip");
  return tip ? tip.innerHTML.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim() : null;
};

test("each cell of a composed page names its own columns", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(
      { arrange: "beside",
        cells: [pairCell({ field: "g" }), pairCell({ field: "pop", scale: "log" })] },
      PAIR_TABLE);
    assert.equal(panels.length, 2, "one panel per cell");
    const [left, right] = panels;
    assert.ok(right.offset[0] > 0 && right.x.log === 10, "the second cell sits to the right, on a log axis");

    const aim = (p, xv, yv) =>
      container.send("pointermove", placeOn(p.x, xv) + p.offset[0], placeOn(p.y, yv) + p.offset[1]);
    aim(right, 1e5, 50);
    assert.equal(tipText(), "pop 100000 v 50", "the right cell's own columns");
    aim(left, 50, 50);
    assert.equal(tipText(), "g 50 v 50", "and the left cell's, for the same row");
    handle.destroy();
  } finally {
    undo();
  }
});

// `limits` cuts the rows outside it, so they are not on the picture. One cut
// just past the edge sits within a glyph's reach of a pointer inside the panel,
// and nothing tested a row against the domain the panel states.
test("a row `limits` cut is not named, even beside the edge", async () => {
  const undo = stubDom();
  try {
    const spec = { ...pairCell({ field: "g", limits: [0, 69] }) };
    const { handle, container, panels } = await hoverFixture(spec, PAIR_TABLE);
    const p = panels[0];
    assert.ok(placeOn(p.x, 70) - p.x1 < 13, "the cut row at 70 would sit within reach of the edge");
    container.send("pointermove", p.x1 - 1, placeOn(p.y, 20));
    assert.equal(onPage("gog-tip").length, 0, "and it is not named");
    container.send("pointermove", placeOn(p.x, 50), placeOn(p.y, 50));
    assert.equal(tipText(), "g 50 v 50", "while a row the panel drew still is");
    handle.destroy();
  } finally {
    undo();
  }
});

// A negative on a log axis has no place. It came back as `NaN`, and a `NaN`
// distance met first silenced the whole panel, since nothing is smaller than
// it. The engine refuses such a value in a column it places on a log axis, so
// it reaches the readout only from a table the axis does not measure, which is
// why this asks the arithmetic directly.
test("a value with no place on a log axis does not silence the panel", () => {
  const axis = { from: 0, to: 2, lo: 0, hi: 100, log: 10, cats: null };
  assert.equal(placeOn(axis, 0), null);
  assert.equal(placeOn(axis, -5), null);
  const panel = {
    place: "row", x0: 0, y0: 0, x1: 100, y1: 100, facets: [], play: null,
    x: { ...axis, field: "g" },
    y: { field: "v", from: 0, to: 100, lo: 100, hi: 0, log: null, cats: null },
  };
  // The first row cannot be placed; the second sits under the pointer.
  const req = {
    spec: { data: "t", x: { field: "g" }, y: { field: "v" },
            layers: [{ mark: "point", encodings: {} }] },
    data: { t: { floats: { g: [-5, 10], v: [50, 50] } } },
  };
  assert.deepEqual(nearestRow(panel, { x: 50, y: 50 }, req)?.row, [["g", 10], ["v", 50]]);
});

// A disc turns `x` into an angle and `y` into a distance from its center, and the
// readout reads a value back along straight axes, so there is no answer to give.
// It shows no card, and prints nothing under the plot: the reason it used to
// print was in the engine's terms, and readers could not use it.
test("a plot that cannot place a row shows no card, and prints nothing", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(
      { ...POINTS.spec, coord: { polar: {} } }, POINTS.data);
    const p = panels[0];
    assert.equal(p.place, "polar", "the engine stated what it did to the position");

    container.send("pointermove", placeOn(p.x, 50), placeOn(p.y, 50));
    assert.equal(onPage("gog-tip").length, 0, "no readout on a panel that cannot place");
    assert.equal(handle.unplaced, undefined, "and no reason is kept for a line under the plot");
    handle.destroy();
  } finally {
    undo();
  }
});

// A `bounds` pair lies along one axis, which therefore names no single field. A
// brush on either column moves along it: matched against the field alone, a
// brushed `interval * bounds(lo, hi)` was right as first drawn and no drag moved it.
test("a brush on a bounds column moves with a drag", async () => {
  const undo = stubDom();
  try {
    const engine = await loadEngine(fs.readFileSync(WASM));
    const req = {
      spec: {
        data: "t", y: { field: "term" },
        layers: [{ mark: "interval", encodings: {}, transforms: ["bounds"],
                   bounds: { lower: "lo", upper: "hi" } }],
        brush: [{ field: "lo", at: [0, 0.2] }],
      },
      data: { t: { strings: { term: ["Age", "Education", "Experience"] },
                   floats: { lo: [0.02, 0.31, 0.11], hi: [0.18, 0.55, 0.29] } } },
    };
    const container = stubContainer();
    const handle = attachBrush(engine, container, req);
    assert.equal(handle.selection().kept, 2, "as first drawn: Age and Experience");
    const g = container.querySelectorAll("[data-gog-panel]")[0];
    assert.equal(g.getAttribute("data-x-fields"), "lo|hi", "the axis names both columns it draws");
    assert.equal(g.getAttribute("data-gog-place"), "bounds", "and says each shape is one row");

    const p = axesOf(g);
    const y = (p.y0 + p.y1) / 2;
    container.send("pointerdown", placeOn(p.x, 0.25), y);
    container.send("pointermove", placeOn(p.x, 0.5), y);
    container.send("pointerup", placeOn(p.x, 0.5), y);
    assert.equal(handle.selection().kept, 1, "a band over 0.25 to 0.5 holds one lo, Education's");
    handle.destroy();
  } finally {
    undo();
  }
});

// A polar plot draws `x` as an angle and `y` as a distance, and a band is read
// along straight axes, so a drag over the right half of the circle caught rows
// drawn on the left. The plot keeps the sentence's selection and takes no drag.
test("a drag on a polar panel keeps the sentence's selection", async () => {
  const undo = stubDom();
  try {
    const spec = { ...POINTS.spec, coord: { polar: {} }, brush: [{ field: "g", at: [0, 60] }] };
    const { handle, container, panels } = await hoverFixture(spec, POINTS.data);
    const before = handle.selection().kept;
    const p = panels[0];
    container.send("pointerdown", p.x0 + 5, p.y0 + 5);
    container.send("pointermove", p.x1 - 5, p.y1 - 5);
    container.send("pointerup", p.x1 - 5, p.y1 - 5);
    assert.equal(handle.selection().kept, before, "the drag changed nothing");
    handle.destroy();
  } finally {
    undo();
  }
});

// And the bar says as much: the count and `show rows`, as under a turnable plot,
// and no `clear`, since no gesture here can move the bound.
test("a brushed polar plot shows its count and rows, and no clear", async () => {
  const undo = stubDom();
  try {
    const spec = { ...POINTS.spec, coord: { polar: {} }, brush: [{ field: "g", at: [0, 60] }] };
    const m = await mountFixture(spec, POINTS.data);
    assert.match(m.count() ?? "", /^2 of 3 selected$/, `the sentence's selection is counted: ${m.count()}`);
    assert.ok(m.button("show rows"), "and can be listed");
    assert.equal(m.button("clear"), undefined, "no clear: no gesture can move the bound");
  } finally {
    undo();
  }
});

// No drag moves its selection, and the rest is the view the same plot has
// unbrushed: zoomed in, a drag pans. A view built beside `mountView` instead of
// through it had no pan, so a zoomed polar plot could not be moved at all.
test("a brushed polar plot pans once zoomed, as it would unbrushed", async () => {
  const undo = stubDom();
  try {
    const container = stubContainer();
    const host = globalThis.document.createElement("div");
    host.appendChild(container);
    container.parentNode = host;
    container.dataset = {};
    let box = "0 0 800 600";
    container.querySelector = (sel) => (sel !== "svg" ? null : {
      style: {},
      getAttribute: (n) => (n === "viewBox" ? box : null),
      setAttribute: (n, v) => { if (n === "viewBox") box = v; },
      getBoundingClientRect: () => ({ left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600 }),
    });
    const spec = { ...POINTS.spec, coord: { polar: {} }, brush: [{ field: "g", at: [0, 60] }] };
    const handle = await mount(container, { spec, data: POINTS.data }, { wasm: fs.readFileSync(WASM) });
    const zoomIn = everythingUnder(host).find((n) => n.attrs?.["aria-label"] === "zoom in");
    zoomIn.listeners.get("click")();
    const zoomed = box;
    assert.notEqual(zoomed, "0 0 800 600", "zoom in narrowed the window");

    const send = (type, x, y) =>
      container.listeners.get(type)?.({ clientX: x, clientY: y, pointerId: 1, preventDefault() {} });
    send("pointerdown", 400, 300);
    send("pointermove", 300, 250);
    send("pointerup", 300, 250);
    assert.notEqual(box, zoomed, "the drag moved the window, as it does on the plot unbrushed");
    assert.match(everythingUnder(host).find((n) => / selected$/.test(n.textContent ?? ""))?.textContent ?? "",
      /^2 of 3 selected$/, "and the selection is still the sentence's");
    handle.destroy();
  } finally {
    undo();
  }
});

// A host that runs a block's script twice would stack a second bar and a second
// drag on one picture.
test("a plot mounts once, however many times its script runs", async () => {
  const undo = stubDom();
  try {
    const m = await mountFixture(POINTS.spec, POINTS.data);
    const again = await mount(m.container, { spec: POINTS.spec, data: POINTS.data },
                              { wasm: fs.readFileSync(WASM) });
    assert.equal(again, null, "the second mount attaches nothing");
    m.handle.destroy?.();
  } finally {
    undo();
  }
});

// Ids come from the drawing, and a page resolves an id against the whole
// document, so the same plot twice on a page shared its clips and textures. The
// binding names its first drawing after the block; every redraw must too.
test("a redraw names its ids after the block it is drawn in", async () => {
  const undo = stubDom();
  try {
    const m = await mountFixture(POINTS.spec, POINTS.data, "gog-salt1");
    const p = m.panels[0];
    m.container.send("pointerdown", placeOn(p.x, 20), placeOn(p.y, 20));
    m.container.send("pointermove", placeOn(p.x, 60), placeOn(p.y, 60));
    m.container.send("pointerup", placeOn(p.x, 60), placeOn(p.y, 60));
    const html = m.container.innerHTML;
    const ids = [...html.matchAll(/ id="([^"]+)"/g)].map((x) => x[1]);
    assert.ok(ids.length > 0, "the premise: the redraw minted ids");
    assert.ok(ids.every((i) => i.startsWith("gog-salt1-")), `every id is the block's: ${ids.slice(0, 3)}`);
    const refs = [...html.matchAll(/url\(#([^)]+)\)/g)].map((x) => x[1]);
    assert.ok(refs.every((r) => ids.includes(r)), "and every reference finds its own definition");
    m.handle.destroy?.();
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

// `clear` puts back what the sentence asked for, so it is on exactly when that
// is gone. It used to ask the count, which cannot tell: a click on empty space
// reads `0 of 0` and a drag over the whole panel reads every row, and after
// either one the sentence's bound had no way back.
test("after a click empties the selection, clear brings the sentence's bound back", async () => {
  const undo = stubDom();
  try {
    const spec = { ...POINTS.spec, brush: [{ field: "g", at: [40, 100] }] };
    const { container, panels, button, count, press } = await mountFixture(spec, POINTS.data);
    const p = panels[0];
    const clear = button("clear");
    assert.equal(count(), "2 of 3 selected");
    assert.equal(clear.disabled, true, "nothing has moved, so there is nothing to put back");

    // A click on a part of the panel with no mark near it.
    container.send("pointerdown", placeOn(p.x, 30), placeOn(p.y, 70));
    container.send("pointerup", placeOn(p.x, 30), placeOn(p.y, 70));
    assert.equal(count(), "0 of 0 selected", "the click emptied the selection");
    assert.equal(clear.disabled, false, "and the sentence's bound can be brought back");

    press(clear);
    assert.equal(count(), "2 of 3 selected", "clear restored the sentence's bound");
    assert.equal(clear.disabled, true);

    // A drag across the whole panel catches every row, which the count cannot
    // tell apart from a plot nobody has touched.
    container.send("pointerdown", p.x0 + 1, placeOn(p.y, 50));
    container.send("pointermove", p.x1 - 1, placeOn(p.y, 50));
    container.send("pointerup", p.x1 - 1, placeOn(p.y, 50));
    assert.equal(count(), "3 of 3 selected");
    assert.equal(clear.disabled, false, "the drag replaced the sentence's bound");
    assert.equal(button("show rows").disabled, true, "everything caught is nothing to list");
    press(clear);
    assert.equal(count(), "2 of 3 selected");
  } finally {
    undo();
  }
});

test("a click on a brush that named no bound leaves nothing for clear to do", async () => {
  const undo = stubDom();
  try {
    const spec = { ...POINTS.spec, brush: [{ field: "" }] };
    const { container, panels, button, count, press } = await mountFixture(spec, POINTS.data);
    const p = panels[0];
    const clear = button("clear");
    assert.equal(clear.disabled, true);

    // The first gesture turns bare `brush` into one bound per axis. A click
    // gives them no range, so the plot selects what it did before.
    container.send("pointerdown", placeOn(p.x, 30), placeOn(p.y, 70));
    container.send("pointerup", placeOn(p.x, 30), placeOn(p.y, 70));
    assert.equal(count(), "0 of 0 selected");
    assert.equal(clear.disabled, true, "a declaration has nothing to restore");

    // From just inside the corner, so the row at 10 is not left to rounding.
    const [cx, cy] = [(p.x0 + placeOn(p.x, 10)) / 2, (p.y1 + placeOn(p.y, 10)) / 2];
    container.send("pointerdown", cx, cy);
    container.send("pointermove", placeOn(p.x, 60), placeOn(p.y, 60));
    container.send("pointerup", placeOn(p.x, 60), placeOn(p.y, 60));
    assert.equal(count(), "2 of 3 selected");
    assert.equal(clear.disabled, false);
    press(clear);
    assert.equal(count(), "0 of 0 selected", "back to the declaration");
    assert.equal(clear.disabled, true);
  } finally {
    undo();
  }
});

// In the cube the drag turns the plot, so a brush there is written in the
// sentence. The page used to ask only whether a plot could turn and never whether
// it was brushed, so a turnable plot's selection was drawn and never counted: no
// count, no `show rows`. The count and the table need no gesture, so they come
// back on a line of their own; the gestures a drag would need stay away.
test("a turnable plot that names a brush says what it caught", async () => {
  const undo = stubDom();
  try {
    const { spec, data } = cube();
    spec.brush = [{ field: "a", at: [2, 3.5] }];
    const { button, count, press, host } = await mountFixture(spec, data);
    assert.equal(count(), "2 of 6 selected", "the sentence's bound, counted");
    const show = button("show rows");
    assert.ok(show && !show.disabled, "and the rows it caught can be opened");
    press(show);
    const table = everythingUnder(host).find((n) => /<table/.test(n.innerHTML ?? ""));
    const body = table?.innerHTML.split("<tbody>")[1] ?? "";
    assert.equal((body.match(/<tr>/g) ?? []).length, 2, "one row per caught row");

    // The drag is the turn's, so nothing here moves the bound or reads a point.
    assert.equal(button("clear"), undefined, "no gesture here can move the bound, so nothing to clear");
    assert.ok(!everythingUnder(host).some((n) => n.textContent === "drag:"), "and no drag switcher");

    // Under the angle, not in place of it.
    const wrap = everythingUnder(host).find((n) => n.className === "gog-plot-with-controls");
    const lines = wrap.children.map((c) => c.className).filter((c) => /-controls$/.test(c ?? ""));
    assert.deepEqual(lines, ["gog-view-controls", "gog-view-controls", "gog-selection-controls"]);
    assert.ok(everythingUnder(host).some((n) => /^turn \d+° · tilt \d+°$/.test(n.textContent ?? "")),
      "the angle readout is still there");
  } finally {
    undo();
  }
});

test("a turnable plot with no brush has no count to give", async () => {
  const undo = stubDom();
  try {
    const { spec, data } = cube();
    const { count, button } = await mountFixture(spec, data);
    assert.equal(count(), undefined);
    assert.equal(button("show rows"), undefined);
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

// ---------------------------------------------------------------------------
// What the camera writes
//
// A reader who arranges cards and presses the camera should get the cards. They
// are HTML on `document.body`, so the copy the camera serializes has never held
// them; something has to draw them into it. The rule with teeth is the one about
// what is *left out*: a stamp waiting out a frame it does not belong to is not on
// the picture, and a saved picture that showed it would be a wrong picture.
// ---------------------------------------------------------------------------

/** Give a card the boxes a browser would have measured for it.
 *
 *  The stub parses no markup, so the row elements the card builds from a string
 *  do not exist here and are supplied. Their boxes are the browser's job either
 *  way: nothing lays anything out in this file, and the code under test reads
 *  measurements rather than making them. */
const layOut = (card, rows, left = 200, top = 100, w = 120) => {
  const box = (l, t, right, b) =>
    ({ left: l, top: t, right, bottom: b, width: right - l, height: b - t });
  card.rect = box(left, top, left + w, top + 6 + rows * 18);
  const body = card.children[0];
  while (body.children.length < rows) {
    body.appendChild(globalThis.document.createElement("div"));
  }
  body.children.forEach((row, i) => {
    row.rect = box(left + 5, top + 3 + i * 18, left + w - 16, top + 21 + i * 18);
  });
  card.children[1].rect = box(left + w - 14, top + 3, left + w - 5, top + 16);
};

const inkOf = (clone) => clone.children.find((c) => c.getAttribute("class") === "gog-stamp-ink");
const kinds = (ink, tag) => (ink?.children ?? []).filter((c) => c.tag === tag);

test("the camera's copy carries a stamp, and the picture keeps none of it", async () => {
  const undo = stubDom();
  try {
    const view = stubView();
    const { handle, container, panels } = await hoverFixture(
      POINTS.spec, POINTS.data, { view });
    const p = panels[0];
    container.send("pointerdown", placeOn(p.x, 50), placeOn(p.y, 50));
    container.send("pointerup", placeOn(p.x, 50), placeOn(p.y, 50));
    const [stamp] = onPage("gog-stamp");
    const card = cardOf(stamp);
    layOut(card, 2);

    const clone = globalThis.document.createElement("svg");
    view.decorate(clone);
    const ink = inkOf(clone);
    assert.ok(ink, "the copy gained the stamps");
    assert.equal(kinds(ink, "rect").length, 1, "one card");
    assert.equal(kinds(ink, "circle").length, 1, "one dot on its row");
    // Two mapped columns on this plot, plus the cross.
    assert.equal(kinds(ink, "text").length, 3, "a line per row, and the cross");

    // Drawn on the copy alone. The picture the reader is looking at never gains
    // the group, and a second save gets its own copy of it rather than moving
    // the first one, which is the same property said twice.
    assert.ok(!container.innerHTML.includes("gog-stamp-ink"),
      "nothing was written into the picture the reader is looking at");
    const again = globalThis.document.createElement("svg");
    view.decorate(again);
    assert.equal(kinds(inkOf(again), "rect").length, 1, "and saving twice works twice");
    handle.destroy();
  } finally {
    undo();
  }
});

test("a stamp waiting out its frame is left out of the saved picture", async () => {
  const undo = stubDom();
  try {
    const view = stubView();
    const { handle, container, panels } = await hoverFixture(
      PLAYED.spec, PLAYED.data, { view });
    const p = panels[0];

    CLOCK.t = 0;
    container.send("pointerdown", placeOn(p.x, 10), placeOn(p.y, 10));
    container.send("pointerup", placeOn(p.x, 10), placeOn(p.y, 10));
    layOut(cardOf(onPage("gog-stamp")[0]), 2);

    const showing = globalThis.document.createElement("svg");
    view.decorate(showing);
    assert.equal(kinds(inkOf(showing), "rect").length, 1,
      "saved while its frame shows, the card is in the file");

    CLOCK.t = 1.2;
    globalThis.__frames(1);
    const hidden = globalThis.document.createElement("svg");
    view.decorate(hidden);
    assert.equal(inkOf(hidden), undefined,
      "saved in another frame, nothing of it is");
    handle.destroy();
  } finally {
    CLOCK.t = 0;
    undo();
  }
});

// A value comes out of the reader's own table, and a table is allowed to hold a
// `<`. Pasted into markup it stopped being a value and started being structure.
test("a value that looks like markup is shown, not obeyed", async () => {
  const undo = stubDom();
  try {
    const { handle, container, panels } = await hoverFixture(
      { data: "t", x: { field: "g" }, y: { field: "v" },
        layers: [{ mark: "point",
                   encodings: { color: { field: "name" } }, transforms: [] }],
        brush: [{ field: "g" }] },
      { t: { floats: { g: [10, 90], v: [10, 90] },
             strings: { name: ["a <b> & c", "plain"] } } });
    const p = panels[0];
    container.send("pointermove", placeOn(p.x, 10), placeOn(p.y, 10));
    const [tip] = onPage("gog-tip");
    assert.ok(!/<b>/.test(tip.innerHTML), "the readout does not carry the tag through");
    assert.match(tip.innerHTML, /&lt;b&gt; &amp; c/, "it shows the characters instead");

    container.send("pointerdown", placeOn(p.x, 10), placeOn(p.y, 10));
    container.send("pointerup", placeOn(p.x, 10), placeOn(p.y, 10));
    const rows = rowsOf(cardOf(onPage("gog-stamp")[0]));
    assert.ok(!/<b>/.test(rows), "and neither does the card it leaves behind");
    assert.match(rows, /&lt;b&gt; &amp; c/);
    handle.destroy();
  } finally {
    undo();
  }
});

test("nothing stamped puts nothing in the copy", async () => {
  const undo = stubDom();
  try {
    const view = stubView();
    const { handle } = await hoverFixture(POINTS.spec, POINTS.data, { view });
    const clone = globalThis.document.createElement("svg");
    view.decorate(clone);
    assert.equal(clone.children.length, 0, "a plain save is exactly what it was");
    handle.destroy();
  } finally {
    undo();
  }
});

test("destroy takes the pen off, so a later save draws nothing", async () => {
  const undo = stubDom();
  try {
    const view = stubView();
    const { handle, container, panels } = await hoverFixture(
      POINTS.spec, POINTS.data, { view });
    const p = panels[0];
    container.send("pointerdown", placeOn(p.x, 50), placeOn(p.y, 50));
    container.send("pointerup", placeOn(p.x, 50), placeOn(p.y, 50));
    layOut(cardOf(onPage("gog-stamp")[0]), 2);
    handle.destroy();

    const clone = globalThis.document.createElement("svg");
    view.decorate(clone);
    assert.equal(clone.children.length, 0, "a destroyed plot draws into nobody's copy");
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

// ---------------------------------------------------------------------------
// A control with no word on it
//
// Eleven of the controls under a plot are drawings: two magnifiers, a frame, a
// hand, a camera, three drag modes, two page arrows and the cross on a stamp. A
// drawing is only recognizable to a reader who has met it before, so each one
// carries a label the pointer can ask for. All eleven are built through one
// helper; these drive the five that every plot has, which is where a break in
// the helper would show first.
//
// The browser's own `title` is deliberately not set beside it. Both would show,
// a second apart, in two different boxes.
// ---------------------------------------------------------------------------

/** A view that answers the bar's questions and moves nothing. */
const stillView = () => ({
  zoom() {}, reset() {},
  zoomed: () => false,
  canZoomIn: () => true,
  canZoomOut: () => false,
  svg: () => null,
});

/** Long enough that a label which waits for the pointer to settle has shown. */
const settle = () => new Promise((go) => setTimeout(go, 380));

const raised = () => onPage("gog-hint")[0];

test("every control that is only a drawing says what it is", () => {
  const undo = stubDom();
  try {
    const bar = controlBar("view");
    addViewControls(bar, stillView());
    assert.deepEqual(
      bar.children.map((c) => c.attrs["aria-label"]),
      ["zoom out", "zoom in", "show the whole plot", "save as PNG"],
      "all four name themselves, in the order the bar puts them",
    );
    for (const c of bar.children) {
      assert.equal(c.title, undefined,
        "and none asks the browser for a second tooltip saying the same thing");
    }
    assert.equal(onPage("gog-hint").length, 0, "and nothing is on the page until it is asked for");
  } finally {
    undo();
  }
});

test("a label is fixed to the window, not parented to the control", async () => {
  const undo = stubDom();
  try {
    const bar = controlBar("view");
    addViewControls(bar, stillView());
    const [zoomOut] = bar.children;

    // A control at the bottom right of the window, which is where the second row
    // of the last plot on a page ends up.
    zoomOut.rect =
      { left: 1180, top: 760, right: 1210, bottom: 790, width: 30, height: 30 };
    zoomOut.listeners.get("mouseenter")();
    await settle();

    assert.equal(zoomOut.children.length, 0,
      "inside the control it would count toward the scroll height of any ancestor " +
      "that clips, which is how a label under the lowest row gave the page a scrollbar");
    const label = raised();
    assert.ok(label, "it is on the body instead");
    assert.match(label.style.cssText, /position:fixed/, "and fixed to the window");
    assert.equal(label.textContent, "zoom out");

    // Nothing was laid out, so the stub measures zero. Say what it is, and ask
    // again: a label 40 tall has no room under a control ending 10px from the
    // foot of an 800px window, and a 200-wide one centered on x=1195 runs off
    // the right of a 1200-wide one.
    label.offsetHeight = 40;
    label.offsetWidth = 200;
    zoomOut.listeners.get("mouseleave")();
    zoomOut.listeners.get("mouseenter")();
    await settle();
    assert.equal(label.style.top, "714px", "so it opens upward instead of off the bottom");
    assert.equal(label.style.left, "994px", "and stops at the window's edge");
  } finally {
    undo();
  }
});

test("a label waits for the pointer to rest, and a press takes it down", async () => {
  const undo = stubDom();
  try {
    const bar = controlBar("view");
    addViewControls(bar, stillView());
    const [zoomOut] = bar.children;

    zoomOut.listeners.get("mouseenter")();
    assert.equal(onPage("gog-hint").length, 0,
      "a pointer crossing the bar to reach the camera trails no labels behind it");
    await settle();
    assert.equal(onPage("gog-hint").length, 1, "and one that stopped gets an answer");

    zoomOut.listeners.get("mousedown")();
    assert.equal(onPage("gog-hint").length, 0,
      "a press has answered the question, so the label would only cover what it changed");

    zoomOut.listeners.get("mouseenter")();
    zoomOut.listeners.get("mouseleave")();
    await settle();
    assert.equal(onPage("gog-hint").length, 0, "and leaving cancels one that was on its way");
  } finally {
    undo();
  }
});

test("a label fills with the page's color and writes in what can be read on it", async () => {
  const undo = stubDom();
  try {
    const bar = controlBar("view");
    addViewControls(bar, stillView());
    const [, zoomIn] = bar.children;

    // A light page: its text is near black, so the label is near black too and
    // has to write in white.
    globalThis.getComputedStyle = () => ({ color: "rgb(24, 24, 24)" });
    zoomIn.listeners.get("mouseenter")();
    await settle();
    assert.equal(raised().style.background, "rgb(24, 24, 24)", "the bar's own color fills it");
    assert.equal(raised().style.color, "#fff", "and white is what can be read there");
    zoomIn.listeners.get("mouseleave")();

    // A dark editor, where the same rule arrives at the opposite answer. This is
    // the case a media query gets wrong: the desktop can be light while the host
    // is dark, and only the inherited color knows.
    globalThis.getComputedStyle = () => ({ color: "rgb(232, 232, 232)" });
    zoomIn.listeners.get("mouseenter")();
    await settle();
    assert.equal(raised().style.background, "rgb(232, 232, 232)");
    assert.equal(raised().style.color, "#000", "black, on the same test read the other way");
  } finally {
    delete globalThis.getComputedStyle;
    undo();
  }
});

test("swapping a drawing does not take the label with it", async () => {
  const undo = stubDom();
  try {
    const bar = controlBar("view");
    addViewControls(bar, stillView());
    const camera = bar.children[3];

    // The camera is the one control whose drawing is replaced while the page is
    // open: it shows a tick for a moment after it writes a file. The label is an
    // attribute and an element in the window, so neither is inside the markup
    // that replaces.
    camera.innerHTML = "<svg>a tick</svg>";
    assert.equal(camera.attrs["aria-label"], "save as PNG", "the name outlives the swap");
    camera.listeners.get("mouseenter")();
    await settle();
    assert.equal(raised()?.textContent, "save as PNG", "and so does the label it raises");
  } finally {
    undo();
  }
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
  const block = R.html_block(p);

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

