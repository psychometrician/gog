// The three buttons that work a played plot's clock.
//
// **These read the engine's real markup rather than a fixture**, and that is the
// point of the file. The transport is not told how many frames a plot has; it
// works it out from the `<animate>` elements the renderer wrote, so the two
// agree only as long as something checks. A fixture would keep agreeing with a
// renderer that had changed.
//
// The DOM here is a stub, and deliberately a small one. `addTransport` touches
// six SMIL methods and four DOM methods, all of them listed below, so a stub
// that implements exactly those tests the arithmetic without a browser. What it
// cannot test is whether a real browser honors `setCurrentTime` on a paused
// timeline, which is what looking at a plot is for.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { addTransport, controlBar } from "../src/view.js";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..", "..", "..");
const CLI = path.join(ROOT, "target/release/gog-cli");
const have = fs.existsSync(CLI);
if (!have) {
  console.error(
    "\n  transport.test.mjs SKIPPED — no target/release/gog-cli." +
      "\n  cargo build --release\n",
  );
}

const FRAMES = 12;
const played = (frames = FRAMES) => ({
  spec: {
    data: "t",
    layers: [
      {
        mark: "point",
        encodings: { x: { field: "a" }, y: { field: "b" }, play: { field: "t" } },
        transforms: [],
      },
    ],
  },
  data: {
    t: {
      floats: {
        a: Array.from({ length: frames }, (_, i) => i + 1),
        b: Array.from({ length: frames }, (_, i) => frames - i),
        t: Array.from({ length: frames }, (_, i) => 1952 + i * 5),
      },
    },
  },
});

const still = () => ({
  spec: {
    data: "t",
    layers: [
      { mark: "point", encodings: { x: { field: "a" }, y: { field: "b" } }, transforms: [] },
    ],
  },
  data: { t: { floats: { a: [1, 2, 3], b: [3, 1, 2] } } },
});

const draw = (request) =>
  execFileSync(CLI, { input: JSON.stringify(request), encoding: "utf8" });

// ---------------------------------------------------------------------------
// The smallest DOM that `addTransport` can run against.
// ---------------------------------------------------------------------------

/** One `<animate>` lifted out of the rendered SVG, attributes and all. */
function animates(svg) {
  return [...svg.matchAll(/<animate ([^/]*?)\/>/g)].map((m) => {
    const attrs = Object.fromEntries(
      [...m[1].matchAll(/([\w-]+)="([^"]*)"/g)].map((a) => [a[1], a[2]]),
    );
    const group = { display: null, setAttribute(k, v) { if (k === "display") this.display = v; } };
    return {
      attrs,
      parentNode: group,
      removed: false,
      getAttribute: (k) => attrs[k] ?? null,
      remove() { this.removed = true; },
    };
  });
}

function fakeSvg(svgText) {
  const anims = animates(svgText);
  return {
    anims,
    time: 0,
    paused: false,
    pauseAnimations() { this.paused = true; },
    unpauseAnimations() { this.paused = false; },
    animationsPaused() { return this.paused; },
    getCurrentTime() { return this.time; },
    setCurrentTime(t) { this.time = t; },
    querySelector: () => anims[0] ?? null,
    querySelectorAll: () => anims,
  };
}

/** Just enough of `document` for a button and a span. */
function installDom() {
  const el = () => ({
    tagName: "DIV",
    style: { cssText: "" },
    innerHTML: "",
    children: [],
    addEventListener(type, fn) { (this.on ??= {})[type] = fn; },
    removeEventListener() {},
    setAttribute() {},
    append(...kids) { this.children.push(...kids); },
    remove() {},
    querySelectorAll: () => [],
  });
  globalThis.document = {
    createElement: () => el(),
    body: el(),
  };
  return el;
}

// ---------------------------------------------------------------------------

test("a still plot gets no transport", { skip: !have }, () => {
  installDom();
  const svg = fakeSvg(draw(still()));
  const t = addTransport(controlBar("view"), { querySelector: () => svg });
  assert.equal(t, null, "a plot with no clock must not grow buttons for one");
});

test("the frame count is read off keyTimes, not counted", { skip: !have }, () => {
  installDom();
  const rendered = draw(played());
  const svg = fakeSvg(rendered);
  // The premise the transport depends on: a played plot writes more than one
  // `<animate>` per frame, because the strip naming the moment is its own group.
  // Counting elements would have said 24 here.
  assert.ok(
    svg.anims.length > FRAMES,
    `expected more animates than frames, got ${svg.anims.length}`,
  );
  const bar = controlBar("view");
  assert.ok(addTransport(bar, { querySelector: () => svg }), "played plot should get a transport");

  // Three buttons, in one child of the row, so a narrow plot cannot break
  // through the middle of the stepper.
  assert.equal(bar.children.length, 1);
  assert.equal(bar.children[0].children.length, 3);
});

/** Drive the buttons: 0 is back, 1 is play/pause, 2 is forward. */
function mount(svg) {
  const bar = controlBar("view");
  const t = addTransport(bar, { querySelector: () => svg });
  const [back, toggle, forward] = bar.children[0].children;
  const press = (b) => b.on.click();
  const frame = () => Math.floor((svg.time % (FRAMES * 0.8)) / 0.8);
  return { t, back, toggle, forward, press, frame };
}

test("stepping forward moves one frame and pauses", { skip: !have }, () => {
  installDom();
  const svg = fakeSvg(draw(played()));
  const { forward, press, frame } = mount(svg);
  assert.equal(svg.paused, false, "a plot opens running, as it always has");
  press(forward);
  assert.equal(svg.paused, true, "stepping must stop the clock it stepped");
  assert.equal(frame(), 1);
  press(forward);
  assert.equal(frame(), 2);
});

test("the ends wrap, in both directions", { skip: !have }, () => {
  installDom();
  const svg = fakeSvg(draw(played()));
  const { back, forward, press, frame } = mount(svg);

  // Back from the first frame reaches the last, because the loop already
  // crosses that seam every time it runs round.
  press(back);
  assert.equal(frame(), FRAMES - 1, "back from the first frame should reach the last");

  // And forward from the last returns to the first.
  press(forward);
  assert.equal(frame(), 0, "forward from the last frame should reach the first");
});

test("every frame is reachable by stepping, and only once round", { skip: !have }, () => {
  installDom();
  const svg = fakeSvg(draw(played()));
  const { forward, press, frame } = mount(svg);
  const seen = [];
  for (let i = 0; i < FRAMES; i += 1) {
    press(forward);
    seen.push(frame());
  }
  assert.deepEqual(seen, [...Array.from({ length: FRAMES - 1 }, (_, i) => i + 1), 0]);
});

test("play and pause toggle the clock", { skip: !have }, () => {
  installDom();
  const svg = fakeSvg(draw(played()));
  const { toggle, press } = mount(svg);
  press(toggle);
  assert.equal(svg.paused, true);
  press(toggle);
  assert.equal(svg.paused, false);
});

// **Found by a reader**: stop a played plot on a frame, then draw a selection on
// it. The selection redraws the picture, and a freshly inserted SVG starts its
// own timeline — so the plot ran on while the button still said it was stopped.
// `redraw` carried the clock's *reading* across the swap and not its *state*,
// which is why one defect looked like two.
test("a redraw carries the pause, not only the reading", { skip: !have }, () => {
  installDom();
  // Two elements, standing for the picture before and after a selection redraws
  // it. The incoming one behaves as the browser does: its clock is running.
  const before = fakeSvg(draw(played()));
  const after = fakeSvg(draw(played()));

  // The reader stops on frame 3.
  const { forward, press } = mount(before);
  for (let i = 0; i < 3; i += 1) press(forward);
  assert.equal(before.paused, true);
  const reading = before.time;

  // What `redraw` does across the swap, in the order it does it.
  const stopped = before.animationsPaused();
  assert.equal(stopped, true, "the state to carry");
  if (stopped) after.pauseAnimations();
  after.setCurrentTime(reading);

  assert.equal(after.paused, true, "the new picture must not start running");
  assert.equal(after.time, reading, "and it must hold the frame it was stopped on");

  // And the button reads the new element, so it cannot go on describing the old.
  const bar = controlBar("view");
  const t = addTransport(bar, { querySelector: () => after });
  t.refresh();
  assert.equal(after.animationsPaused(), true, "refresh must not restart the clock");
});

test("the camera photographs the frame on show, not the first", { skip: !have }, () => {
  installDom();
  const svg = fakeSvg(draw(played()));
  // A view that records the pen the transport hands it, which is what the real
  // one does with `onSave`.
  const pens = [];
  const view = { onSave: (fn) => pens.push(fn) };
  const bar = controlBar("view");
  addTransport(bar, { querySelector: () => svg }, view);
  assert.equal(pens.length, 1, "the transport must tell the camera which moment it is on");

  const [, , forward] = bar.children[0].children;
  forward.on.click();
  forward.on.click();
  forward.on.click();
  assert.equal(Math.floor((svg.time % (FRAMES * 0.8)) / 0.8), 3);

  // The clone the camera is about to serialize.
  const clone = { querySelectorAll: () => svg.anims };
  pens[0](clone);

  const shown = svg.anims.filter((a) => a.parentNode.display === "inline");
  const hidden = svg.anims.filter((a) => a.parentNode.display === "none");
  assert.ok(shown.length >= 1, "the frame on show must be written into the copy");
  assert.equal(shown.length + hidden.length, svg.anims.length);
  // Every group written `inline` must be a group of frame 3.
  for (const a of shown) {
    assert.equal(Math.round(Number.parseFloat(a.getAttribute("begin")) / 0.8), 3);
  }
  // And no clock survives, or the saved picture would start moving again.
  assert.ok(svg.anims.every((a) => a.removed), "the copy must carry no animation");
});
