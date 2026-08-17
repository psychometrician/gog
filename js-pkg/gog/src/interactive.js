// interactive.js — the engine in the page, and the mouse wired to the camera
//
// `render.js` is the bridge for a *process*: it spawns `gog-cli` and reads an
// SVG off stdout. This is the bridge for a *page*, where there is no process to
// spawn. It loads the same engine compiled to WebAssembly and calls it directly,
// which is what makes a 3-D plot turnable — dragging is the same spec re-rendered
// with two numbers changed, and at 60 frames a second a process launch per frame
// is not available while a function call is.
//
// No policy lives here either, for the same reason it does not live in
// `render.js`. Which plots are legal, what a missing value costs a row, how a
// cube projects — all of it is `gog-core`'s, reached through `gog-wasm`. What
// this file owns is exactly three things a browser has and Rust does not: when
// to redraw, what a mouse movement means in degrees, and the clock.
//
// The clock is the part worth reading. `play` swaps its frames with SMIL
// `<animate>` elements *inside* the SVG, and replacing that SVG restarts the
// SMIL timeline — so a naive redraw snaps an animation back to its first frame
// on every mouse movement, which measured as 3.00s becoming 0.00s. Reading
// `getCurrentTime()` off the outgoing element and writing it to the incoming one
// is the whole fix, and it is why a plot can be turned *while it plays*.

/** The engine's own defaults, mirrored from `ir.rs`. A spec that never named an
 *  angle is opening at these, so a drag has to start from them or the picture
 *  would jump on the first movement. */
import {
  attachView,
  addViewControls,
  addTransport,
  BUTTON_STYLE,
  controlBar,
  hoverLabel,
  placeBar,
  mountView,
} from "./view.js";

export const DEFAULT_TURN = 30;
export const DEFAULT_TILT = 25;

/**
 * Which build of this module a page is running.
 *
 * Stamped onto every plot it mounts, as `data-gog-build`. A page loads this file
 * by URL and browsers cache modules hard, so "is the reader seeing the fix?" has
 * been unanswerable from outside the browser — three separate defects this week
 * were reported against pages running an older copy, and each one cost a round
 * of guessing before anyone could rule it out. One attribute settles it.
 *
 * Bump it whenever the interaction behaves differently, not on every edit.
 */
export { attachView, mountView } from "./view.js";

export const BUILD = "2026-08-05";

/**
 * Engines already loaded, keyed by where they came from.
 *
 * A page can hold many plots and they must not each compile their own copy of
 * a 296 KB module. Keyed by URL rather than counted, so a notebook that inlines
 * the engine as a `data:` URI and a book that fetches a shared file both get
 * exactly one instance of whatever they named.
 */
const ENGINES = new Map();

/** Load once per source, and hand the same promise to every later caller. */
export function engineFor(source) {
  const key = typeof source === "string" ? source : "bytes";
  if (!ENGINES.has(key)) ENGINES.set(key, loadEngine(source));
  return ENGINES.get(key);
}

const STATUS_OK = 0;
const STATUS_BAD_JSON = 1;
const STATUS_REFUSED = 2;

/**
 * Load the WebAssembly engine.
 *
 * @param {string|BufferSource} source A URL to fetch `gog.wasm` from, or its
 *   bytes directly. Bytes are what a notebook uses, where the page must survive
 *   being emailed and cannot rely on a file sitting beside it.
 * @returns {Promise<object>} an engine handle for {@link renderSpec}.
 */
export async function loadEngine(source) {
  const bytes =
    typeof source === "string"
      ? await (await fetch(source)).arrayBuffer()
      : source;
  // `instantiate` on the bytes rather than `instantiateStreaming` on the
  // response: streaming requires the server to send `application/wasm`, and the
  // static hosts this has to work on — a notebook's file://, RStudio's Viewer
  // pane, a plain directory of HTML — cannot all be relied on to.
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const { alloc, dealloc, gog_render, gog_notes, memory } = instance.exports;
  return { alloc, dealloc, gog_render, gog_notes, memory };
}

/**
 * Render one spec.
 *
 * @param {object} engine from {@link loadEngine}
 * @param {object} request the `{spec, data}` wire object, the same shape the
 *   CLI reads on stdin
 * @returns {{svg: string|null, error: string|null, notes: string[]}}
 *   `svg` is null when the plot was refused, and `error` carries the engine's
 *   diagnostics — refused means nothing is drawn, never a broken picture.
 */
export function renderSpec(engine, request) {
  const { alloc, dealloc, gog_render, gog_notes, memory } = engine;

  const input = new TextEncoder().encode(JSON.stringify(request));
  const inPtr = alloc(input.length);
  new Uint8Array(memory.buffer, inPtr, input.length).set(input);

  const lenPtr = alloc(4);
  const statusPtr = alloc(4);
  // `gog_render` consumes `inPtr`; freeing it here would be a double free.
  const outPtr = gog_render(inPtr, input.length, lenPtr, statusPtr);

  // Read the out-parameters before anything else can grow linear memory, which
  // would detach every view built on the old buffer.
  const len = new Uint32Array(memory.buffer, lenPtr, 1)[0];
  const status = new Int32Array(memory.buffer, statusPtr, 1)[0];
  const text = new TextDecoder().decode(new Uint8Array(memory.buffer, outPtr, len));

  let notes = [];
  if (status === STATUS_OK) {
    const nLenPtr = alloc(4);
    const nPtr = gog_notes(nLenPtr);
    const nLen = new Uint32Array(memory.buffer, nLenPtr, 1)[0];
    const noteText = new TextDecoder().decode(new Uint8Array(memory.buffer, nPtr, nLen));
    if (noteText) notes = noteText.split("\n");
    dealloc(nPtr, nLen);
    dealloc(nLenPtr, 4);
  }

  // Every frame allocates its request and its SVG — roughly 240 KB for a plot
  // of any size. Dragging for a minute at 60 fps is most of a gigabyte if these
  // are not returned, and it surfaces as jank that reads exactly like a slow DOM.
  dealloc(outPtr, len);
  dealloc(lenPtr, 4);
  dealloc(statusPtr, 4);

  if (status === STATUS_OK) return { svg: text, error: null, notes };
  if (status === STATUS_BAD_JSON) return { svg: null, error: text, notes: [] };
  if (status === STATUS_REFUSED) return { svg: null, error: text, notes: [] };
  return { svg: null, error: `gog: unknown engine status ${status}`, notes: [] };
}

/** Does this **plot** carry a turnable view — the cube's, or the globe's?
 * The leaf question, asked per cell. The two spaces share their view words
 * (`turn`, `tilt`) and their gesture: a drag pins the thing drawn to the
 * pointer, whichever of the two it is. */
function plotIsSpatial(spec) {
  if (spec?.coord && typeof spec.coord === "object" && (spec.coord.space || spec.coord.globe)) return true;
  // A network with a *stated* angle is the cube form and turns; bare
  // `network()` is flat and has nothing to drag.
  const net = spec?.coord && typeof spec.coord === "object" ? spec.coord.network : null;
  if (net && (net.turn !== undefined || net.tilt !== undefined)) return true;
  // A `z` binding puts a plot in the cube without naming `space()`, so the
  // coordinate can still read "flat" on a plot that projects. `space_of` makes
  // the same judgment in the engine; this is its browser-side twin, and it is
  // why the check is not simply `coord.space`.
  const layers = spec?.layers ?? [];
  return layers.some((l) => l?.encodings && "z" in l.encodings) || spec?.z != null;
}

/**
 * Does this figure draw in the cube? Only an angle can be dragged.
 *
 * **Asked of every plot in the figure, because a page has no coordinate of its
 * own.** A composition keeps each cell's space on the cell, so looking only at
 * the top level said "flat" for a page of cubes: the drag was never attached,
 * and a reader who composed two turnable plots got a pair that would not turn.
 * Silently, since the picture is correct and only the gesture is missing.
 *
 * [`hasBrush`] three definitions down had recursed all along, which is what made
 * this hard to see: the same file answered the same shape of question two ways.
 * The engine's own `spec_is_spatial` recursed too, so a page still loaded the
 * WebAssembly it then had no use for.
 */
export function isSpatial(spec) {
  return eachPlot(spec).some(plotIsSpatial);
}

/**
 * Render a request and swap the result into the container, keeping the clock.
 *
 * Every interaction in this file is the same loop — change one field of the
 * spec, render, replace the picture — so this is where that loop lives. It was
 * extracted from `attachDrag` when a second caller arrived, rather than copied
 * into it, because the subtle part is not the swap: it is the two lines around
 * it. A `play` plot runs on SMIL's timeline, a fresh element starts that
 * timeline at zero, and without carrying the clock across the swap a drag would
 * restart the animation on every mouse move. One copy of that, or the second
 * caller silently loses it.
 *
 * @returns {{ok: boolean, notes: string[]}} `ok: false` means the engine refused
 *   and the container now holds the message; there is nothing to interact with.
 */
export function redraw(engine, container, req, options = {}) {
  const { keep = false } = options;
  const { svg, error, notes } = renderSpec(engine, req);

  if (error !== null) {
    // A refusal on the **first** draw is the engine's to explain: there is no
    // picture yet, so showing the message beats leaving an empty box.
    //
    // Mid-gesture it is the opposite, and getting this wrong is what made the
    // first build of the brush unusable. Replacing the SVG with a message
    // destroys the panels the next pointer event would be measured against, so
    // one refused frame killed the plot for the rest of the page — and a plain
    // click refused, because a zero-width drag is a range that does not run
    // upward. Keep the last good picture and hand the caller the message.
    if (!keep) container.textContent = error;
    return { ok: false, notes: [], error };
  }

  // **A redraw must carry the clock's state as well as its reading**, and the
  // two are separate facts. Found by a reader: stop a played plot on one frame,
  // then draw a selection on it. The frame came across correctly and then ran on,
  // because a freshly inserted SVG starts its own timeline, and the transport's
  // button was left claiming the plot was stopped while it played. Carrying the
  // time and not the pause is what made that look like two bugs.
  const outgoing = container.querySelector("svg");
  let clock = null;
  let stopped = false;
  if (outgoing && typeof outgoing.getCurrentTime === "function") {
    try {
      clock = outgoing.getCurrentTime();
    } catch {
      clock = null;
    }
    // Asked separately, so a browser that refuses one still answers the other. A
    // shared `try` would have thrown the reading away to learn nothing about the
    // state, which is the worse of the two failures.
    try {
      stopped = outgoing.animationsPaused?.() ?? false;
    } catch {
      stopped = false;
    }
  }

  container.innerHTML = svg;

  const incoming = container.querySelector("svg");

  // **The engine draws a fixed canvas and knows nothing about the column it
  // lands in**, so the picture has to be told to fit — the same thing every
  // binding does to the *static* SVG before writing it into the page. This is
  // that instruction applied to the redrawn one, and without it a redraw
  // silently undoes it: the element the engine hands back carries no style, so
  // it keeps its drawn width in a column narrower than that. A flat plot is
  // never redrawn and shrinks; a cube beside it on the same page did not, which
  // is how one page showed two behaviors. Measured at a 1000px window, the plot
  // ran 272px past its column.
  //
  // It belongs here and not in three other places. A stylesheet would fix the
  // book and miss a notebook and a saved page, which is exactly why the inline
  // style exists. The engine cannot carry it either: fitting a column is one
  // host's concern and would be meaningless in a `.svg` written to disk, so
  // Law 9 keeps it out of the IR. That leaves the browser-side swap, which is
  // where the style is lost and the only place that sees every host.
  if (incoming) {
    incoming.style.maxWidth = "100%";
    incoming.style.height = "auto";
  }

  if (clock !== null && incoming && typeof incoming.setCurrentTime === "function") {
    try {
      // Stop it before seeking, so the moment being restored cannot be stepped
      // past by the timeline that started when the element was inserted.
      if (stopped) incoming.pauseAnimations?.();
      incoming.setCurrentTime(clock);
    } catch {
      /* a static plot has no timeline; nothing to restore */
    }
  }

  return { ok: true, notes, error: null };
}

/**
 * Where two pixels fall on an axis, in the column's own units — or, on a column
 * of categories, which slots they cover.
 *
 * Pure arithmetic over what the engine already stated, which is the whole point:
 * no scale knowledge lives here, so a log axis and a calendar axis need no cases.
 *
 * **The two ends are sorted by fraction, never by pixel**, and that is the one
 * subtlety. The y axis runs *down* the screen and *up* the data, so it arrives
 * with its ends swapped (`lo` is the bottom edge). Sorting pixels first looked
 * right and inverted every vertical selection: the smaller pixel is the larger
 * value, so the range came out backwards and the engine refused it — correctly,
 * since a range that does not run upward selects nothing. Dragging on y did
 * nothing at all, and so did the rectangle, because one bad bound refuses the
 * whole plot.
 *
 * Exported because it is the only part of the gesture that can be wrong in a way
 * a test can see. Everything else needs a pointer and a DOM.
 */
export function boundOn(axis, a, b) {
  const frac = (v) => (v - axis.lo) / (axis.hi - axis.lo || 1);
  const [f0, f1] = [frac(a), frac(b)].sort((m, n) => m - n);
  // A fraction of the panel, read back as the number the axis says it spans.
  // Both readings below start here, because both kinds of axis are drawn from
  // the same two numbers: a category sits at a whole one of them, a measurement
  // anywhere between them.
  const value = (f) => axis.from + f * (axis.to - axis.from);
  if (axis.cats) {
    const n = axis.cats.length;
    // The slot under a pixel is that number rounded. It is **not** the fraction
    // times the count, which is what this did until it met an axis wider than
    // its own slots: `density(reach = )` past half a slot widens the domain to
    // make room for shapes that lean out of it, and every category then sits
    // somewhere the count does not predict. On an unwidened axis the two agree
    // exactly, which is why the old reading survived so long.
    const slot = (f) => Math.max(0, Math.min(n - 1, Math.round(value(f))));
    return { levels: axis.cats.slice(slot(f0), slot(f1) + 1) };
  }
  // A log axis states its domain in log space, because that is the space
  // positions are linear in, so interpolating between its two numbers gives a
  // logarithm rather than a value. Undoing that is the whole of what `log` is
  // for, and without it a drag on a log axis produced a bound in units the
  // engine does not compare against.
  const undo = (f) => (axis.log ? axis.log ** value(f) : value(f));
  return { at: [undo(f0), undo(f1)] };
}

/**
 * Where a value sits on an axis, in the units the panel rectangle is written
 * in. `boundOn` run forwards, and the only other arithmetic the browser needs.
 */
export function placeOn(axis, value) {
  if (axis.cats) {
    const i = axis.cats.indexOf(value);
    if (i < 0) return null;
    // A category is a whole number on its own axis, so this is the measured
    // line below with the category's place standing in for a measurement.
    // Reading the domain the panel states, rather than counting the categories,
    // is what keeps an axis widened by `density(reach = )` honest.
    return axis.lo + ((i - axis.from) / (axis.to - axis.from || 1)) * (axis.hi - axis.lo);
  }
  if (typeof value !== "number" || !Number.isFinite(value)) return null;
  const v = axis.log ? Math.log(value) / Math.log(axis.log) : value;
  const f = (v - axis.from) / (axis.to - axis.from || 1);
  return axis.lo + f * (axis.hi - axis.lo);
}

/**
 * One pixel, in the column's own units. `placeOn` run backwards, and what a
 * traced outline is made of: a lasso is a list of these, two per pointer sample.
 *
 * `null` on a column of **categories**, and that is the boundary rather than a
 * gap. A category has no half, so a free shape has nothing to say about one —
 * where either axis names categories the drag stays a rectangle and selects
 * whole slots, which is what the reader wanted there anyway.
 */
export function valueOn(axis, px) {
  if (!axis || axis.cats) return null;
  const f = (px - axis.lo) / (axis.hi - axis.lo || 1);
  const v = axis.from + f * (axis.to - axis.from);
  return axis.log ? axis.log ** v : v;
}

/**
 * Is this point inside the traced outline? Ray casting: count the edges a ray
 * to the right crosses, and an odd count is inside.
 *
 * The engine runs exactly this, in `RegionDef::holds`, and two implementations
 * of one rule is a drift surface — the same one `selectedRows` already accepts
 * for a bound, and for the same reason. The browser has to answer "how many did
 * I catch" without asking the engine, and a test pins both against one shape.
 */
export function holdsIn(path, x, y) {
  if (!Array.isArray(path) || path.length < 3) return false;
  if (!Number.isFinite(x) || !Number.isFinite(y)) return false;
  let inside = false;
  for (let i = 0; i < path.length; i++) {
    const [ax, ay] = path[i];
    const [bx, by] = path[(i + 1) % path.length];
    if (ay > y !== by > y) {
      if (x < ax + ((y - ay) / (by - ay)) * (bx - ax)) inside = !inside;
    }
  }
  return inside;
}

/**
 * Every plot in a figure: the plot itself, or each cell of a page, all the way
 * down, since a page nests.
 *
 * A page is where the selection stops being one plot's business. Two composed
 * plots that name the same column are already answering the same predicate —
 * that needs nothing added, because a bound is a fact about a column and not
 * about a panel. What needs saying is only that one *drag* reaches all of them.
 */
function eachPlot(spec, out = []) {
  if (!spec || typeof spec !== "object") return out;
  const cells = spec.cells ?? spec.plots;
  if (Array.isArray(cells) && cells.length) {
    for (const cell of cells) eachPlot(cell, out);
  } else {
    out.push(spec);
  }
  return out;
}

/** Does this figure name a selection the reader can move? */
export function hasBrush(spec) {
  return eachPlot(spec).some((p) => Array.isArray(p.brush) && p.brush.length > 0);
}

/**
 * Drag a rectangle over a panel, and the rows outside it step back.
 *
 * The whole of the coordinate work is here, and it is deliberately arithmetic
 * rather than knowledge. The engine writes each panel's rectangle and each
 * axis's domain into the SVG — `data-gog-panel`, `data-x-field`, `data-x` — so
 * this function never has to know what a log scale is, where a category's slot
 * falls, or which column an axis ended up reading after scope resolution. It
 * measures where the pointer landed inside a rectangle and reads the answer off
 * a straight line. A browser that worked any of that out for itself would be a
 * second copy of the scale code in another language, which is the drift that
 * cost this project its second renderer.
 *
 * A drag writes `at` onto the brushes whose column an axis of the panel under
 * the pointer measures, and leaves the rest alone. Brushing a column the plot
 * does not place — a third variable, bound but never drawn — is a legal
 * sentence the mouse simply cannot reach, and it stays where it was written.
 *
 * @returns {{destroy: () => void, reset: () => void, opened: object[]}}
 */
export function attachBrush(engine, container, request, options = {}) {
  const { onNotes, onSelect, view } = options;
  let dragMode = "select";
  let picked = "select";
  const mode = () => dragMode;
  const req = JSON.parse(JSON.stringify(request));
  // What the sentence asked for, so `reset` returns there rather than to
  // nothing — the same rule `attachDrag` follows for the angle a plot opens at.
  const opened = eachPlot(req.spec).map((p) => JSON.parse(JSON.stringify(p.brush ?? [])));

  let first = true;
  function draw() {
    const { ok, notes } = redraw(engine, container, req, { keep: !first });
    if (!ok) return false;
    if (first) {
      first = false;
      if (onNotes && notes.length) onNotes(notes);
    }
    // The element the `viewBox` was set on is gone; put it back before the
    // browser paints, or every brush frame snaps the zoom out to fit.
    view?.apply();
    // A stamp is anchored to a panel element the redraw above destroyed, so it
    // has to find the new one. Only when there is no view to do it for us:
    // `apply` notifies its watchers, and re-anchoring twice a frame during a
    // drag is work nobody sees.
    if (!view) anchor();
    if (onSelect) onSelect();
    return true;
  }
  // Assigned once the stamps and the panel reader below exist. `draw` runs one
  // frame before that, and nothing is anchored yet on the first frame.
  let anchor = () => {};
  if (!draw()) return { destroy() {}, reset() {}, opened };

  // Every panel on the page, in document order, with its two domains parsed.
  //
  // Each one is measured against **its own** transform rather than the outer
  // `<svg>`'s. A composed page nests one `<svg>` per cell, so a cell's panel
  // rectangle is written in that cell's user space; reading it against the outer
  // element would put every panel but the first in the wrong place. Asking the
  // element itself for its screen transform makes the nesting cost nothing.
  const panels = () =>
    Array.from(container.querySelectorAll("[data-gog-panel]")).map((g) => {
      const [x0, y0, x1, y1] = g.getAttribute("data-gog-panel").split(" ").map(Number);
      const axis = (name, lo, hi) => {
        const span = g.getAttribute(`data-${name}`);
        if (!span) return null;
        const [from, to] = span.split(" ").map(Number);
        const cats = g.getAttribute(`data-${name}-cats`);
        const log = g.getAttribute(`data-${name}-log`);
        return {
          field: g.getAttribute(`data-${name}-field`),
          from, to, lo, hi,
          log: log === null ? null : Number(log),
          cats: cats === null ? null : cats.split("|"),
        };
      };
      // Which slice of the table this panel drew, and which moment of it. Both
      // are absent on a plot that has neither, and the readout treats absence as
      // "no filter" rather than guessing.
      const slice = (side) => {
        const field = g.getAttribute(`data-facet-${side}-field`);
        const level = g.getAttribute(`data-facet-${side}`);
        return field === null || level === null ? null : { field, level };
      };
      const playField = g.getAttribute("data-play-field");
      const play = playField === null ? null : {
        field: playField,
        levels: (g.getAttribute("data-play-levels") ?? "").split("|"),
        seconds: Number(g.getAttribute("data-play-seconds")),
      };
      // y runs down the page and up the axis, so its two ends are swapped
      // against x's. That is the one asymmetry here.
      return {
        el: g, x0, y0, x1, y1, x: axis("x", x0, x1), y: axis("y", y1, y0),
        facets: [slice("col"), slice("row")].filter(Boolean),
        play,
        // A panel that does not say reads as one that cannot, which is what an
        // old engine under a new module looks like. Going quiet is the state a
        // reader had before any of this, and it is the safe way to be wrong.
        place: g.getAttribute("data-gog-place"),
      };
    });

  // Where the pointer is, in this panel's own user space.
  const pointIn = (panel, event) => {
    const owner = panel.el.ownerSVGElement;
    if (!owner || typeof panel.el.getScreenCTM !== "function") return null;
    const ctm = panel.el.getScreenCTM();
    if (!ctm) return null;
    const pt = owner.createSVGPoint();
    pt.x = event.clientX;
    pt.y = event.clientY;
    return pt.matrixTransform(ctm.inverse());
  };

  const holds = (panel, at) =>
    at !== null && at.x >= panel.x0 && at.x <= panel.x1 &&
    at.y >= panel.y0 && at.y <= panel.y1;

  // ---------------------------------------------------------------------
  // The band under the pointer
  //
  // A selection is invisible while it is being drawn: the dimming only lands
  // on the next frame, and on an axis the sentence did not bind, nothing lands
  // at all. So the gesture draws itself.
  //
  // **It shows exactly what is bound, and nothing more.** One brush on `gdp` is
  // a vertical band, because that is what was selected; a brush on each
  // position is a rectangle. Drawing a rectangle for a one-column brush is the
  // obvious thing every other tool does and it would be a lie — it would show
  // a `life` range nobody asked for and nothing would be dimmed by it.
  //
  // It lives on `document.body`, positioned in viewport coordinates, and **not**
  // inside the container. That is not a detail: a redraw does
  // `container.innerHTML = svg`, so anything parented to the container is
  // destroyed on every animation frame. The first build of this put the band in
  // the container and it was invisible for exactly that reason — created, then
  // wiped a few milliseconds later, sixty times a second. `addControls` already
  // knew this and hangs its readout off `container.after`; this is the same
  // lesson learned twice.
  // ---------------------------------------------------------------------
  let band = null;

  const boundAxes = (panel) => {
    const fields = new Set();
    let bare = false;
    for (const plot of eachPlot(req.spec)) {
      for (const entry of plot.brush ?? []) {
        if (entry.field) fields.add(entry.field);
        else bare = true;
      }
    }
    // A bare brush has not chosen its axes yet, so both are in play and the
    // band is a rectangle from the first pixel of the drag.
    if (bare) return { x: panel.x, y: panel.y };
    return {
      x: panel.x && fields.has(panel.x.field) ? panel.x : null,
      y: panel.y && fields.has(panel.y.field) ? panel.y : null,
    };
  };

  const showBand = (panel, a, b) => {
    const owner = panel.el.ownerSVGElement;
    const ctm = panel.el.getScreenCTM();
    if (!owner || !ctm) return;
    const bound = boundAxes(panel);
    // An unbound axis spans the panel, which is the honest picture: the
    // selection does not narrow it.
    const lo = {
      x: bound.x ? Math.min(a.x, b.x) : panel.x0,
      y: bound.y ? Math.min(a.y, b.y) : panel.y0,
    };
    const hi = {
      x: bound.x ? Math.max(a.x, b.x) : panel.x1,
      y: bound.y ? Math.max(a.y, b.y) : panel.y1,
    };
    const corner = (x, y) => {
      const p = owner.createSVGPoint();
      p.x = x;
      p.y = y;
      return p.matrixTransform(ctm);
    };
    // Clamped to the panel, which is both the honest picture — a selection
    // cannot reach data that is not there — and what stops a drag flung past
    // the edge from putting a fixed element outside the viewport.
    const clamp = (v, a, b) => Math.min(Math.max(v, a), b);
    const tl = corner(clamp(lo.x, panel.x0, panel.x1), clamp(lo.y, panel.y0, panel.y1));
    const br = corner(clamp(hi.x, panel.x0, panel.x1), clamp(hi.y, panel.y0, panel.y1));
    if (!band) {
      band = document.createElement("div");
      band.className = "gog-selection";
      // `fixed` takes the viewport coordinates `getScreenCTM` already hands
      // back, so there is no container offset to get wrong, and it survives the
      // redraw that would have destroyed a child of the container.
      band.style.cssText =
        "position:fixed;pointer-events:none;border:1.5px dotted #333;" +
        "background:rgba(51,51,51,0.08);z-index:2147483647;";
    }
    if (!band.isConnected) document.body.appendChild(band);
    band.style.left = `${tl.x}px`;
    band.style.top = `${tl.y}px`;
    band.style.width = `${Math.max(0, br.x - tl.x)}px`;
    band.style.height = `${Math.max(0, br.y - tl.y)}px`;
  };

  const hideBand = () => {
    band?.remove();
    band = null;
  };

  // ---------------------------------------------------------------------
  // The traced outline
  //
  // A rectangle is what a *sentence* can say, so it is what `brush` says. Some
  // groups are not rectangles, and a reader who can see one wants to draw around
  // it. That act adds no word: the sentence still says `brush`, nothing new is
  // typed, and the printed page still shows the bound the author named.
  //
  // The outline is collected in the columns' own units, never in pixels, which
  // is what lets the engine test it at any size and on any axis. A drawn shape
  // in screen coordinates would be a picture; a shape in data units is a fact
  // about the rows.
  // ---------------------------------------------------------------------
  let outline = null;

  // Two pixels apart is enough to draw a smooth-looking shape and keeps the path
  // short enough to re-render inside a frame. A pointer emits far more samples
  // than a polygon needs.
  const TRACE_STEP = 2;

  const traceable = (panel) =>
    panel && panel.x && panel.y && !panel.x.cats && !panel.y.cats &&
    panel.x.field && panel.y.field;

  const showOutline = (panel, trace) => {
    const owner = panel.el.ownerSVGElement;
    const ctm = panel.el.getScreenCTM();
    if (!owner || !ctm || trace.length < 2) return;
    const seen = trace.map((p) => {
      const q = owner.createSVGPoint();
      q.x = p.x;
      q.y = p.y;
      return q.matrixTransform(ctm);
    });
    if (!outline) {
      // An `<svg>` over the whole viewport, for the same reason the rectangle
      // band is a fixed `div`: a redraw replaces the container's contents, so
      // anything parented to it is destroyed sixty times a second.
      outline = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      outline.setAttribute("class", "gog-lasso");
      outline.style.cssText =
        "position:fixed;left:0;top:0;width:100vw;height:100vh;" +
        "pointer-events:none;z-index:2147483647;";
      const shape = document.createElementNS("http://www.w3.org/2000/svg", "polygon");
      shape.setAttribute("fill", "rgba(51,51,51,0.08)");
      shape.setAttribute("stroke", "#333");
      shape.setAttribute("stroke-width", "1.5");
      shape.setAttribute("stroke-dasharray", "3 3");
      outline.appendChild(shape);
    }
    if (!outline.isConnected) document.body.appendChild(outline);
    outline.firstChild.setAttribute(
      "points", seen.map((p) => `${p.x},${p.y}`).join(" "));
  };

  const hideOutline = () => {
    outline?.remove();
    outline = null;
  };

  // The shape reaches every plot that declared a brush, which is the rule the
  // bound already follows: one gesture, every plot on the page that is listening.
  // A cell whose table does not carry these two columns is left alone by the
  // engine rather than emptied, exactly as a bound on a column it never heard of
  // leaves it alone.
  const applyRegion = (panel, trace) => {
    if (!traceable(panel)) return false;
    const path = trace
      .map((p) => [valueOn(panel.x, p.x), valueOn(panel.y, p.y)])
      .filter(([px, py]) => px !== null && py !== null);
    if (path.length < 3) return false;
    for (const plot of eachPlot(req.spec)) {
      if (!(plot.brush ?? []).length) continue;
      // **Tracing replaces the bound on the axes it covers**, because it is the
      // same drag doing the same job: in a rectangle the drag writes `at` on
      // both axes, and here it writes a shape over the pair. Leaving both in
      // force would quietly intersect them, and the reader would see fewer rows
      // lit than the shape they drew holds. A bound on some *other* column is a
      // constraint the sentence made and stays.
      for (const entry of plot.brush) {
        if (entry.field === panel.x.field || entry.field === panel.y.field) {
          delete entry.at;
          delete entry.levels;
        }
      }
      plot.region = { x: panel.x.field, y: panel.y.field, path };
    }
    return true;
  };

  const clearRegion = () => {
    let had = false;
    for (const plot of eachPlot(req.spec)) {
      if (plot.region) {
        delete plot.region;
        had = true;
      }
    }
    return had;
  };

  // ---------------------------------------------------------------------
  // Reading the row under the pointer
  //
  // Reporting what a row is does not change what the picture claims about the
  // data, so this is the medium's and needs no atom — and it is **not** the
  // blocked `click`, because reading a row is not selecting one by identity.
  //
  // The obvious way to do this is hit-testing the DOM, which would need every
  // mark to carry its row number, which is exactly what this feature refused to
  // add. It does not need to: the browser has the data and it has the panel's
  // two domains, so it can place every row itself and keep the nearest. That is
  // `placeOn`, which is `boundOn` run forwards. No engine change, nothing added
  // to the SVG, and it works on a log axis and a category axis for free.
  //
  // **What the arithmetic assumes, and who checks it.** Re-deriving a position
  // is exact only where the mark stands at the row's own value. `jitter` moves
  // it, `dodge` and `stack` move it, a summary draws one shape for many rows, a
  // disc bends both axes, and a map projects its columns before anything is
  // fitted. The engine says which of those happened, on the panel, because it is
  // the only side that knows — see `data-gog-place`. Reading a value back
  // against a picture that moved it is not a near miss: the reader gets a
  // plausible row at a plausible position and no way to tell.
  //
  // Two more filters answer the same question about *which* rows this panel
  // drew. A faceted plot is one plot over one table, and a played plot holds
  // every moment in the document at once, so without them the table on the page
  // is larger than the picture in front of the reader.
  // ---------------------------------------------------------------------
  let tip = null;
  // Why the last refused panel a reader pointed at could not answer, or `null`
  // while nobody has asked. The bar reads it.
  let unplaced = null;

  // The moment showing now. Every frame is in the document and the clock chooses
  // between them: frame `i` is displayed over `[i*s, i*s + s)` and the sequence
  // repeats, so the index is the elapsed time divided by one frame's length.
  // Read off the same `<svg>` a redraw reads and writes, so there is one
  // timeline even on a page of nested cells.
  const moment = (panel) => {
    if (!panel.play || !(panel.play.seconds > 0)) return null;
    const svg = container.querySelector("svg");
    const t = typeof svg?.getCurrentTime === "function" ? svg.getCurrentTime() : 0;
    const n = panel.play.levels.length;
    return panel.play.levels[((Math.floor(t / panel.play.seconds) % n) + n) % n];
  };

  const nearest = (panel, at) => {
    // The engine could not promise a position, so there is no honest answer to
    // give. The bar says why the first time a reader asks.
    if (panel.place !== "row") return null;
    const now = moment(panel);
    let best = null;
    for (const plot of eachPlot(req.spec)) {
      const df = req.data?.[plot.data];
      if (!df || !panel.x || !panel.y) continue;
      const floats = df.floats ?? {};
      const strings = df.strings ?? {};
      const get = (f, i) => (floats[f] ? floats[f][i] : strings[f]?.[i]);
      const n = floats[panel.x.field]?.length ?? strings[panel.x.field]?.length ?? 0;
      const named = [];
      const add = (f) => { if (f && !named.includes(f) && (floats[f] || strings[f])) named.push(f); };
      for (const c of [plot.x, plot.y]) add(c?.field);
      for (const c of Object.values(plot.channels ?? {})) add(c?.field);
      for (const layer of plot.layers ?? []) {
        for (const c of Object.values(layer.encodings ?? {})) add(c?.field);
      }
      // Every column a position or a channel reads. The engine drops a row with
      // a gap in any of them before it draws, so the browser has to drop it too
      // or it names a row that is not on the page. The facet columns join the
      // list because they decide which panel a row is in.
      const mapped = [...named, plot.z?.field, ...panel.facets.map((f) => f.field)]
        .filter((f) => f && (floats[f] || strings[f]));
      // Is this row one of the ones this panel drew? Compared as strings,
      // because a level arrives off an attribute and a moment key is written
      // the way the column prints it.
      const drew = (i) =>
        panel.facets.every((f) => String(get(f.field, i)) === f.level) &&
        (now === null || String(get(panel.play.field, i)) === now) &&
        mapped.every((f) => get(f, i) !== null && get(f, i) !== undefined);
      for (let i = 0; i < n; i++) {
        if (!drew(i)) continue;
        const px = placeOn(panel.x, get(panel.x.field, i));
        const py = placeOn(panel.y, get(panel.y.field, i));
        if (px === null || py === null) continue;
        const d = (px - at.x) ** 2 + (py - at.y) ** 2;
        if (best === null || d < best.d) {
          best = { d, px, py, row: named.map((f) => [f, get(f, i)]) };
        }
      }
    }
    // Within about a glyph's reach, or the reader is not pointing at anything.
    return best && best.d <= 14 * 14 ? best : null;
  };

  const showTip = (panel, hit) => {
    const owner = panel.el.ownerSVGElement;
    const ctm = panel.el.getScreenCTM();
    if (!owner || !ctm) return hideTip();
    const p = owner.createSVGPoint();
    p.x = hit.px;
    p.y = hit.py;
    const at = p.matrixTransform(ctm);
    if (!tip) {
      tip = document.createElement("div");
      tip.className = "gog-tip";
      tip.style.cssText =
        "position:fixed;pointer-events:none;z-index:2147483647;" +
        "font:12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;" +
        // **These colors are fixed on purpose, and they are the exception.**
        // Every control on the page inherits its color now, so it stays legible
        // whether the host is a light page or a dark one. A tooltip does not,
        // because it is not on the page: it floats over the plot, and a plot is
        // drawn light whatever the host looks like. It also carries its own
        // opaque background, so it is a small light card in both cases rather
        // than text that has to survive an unknown backdrop.
        //
        // The brush band above is fixed for the same reason. It is drawn over
        // the plot, never over the page.
        "background:rgba(255,255,255,0.96);border:1px solid #ccc;border-radius:3px;" +
        "padding:.25em .5em;color:#333;box-shadow:0 1px 4px rgba(0,0,0,.12);white-space:nowrap;";
      document.body.appendChild(tip);
    }
    if (!tip.isConnected) document.body.appendChild(tip);
    tip.innerHTML = rowHtml(hit.row);
    // Kept inside the viewport, so a point near the right edge does not push a
    // fixed element off the page and give the reader a scrollbar.
    const w = tip.offsetWidth;
    tip.style.left = `${Math.min(at.x + 12, window.innerWidth - w - 8)}px`;
    tip.style.top = `${at.y + 12}px`;
  };

  const hideTip = () => {
    tip?.remove();
    tip = null;
  };

  // Shorter than this and the reader did not draw a range, they clicked. The
  // same number decides the same question for a stamp: shorter than this and
  // they did not carry the card anywhere, they clicked it. One threshold rather
  // than two, so a hand that is steady enough for one gesture is steady enough
  // for the other.
  const MIN_DRAG = 3;

  // ---------------------------------------------------------------------
  // Stamping a row
  //
  // A stamp is the readout above, kept. Hovering answers *what is this* and
  // forgets; a reader comparing four points has to hold three of them in their
  // head, and a reader deciding which points are worth a `text` layer has no way
  // to try one out. So a click leaves the answer on the picture and a click on
  // the card takes it off again.
  //
  // Still the medium's, by the same test as the readout it keeps: what the plot
  // claims about the data does not change, and the printed page shows the
  // sentence's own plot with no stamps on it. What a stamp *is* for is finding
  // the labels worth writing down, and writing them down is `text`.
  //
  // **A stamp holds a row and a place, never an element.** Every redraw replaces
  // the whole picture, so a card parented into it would be destroyed sixty times
  // a second during a drag. These live on `document.body` like the tooltip, and
  // are re-projected whenever the picture moves under them.
  //
  // **The card is the reader's to place.** Four stamps on a crowded scatter put
  // four cards over the data they were made to read, and the reader is the only
  // one who knows which corner is empty. So a card can be carried anywhere and
  // the line to its point stretches after it, growing a head once it is long
  // enough that a reader could lose track of which end it means.
  //
  // That offset is held in **screen pixels, never in the data's units**. A card
  // is a fixed-size piece of furniture, so it should sit the same distance from
  // its point at every magnification; measured in the data it would be flung off
  // the panel by the first zoom.
  // ---------------------------------------------------------------------
  const pins = [];

  // **Escaped, because a value comes out of the reader's own table.** A column
  // holding `a < b`, or a country name with an ampersand in it, was being pasted
  // into markup, so the contents of a cell decided what the card was made of.
  // Nothing malicious is needed for that to go wrong, only a `<`.
  const escapeText = (s) =>
    String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

  const rowHtml = (row) => row
    .map(([f, v]) => `<div><span style="color:#888">${escapeText(f)}</span> ` +
                     `${escapeText(v)}</div>`)
    .join("");

  const CARD =
    "font:12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;" +
    // Fixed colors, for the reason the tooltip's are fixed: this is over the
    // plot, and a plot is drawn light whatever the page around it looks like.
    "background:rgba(255,255,255,0.92);border:1px solid #bbb;border-radius:3px;" +
    "padding:.2em .45em;color:#333;box-shadow:0 1px 4px rgba(0,0,0,.12);white-space:nowrap;";

  const SVGNS = "http://www.w3.org/2000/svg";
  // Where a card sits before anyone moves it: this far above its point, which is
  // near enough to belong to it and clear enough to leave it visible.
  const LEADER = 18;
  // How long the line has to be before it grows a head. At rest the card all but
  // touches its dot, and an arrowhead there is a second mark doing the dot's job.
  // Carried across the panel, the line has to say which end it means.
  const ARROW = 24;
  // Clear of the dot, so the line neither hides it nor starts inside it.
  const CLEAR = 5;
  const HEAD = 8;

  // The card, and the line that ties it to its point.
  //
  // Called when a stamp is made and on every frame of a card drag, and never by
  // `placePins`: the offset is measured from the anchor, and the anchor is the
  // thing that moves when the picture does. So zoom, pan, redraw, scroll and
  // resize all carry a placed card with them for free.
  const placeCard = (pin) => {
    pin.card.style.left = `${pin.dx}px`;
    pin.card.style.top = `${pin.dy}px`;
    const w = pin.card.offsetWidth;
    const h = pin.card.offsetHeight;
    // Hidden, so the browser has no size to give and there is nothing to draw
    // between. It will be laid out again when its point comes back.
    if (!w && !h) return;
    // A card is placed by the midpoint of its bottom edge, so its center is half
    // a card higher. The line stops where it meets the border rather than running
    // in under the text: `s` is how far along the way to the point that border
    // is, taken on whichever side the line leaves by.
    const cx = pin.dx;
    const cy = pin.dy - h / 2;
    const s = Math.min(
      1,
      Math.abs(cx) > 0.01 ? w / 2 / Math.abs(cx) : Infinity,
      Math.abs(cy) > 0.01 ? h / 2 / Math.abs(cy) : Infinity
    );
    const ex = cx * (1 - s);
    const ey = cy * (1 - s);
    const far = Math.hypot(ex, ey);
    // Dropped on its own point: there is no gap left to draw a line across, and
    // a one-pixel arrow reads as dirt on the screen.
    if (far < 2) {
      pin.wire.setAttribute("width", "0");
      pin.wire.setAttribute("height", "0");
      pin.line.setAttribute("visibility", "hidden");
      pin.head.setAttribute("visibility", "hidden");
      return;
    }
    const ux = ex / far;
    const uy = ey / far;
    const near = Math.min(CLEAR, far);
    const arrow = far > ARROW;
    const bx = ux * Math.min(near + HEAD, far);
    const by = uy * Math.min(near + HEAD, far);
    const [x1, y1] = arrow ? [bx, by] : [ux * near, uy * near];
    pin.line.setAttribute("x1", x1);
    pin.line.setAttribute("y1", y1);
    pin.line.setAttribute("x2", ex);
    pin.line.setAttribute("y2", ey);
    pin.line.setAttribute("visibility", "visible");
    // Apex at the near end, base two half-widths across the line's own
    // perpendicular, which is `(-uy, ux)`.
    const corners = [
      [ux * near, uy * near],
      [bx - uy * 3.5, by + ux * 3.5],
      [bx + uy * 3.5, by - ux * 3.5],
    ];
    pin.head.setAttribute("points", corners.map(([x, y]) => `${x},${y}`).join(" "));
    pin.head.setAttribute("visibility", arrow ? "visible" : "hidden");
    // **The `<svg>` is sized to what it draws, and never left at nothing.**
    // Anchoring it at the point and letting `overflow` carry the rest is what
    // this did first, and it laid the line out in exactly the right place and
    // then did not paint it: a zero-sized outermost `<svg>` clips its contents
    // in Chrome whatever `overflow` says, so `getBoundingClientRect` reported a
    // line that was not on the screen. A `viewBox` matching the box keeps the
    // coordinates above measured from the point, which is what makes them
    // readable.
    const xs = [x1, ex, ...(arrow ? corners.map((c) => c[0]) : [])];
    const ys = [y1, ey, ...(arrow ? corners.map((c) => c[1]) : [])];
    const pad = 2;
    const minX = Math.min(...xs) - pad;
    const minY = Math.min(...ys) - pad;
    const boxW = Math.max(...xs) - minX + pad;
    const boxH = Math.max(...ys) - minY + pad;
    pin.wire.style.left = `${minX}px`;
    pin.wire.style.top = `${minY}px`;
    pin.wire.setAttribute("width", boxW);
    pin.wire.setAttribute("height", boxH);
    pin.wire.setAttribute("viewBox", `${minX} ${minY} ${boxW} ${boxH}`);
  };

  const placePins = () => {
    const all = panels();
    for (const pin of pins) {
      const panel = all[pin.panel];
      const owner = panel?.el?.ownerSVGElement;
      const ctm = panel?.el?.getScreenCTM?.();
      if (!owner || !ctm) {
        pin.el.style.display = "none";
        continue;
      }
      const p = owner.createSVGPoint();
      p.x = pin.px;
      p.y = pin.py;
      const at = p.matrixTransform(ctm);
      // Zooming in can carry a stamped point outside the window. A card left
      // floating over an unrelated part of the picture is worse than one that
      // waits for its point to come back.
      const box = view?.svg()?.getBoundingClientRect?.();
      const gone = box && (at.x < box.left || at.x > box.right ||
                           at.y < box.top || at.y > box.bottom);
      // **A stamp belongs to the frame it was made in**, and waits out the rest
      // for the same reason. A row on a played plot is one country in one year,
      // so the row a stamp names is simply not on the screen in the other
      // frames; the dot would sit where that row used to be, several hundred
      // pixels from anything, claiming to point at it. Each turn of the loop
      // brings the stamp back on its own point.
      //
      // Following the row instead was the other way to answer this, and it is
      // not available: 1972's Korea and 2007's Korea are two rows, and pairing
      // them needs a notion of identity the grammar does not have.
      const elsewhere = pin.moment !== null && pin.moment !== moment(panel);
      pin.el.style.display = gone || elsewhere ? "none" : "block";
      pin.el.style.left = `${at.x}px`;
      pin.el.style.top = `${at.y}px`;
    }
  };

  // ---------------------------------------------------------------------
  // Writing the stamps into the picture the camera saves
  //
  // A reader who arranges four cards and presses the camera should get those
  // four cards. They are HTML on `document.body`, so the copy the camera
  // serializes has never held them, and that gap is sharper now that a card can
  // be *placed*: an arrangement reads as annotation, and an annotation that
  // vanishes when you save is worse than none.
  //
  // **Nothing here predicts a layout, it reads one.** The stamps are on the page
  // at the moment the button is pressed, so the browser has already decided
  // where every word sits, and `getBoundingClientRect` will say. Run those
  // screen boxes back through the inverse of the picture's own matrix and they
  // are user coordinates. The alternative, measuring monospace text in
  // JavaScript and hoping the raster agrees, is the version of this that comes
  // out slightly wrong.
  //
  // Two things do not survive. `box-shadow` has no cheap equal here and is
  // dropped, which the border covers. And a very long value can sit a pixel past
  // its border if the raster's font metrics differ from the page's, which shows
  // as slack rather than as clipping, because nothing is clipped.
  // ---------------------------------------------------------------------
  const svgTag = (name, attrs, inner = "") => {
    const doc = document.createElementNS(SVGNS, name);
    for (const [k, v] of Object.entries(attrs)) doc.setAttribute(k, String(v));
    if (inner) doc.innerHTML = inner;
    return doc;
  };

  const drawPins = (clone) => {
    if (!pins.length) return;
    const live = view?.svg?.();
    const ctm = live?.getScreenCTM?.();
    if (!live || !ctm || typeof ctm.inverse !== "function") return;
    const inv = ctm.inverse();
    const toUser = (x, y) => {
      const p = live.createSVGPoint();
      p.x = x;
      p.y = y;
      return p.matrixTransform(inv);
    };
    // One screen pixel, in the picture's units. Every size below is quoted in
    // screen pixels because that is what the reader chose, so each is divided
    // through by this.
    const px = 1 / (ctm.a || 1);
    const group = svgTag("g", { class: "gog-stamp-ink" });

    for (const pin of pins) {
      // Hidden is hidden. A stamp waiting out a frame it does not belong to, or
      // one a zoom has carried off the panel, is not on the picture and must not
      // be in the file either.
      if (pin.el.style.display === "none") continue;
      const ax = parseFloat(pin.el.style.left);
      const ay = parseFloat(pin.el.style.top);
      if (!Number.isFinite(ax) || !Number.isFinite(ay)) continue;
      const at = (x, y) => toUser(ax + x, ay + y);

      // The leader, in the coordinates `placeCard` already worked out.
      if (pin.line.getAttribute("visibility") !== "hidden") {
        const a = at(Number(pin.line.getAttribute("x1")), Number(pin.line.getAttribute("y1")));
        const b = at(Number(pin.line.getAttribute("x2")), Number(pin.line.getAttribute("y2")));
        group.appendChild(svgTag("line", {
          x1: a.x, y1: a.y, x2: b.x, y2: b.y, stroke: "#999", "stroke-width": px,
        }));
      }
      if (pin.head.getAttribute("visibility") === "visible") {
        const points = (pin.head.getAttribute("points") ?? "").split(" ")
          .map((pair) => pair.split(",").map(Number))
          .map(([x, y]) => at(x, y))
          .map((u) => `${u.x},${u.y}`)
          .join(" ");
        group.appendChild(svgTag("polygon", { points, fill: "#999" }));
      }

      const box = pin.card.getBoundingClientRect();
      const tl = toUser(box.left, box.top);
      const br = toUser(box.right, box.bottom);
      group.appendChild(svgTag("rect", {
        x: tl.x, y: tl.y, width: br.x - tl.x, height: br.y - tl.y, rx: 3 * px,
        fill: "rgba(255,255,255,0.92)", stroke: "#bbb", "stroke-width": px,
      }));

      // One line of text per row, placed on the row the browser laid out.
      // `central` puts the em box on the middle of that line, which is what
      // `line-height` did on the page, and it needs no font metrics.
      const rows = pin.card.children[0]?.children ?? [];
      pin.row.forEach(([field, value], i) => {
        const r = rows[i]?.getBoundingClientRect?.();
        if (!r) return;
        const p = toUser(r.left, (r.top + r.bottom) / 2);
        group.appendChild(svgTag("text", {
          x: p.x, y: p.y, "dominant-baseline": "central", "xml:space": "preserve",
          "font-family": "ui-monospace, SFMono-Regular, Menlo, monospace",
          "font-size": 12 * px, fill: "#333",
        }, `<tspan fill="#888">${escapeText(field)}</tspan>` +
           `<tspan> ${escapeText(value)}</tspan>`));
      });

      const shut = pin.card.children[1]?.getBoundingClientRect?.();
      if (shut) {
        const p = toUser(shut.left, (shut.top + shut.bottom) / 2);
        group.appendChild(svgTag("text", {
          x: p.x, y: p.y, "dominant-baseline": "central",
          "font-family": "ui-monospace, SFMono-Regular, Menlo, monospace",
          "font-size": 13 * px, fill: "#aaa",
        }, "×"));
      }

      // The dot last, so it sits over the line that runs up to it.
      const a = at(0, 0);
      group.appendChild(svgTag("circle", {
        cx: a.x, cy: a.y, r: 2.75 * px, fill: "#333",
        stroke: "#fff", "stroke-width": 1.5 * px,
      }));
    }

    if (group.children.length) clone.appendChild(group);
  };

  const unpen = view?.onSave?.(drawPins);

  // Nothing above ever runs while a plot plays. `play` swaps its frames with
  // SMIL, inside the `<svg>` and without a redraw, so every path that re-places
  // a stamp — the view, a drag, a scroll — stays quiet through the whole
  // sequence. A stamp that belongs to a frame therefore needs someone watching
  // the clock, and this is the only thing on the page that does.
  //
  // It runs for exactly as long as there is such a stamp, which on most plots is
  // never. An animation frame rather than a timer, because a background tab
  // stops being sent them, and a plot nobody is looking at should cost nothing.
  let ticking = 0;
  const tick = () => {
    ticking = 0;
    if (!pins.some((pin) => pin.moment !== null)) return;
    placePins();
    ticking = requestAnimationFrame(tick);
  };
  const watchFrames = () => {
    if (!ticking && pins.some((pin) => pin.moment !== null)) {
      ticking = requestAnimationFrame(tick);
    }
  };

  const drop = (pin) => {
    const i = pins.indexOf(pin);
    if (i < 0) return;
    pins.splice(i, 1);
    pin.el.remove();
    onSelect?.();
  };

  const stamp = (index, hit) => {
    // One element anchored at the point, with everything else placed against it
    // in fixed CSS. That way re-anchoring writes one pair of numbers per stamp,
    // however many pieces the reader sees, and carrying a card away writes
    // another pair without the anchor being told.
    const el = document.createElement("div");
    el.className = "gog-stamp";
    el.style.cssText =
      "position:fixed;width:0;height:0;z-index:2147483646;pointer-events:none;";
    // The line is drawn rather than laid out. A card the reader has carried
    // across the panel needs a line at an angle, and CSS gives a box an angle
    // only by rotating it, which is two transforms and a second one for the
    // head. `placeCard` sizes and places this to fit whatever it holds; it
    // starts at nothing because there is nothing in it yet.
    const wire = document.createElementNS(SVGNS, "svg");
    wire.setAttribute("class", "gog-stamp-leader");
    wire.setAttribute("width", "0");
    wire.setAttribute("height", "0");
    wire.style.cssText = "position:absolute;left:0;top:0;pointer-events:none;";
    const line = document.createElementNS(SVGNS, "line");
    line.setAttribute("class", "gog-stamp-line");
    line.setAttribute("stroke", "#999");
    line.setAttribute("stroke-width", "1");
    const head = document.createElementNS(SVGNS, "polygon");
    head.setAttribute("class", "gog-stamp-head");
    head.setAttribute("fill", "#999");
    wire.appendChild(line);
    wire.appendChild(head);
    // The dot stays whatever the line does. It is what says *this row*, and the
    // head only says which way to look.
    const dot = document.createElement("span");
    dot.style.cssText =
      "position:absolute;left:-3.5px;top:-3.5px;width:7px;height:7px;box-sizing:border-box;" +
      "border-radius:50%;background:#333;border:1.5px solid #fff;";
    // The card stands off the point rather than on it, joined by that line. A
    // card centered on a four-pixel dot hides the dot and its neighbors, which is
    // the crowd the reader stamped it to read.
    const card = document.createElement("div");
    card.className = "gog-stamp-card";
    card.title = "drag to move this stamp, click to take it off";
    card.style.cssText =
      `position:absolute;left:0;top:0;transform:translate(-50%,-100%);${CARD}` +
      "pointer-events:auto;cursor:grab;display:flex;align-items:flex-start;gap:.45em;" +
      // A drag over text selects it, and a drag on a touch screen scrolls the
      // page. Both would happen instead of the card moving.
      "user-select:none;-webkit-user-select:none;touch-action:none;";
    const body = document.createElement("div");
    body.innerHTML = rowHtml(hit.row);
    // Three ways to undo one thing, for three different reads of the plot. The
    // cross is the one that needs no explaining and cannot be mistaken for
    // anything else, which matters now that the card's own face is a handle.
    const shut = document.createElement("span");
    shut.className = "gog-stamp-close";
    shut.textContent = "×";
    shut.style.cssText =
      "flex:none;cursor:pointer;color:#aaa;font-size:13px;line-height:1.15;";
    // The one label-less control that is not in a bar. It says the smaller of
    // the two things a card can do, and the card's face says the other.
    hoverLabel(shut, "take this stamp off");
    shut.addEventListener("pointerenter", () => { shut.style.color = "#333"; });
    shut.addEventListener("pointerleave", () => { shut.style.color = "#aaa"; });
    card.appendChild(body);
    card.appendChild(shut);
    el.appendChild(wire);
    el.appendChild(dot);
    el.appendChild(card);
    const pin = {
      panel: index, px: hit.px, py: hit.py, row: hit.row,
      // Which frame this row belongs to, or `null` on a plot that does not
      // play. Read once, here, because it is a fact about the row.
      moment: moment(panels()[index]),
      el, card, wire, line, head, dx: 0, dy: -LEADER,
    };
    shut.addEventListener("click", () => drop(pin));

    // Carrying the card, and the click that is not a carry.
    //
    // `moved` latches on the way rather than being measured at the end, for the
    // reason the panel's does: a pointer that wanders out and comes back would
    // read as a click if only its two ends were compared, and taking a stamp off
    // a reader who was placing it is the mistake this must not make.
    //
    // Nothing here can reach the panel underneath. Stamps hang off
    // `document.body` rather than off the plot, so a drag on a card is never
    // also a drag on the picture, and no guard is needed to keep the two apart.
    let grab = null;
    card.addEventListener("pointerdown", (e) => {
      if (e.target === shut) return;
      e.preventDefault();
      grab = { x: e.clientX, y: e.clientY, dx: pin.dx, dy: pin.dy, moved: false };
      card.style.cursor = "grabbing";
      try {
        card.setPointerCapture?.(e.pointerId);
      } catch {
        /* no active pointer to capture; the drag proceeds without it */
      }
    });
    card.addEventListener("pointermove", (e) => {
      if (!grab) return;
      const mx = e.clientX - grab.x;
      const my = e.clientY - grab.y;
      if (Math.hypot(mx, my) >= MIN_DRAG) grab.moved = true;
      if (!grab.moved) return;
      pin.dx = grab.dx + mx;
      pin.dy = grab.dy + my;
      placeCard(pin);
    });
    const release = (e) => {
      if (!grab) return;
      const carried = grab.moved;
      grab = null;
      card.style.cursor = "grab";
      try {
        card.releasePointerCapture?.(e.pointerId);
      } catch {
        /* the capture is already gone */
      }
      // No `click` listener on the card, deliberately: the browser fires one
      // after every `pointerup`, including the one that ends a drag, so a card
      // that listened for both would vanish the moment it was put down.
      if (!carried) drop(pin);
    };
    card.addEventListener("pointerup", release);
    card.addEventListener("pointercancel", release);

    document.body.appendChild(el);
    pins.push(pin);
    // Measured only once it is in the document, since the card's size is the
    // browser's answer rather than ours.
    placeCard(pin);
    placePins();
    watchFrames();
    onSelect?.();
  };

  const clearStamps = () => {
    while (pins.length) pins.pop().el.remove();
    onSelect?.();
  };

  anchor = placePins;
  // Everything that can move a stamp away from its point. The view covers zoom,
  // pan, fit and the re-application after every redraw, because all four end in
  // one `apply`. The other two are the page itself moving under a fixed element:
  // scrolling is caught on the way down so a plot inside a scrolling panel is
  // caught too, not only the document.
  const unwatch = view?.onApply(placePins);
  window.addEventListener("scroll", placePins, true);
  window.addEventListener("resize", placePins);

  const bound = (axis, a, b) => boundOn(axis, a, b);

  // One drag reaches **every** plot on the page that named the dragged column.
  //
  // This is the whole of linked brushing, and it needed no new grammar and no
  // engine change: the two composed plots were already answering the same
  // predicate, because a bound is a fact about a column rather than about a
  // panel. All that was missing is that a gesture in one cell should write the
  // bound the others are reading. A cell that names a different column is left
  // alone, which is what makes a marginal histogram follow the scatter it shares
  // an axis with and ignore the one it does not.
  const apply = (panel, start, now) => {
    let moved = false;
    for (const plot of eachPlot(req.spec)) {
      // Bare `brush` is a *declaration* that both positions are selectable. The
      // first drag is what turns it into bounds, one per axis the panel places,
      // because only now is there an axis to attach each one to. After that it
      // behaves exactly as if the sentence had named the two columns.
      const bare = (plot.brush ?? []).findIndex((b) => !b.field);
      if (bare >= 0) {
        const named = [panel.x, panel.y]
          .filter((a) => a && a.field)
          .map((a) => ({ field: a.field }));
        if (named.length) plot.brush.splice(bare, 1, ...named);
      }
      for (const entry of plot.brush ?? []) {
        for (const [axis, a, b] of [[panel.x, start.x, now.x], [panel.y, start.y, now.y]]) {
          if (!axis || axis.field !== entry.field) continue;
          delete entry.at;
          delete entry.levels;
          // A click clears the selection rather than selecting a point. Two
          // reasons, and the second is the sharper one: nobody means "select
          // exactly this value", and a zero-width range is refused by the
          // engine — correctly, since written down it would be a typo. A
          // gesture is not a typo, so the browser must not produce one.
          if (Math.abs(a - b) >= MIN_DRAG) Object.assign(entry, bound(axis, a, b));
          moved = true;
        }
      }
    }
    return moved;
  };

  // The panel is held by **index** rather than by element, because every redraw
  // destroys the element it was found on. Document order is stable across a
  // redraw of the same spec, so the index survives what the node does not.
  let held = -1;
  let start = null;
  let queued = false;
  let trace = null;
  // Whether this gesture has ever been a drag. Kept rather than measured at the
  // end, because a pointer that wanders out and comes back would read as a click
  // if only its two ends were compared, and dropping a stamp on a reader who
  // drew a selection is the one mistake this must not make.
  let moved = false;

  let panning = null;

  const schedule = () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      draw();
    });
  };

  const onDown = (e) => {
    // This plot said `brush`, so the plain drag belongs to the selection and
    // panning asks with a modifier. On a plot that said nothing there is a
    // spare drag and `attachPan` takes it instead.
    if (view && (mode() === "pan" || e.shiftKey || e.altKey)) {
      panning = { x: e.clientX, y: e.clientY };
      container.style.cursor = "grabbing";
      try {
        container.setPointerCapture?.(e.pointerId);
      } catch {
        /* no active pointer to capture */
      }
      return;
    }
    const all = panels();
    held = all.findIndex((p) => holds(p, pointIn(p, e)));
    if (held < 0) return;
    start = pointIn(all[held], e);
    moved = false;
    // A free shape is collected from the first sample. On a panel that measures
    // categories there is nothing to trace, so the drag stays a rectangle.
    trace = mode() === "lasso" && traceable(all[held]) ? [start] : null;
    try {
      container.setPointerCapture?.(e.pointerId);
    } catch {
      /* no active pointer to capture; the drag proceeds without it */
    }
  };
  const onMove = (e) => {
    if (held < 0 && !panning) {
      // Not dragging: say what is under the pointer.
      const all = panels();
      const over = all.find((p) => holds(p, pointIn(p, e)));
      // A panel that cannot place a row says so, once, and only after someone
      // has pointed at it. Printing the reason under every such plot would put
      // an apology on pages nobody was asking a question about.
      if (over && over.place !== "row" && unplaced !== over.place) {
        unplaced = over.place;
        onSelect?.();
      }
      const hit = over && mode() === "select" ? nearest(over, pointIn(over, e)) : null;
      if (hit) showTip(over, hit);
      else hideTip();
      return;
    }
    if (panning) {
      view.panBy(e.clientX - panning.x, e.clientY - panning.y);
      panning = { x: e.clientX, y: e.clientY };
      return;
    }
    if (held < 0 || !start) return;
    const panel = panels()[held];
    if (!panel) return;
    const now = pointIn(panel, e);
    // A pointer dragged outside the panel keeps reading against that panel
    // rather than stopping, which is what lets a drag select up to an edge
    // without having to land exactly on it.
    if (!now) return;
    if (Math.hypot(now.x - start.x, now.y - start.y) >= MIN_DRAG) moved = true;
    if (trace) {
      const last = trace[trace.length - 1];
      if (Math.hypot(now.x - last.x, now.y - last.y) < TRACE_STEP) return;
      trace.push(now);
      showOutline(panel, trace);
      if (!applyRegion(panel, trace)) return;
      schedule();
      return;
    }
    showBand(panel, start, now);
    if (!apply(panel, start, now)) return;
    schedule();
  };
  const onUp = (e) => {
    // A click means one of two things, and which one is decided by what it
    // landed on. **On a mark it stamps**, because the reader pointed at a row
    // and asked for it to stay. **On empty space it clears**, which is what a
    // click has always done here and what a click on empty space does
    // everywhere else. The same reach the readout uses tells them apart, so a
    // reader who can see a tooltip can stamp exactly what it names.
    const panel = held >= 0 ? panels()[held] : null;
    const at = panel && e ? pointIn(panel, e) : null;
    if (panel && at && start && !moved &&
        Math.hypot(at.x - start.x, at.y - start.y) < MIN_DRAG) {
      const hit = mode() === "select" ? nearest(panel, at) : null;
      if (hit) {
        stamp(held, hit);
      } else if (apply(panel, start, start)) {
        // Nothing under it, so it is the clearing click. `apply` writes the
        // empty bound; without this a click that never moved reached nothing at
        // all, since every other path into `apply` is a `pointermove`.
        schedule();
      }
    }
    // A click clears the shape, exactly as it clears a bound: too few samples to
    // enclose anything is not a tiny selection, it is a reader asking for none.
    if (trace && trace.length < 3 && clearRegion()) schedule();
    held = -1;
    start = null;
    trace = null;
    moved = false;
    if (panning) container.style.cursor = "grab";
    panning = null;
    hideBand();
    hideOutline();
  };
  const onLeave = () => hideTip();

  container.addEventListener("pointerdown", onDown);
  container.addEventListener("pointermove", onMove);
  container.addEventListener("pointerup", onUp);
  container.addEventListener("pointercancel", onUp);
  container.addEventListener("pointerleave", onLeave);

  return {
    destroy() {
      container.removeEventListener("pointerdown", onDown);
      container.removeEventListener("pointermove", onMove);
      container.removeEventListener("pointerup", onUp);
      container.removeEventListener("pointercancel", onUp);
      container.removeEventListener("pointerleave", onLeave);
      window.removeEventListener("scroll", placePins, true);
      window.removeEventListener("resize", placePins);
      unwatch?.();
      unpen?.();
      // The clock watcher stops itself once the last stamp goes, but a plot
      // destroyed mid-sequence would leave one frame already asked for.
      if (ticking) cancelAnimationFrame?.(ticking);
      ticking = 0;
      hideBand();
      hideOutline();
      hideTip();
      clearStamps();
    },
    opened,
    /** What the reader has caught: a count, and one page of the rows to read. */
    selection: (offset = 0) => selectedRows(req, PAGE_ROWS, offset),
    /** Why pointing at this plot cannot name a row, once someone has tried it.
     *  `null` until then, and on every plot that can answer. */
    unplaced: () => unplaced,
    /** How many rows the reader has left on the picture. */
    stamps: () => pins.length,
    clearStamps,
    /** What a plain drag does now. Zooming in switches it, because a reader who
     *  has just magnified something almost always wants to move around in it. */
    mode,
    /** The last mode that *selects*, so returning from a pan comes back to the
     *  one the reader chose rather than always to the rectangle. */
    picked: () => picked,
    setMode(next) {
      if (next === "pan" && view) dragMode = "pan";
      else dragMode = next === "lasso" ? "lasso" : "select";
      if (dragMode !== "pan") picked = dragMode;
      // The pointer says what the drag will do before the reader tries it. An
      // open hand is the universal "you can move this"; a crosshair is the
      // universal "you can draw here", for a rectangle and for a free shape
      // alike — which is why the toolbar shows which of the two is on.
      container.style.cursor = dragMode === "pan" ? "grab" : "crosshair";
    },
    reset() {
      // Back to what the sentence said, which for a bare brush is the
      // declaration rather than whatever the last drag turned it into. A traced
      // shape has no resting form to go back to — no sentence can state one — so
      // it simply goes.
      clearRegion();
      eachPlot(req.spec).forEach((p, i) => {
        if (opened[i]) p.brush = JSON.parse(JSON.stringify(opened[i]));
      });
      draw();
    },
  };
}





/**
 * Which rows a selection caught, and the values a reader would want to read.
 *
 * The point of selecting is to **extract** — to isolate a group visually and
 * then find out what is in it. Dimming does the first half; without this the
 * second half is missing, and the reader can see a group they cannot name.
 *
 * The predicate here is deliberately the same four lines the engine runs in
 * `legality::brush_keeps`, and two implementations of one rule is a drift
 * surface. It is allowed exactly one way: the count this returns is checked
 * against the marks the engine actually drew at full strength, by a test, so
 * the two cannot disagree quietly.
 *
 * Columns are the ones the sentence *maps*, not every column in the table. A
 * twelve-column CSV is unreadable as a readout, and the mapped ones are the
 * ones the reader is already looking at.
 *
 * **`offset` is a window into the selection, not a second selection.** A reader
 * who catches forty rows wants all forty, and a table forty rows long would push
 * the plot off the screen — so the rows arrive a page at a time and the caller
 * turns pages. `kept` is always the whole count whatever the window shows, which
 * is what keeps the readout above the table honest.
 */
export const PAGE_ROWS = 10;

export function selectedRows(req, limit = PAGE_ROWS, offset = 0) {
  const result = { kept: 0, total: 0, columns: [], rows: [], capped: false,
                   from: 0, to: 0 };
  for (const plot of eachPlot(req.spec)) {
    const bounds = (plot.brush ?? []).filter((b) => b.at || b.levels);
    // A traced outline is the other way a reader states the same predicate, and
    // it counts the same way. Fewer than three vertices enclose nothing.
    const region = plot.region?.path?.length >= 3 ? plot.region : null;
    if (!bounds.length && !region) continue;
    const df = req.data?.[plot.data];
    if (!df) continue;
    const floats = df.floats ?? {};
    const strings = df.strings ?? {};
    const value = (field, i) =>
      floats[field] ? floats[field][i] : strings[field]?.[i];
    const rows = Object.values(floats)[0]?.length ?? Object.values(strings)[0]?.length ?? 0;

    // The columns the sentence names, in the order it names them, without
    // repeating one bound twice.
    const named = [];
    const add = (f) => { if (f && !named.includes(f)) named.push(f); };
    for (const c of [plot.x, plot.y, plot.z]) add(c?.field);
    for (const c of Object.values(plot.channels ?? {})) add(c?.field);
    for (const layer of plot.layers ?? []) {
      for (const c of Object.values(layer.encodings ?? {})) add(c?.field);
    }
    for (const b of bounds) add(b.field);
    if (region) {
      add(region.x);
      add(region.y);
    }
    const columns = named.filter((f) => floats[f] || strings[f]);

    for (let i = 0; i < rows; i++) {
      const inside = bounds.every((b) => {
        const v = value(b.field, i);
        if (b.at) return typeof v === "number" && Number.isFinite(v) && v >= b.at[0] && v <= b.at[1];
        return b.levels.includes(v);
      }) && (!region || holdsIn(region.path, value(region.x, i), value(region.y, i)));
      result.total++;
      if (!inside) continue;
      // Where this row sits in the whole selection, which is what the window is
      // cut from. Counting every kept row rather than only the shown ones is
      // what lets a page be asked for by number.
      const place = result.kept;
      result.kept++;
      if (place >= offset && place < offset + limit) {
        result.rows.push(columns.map((f) => value(f, i)));
      }
    }
    if (!result.columns.length) result.columns = columns;
  }
  result.capped = result.kept > limit;
  result.from = result.rows.length ? offset + 1 : 0;
  result.to = offset + result.rows.length;
  return result;
}



/**
 * The bar under a brushed plot: how many rows were caught, the rows themselves
 * on demand, and a way back to nothing selected.
 *
 * The twin of `addControls`, and inserted the same way — `container.after`,
 * outside the element a redraw replaces. A plot in the cube gets an angle and a
 * reset; a brushed plot gets a count and a reset, and neither is in the
 * sentence, because reporting a selection does not change what the picture
 * claims about the data.
 */
function addSelectionBar(container, handle, view) {
  const bar = controlBar("selection");

  // A visible mode rather than a modifier nobody discovers. Plotly's modebar is
  // the proven shape here and the reason is not taste: a drag can only mean one
  // thing at a time, so the reader has to be able to see which, and change it.
  //
  // The word `drag:` stays beside the icons. An icon alone is only recognizable
  // to someone who has met it before, and the whole point of this bar is the
  // reader who has not.
  const label = document.createElement("span");
  label.textContent = "drag:";
  label.style.cssText = "color:inherit;opacity:.68;";

  // A dashed rectangle, a dashed free loop, and the four-way arrow that means
  // move — drawn rather than typed, because no font carries a lasso.
  const icon = (body) =>
    `<svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true" ` +
    `style="display:block;fill:none;stroke:currentColor;stroke-width:1.3">${body}</svg>`;
  // Each says what a *drag* will do, because that is the word the row opens
  // with and the question a reader is holding while they read three icons.
  const MODES = [
    ["select", "drag a rectangle to select rows",
      icon(`<rect x="2.5" y="4" width="11" height="8" stroke-dasharray="3 2"/>`)],
    ["lasso", "draw a free shape around the rows you want",
      icon(`<path d="M8 3.2c3.6 0 5.3 1.9 5.3 3.6 0 2.2-2.6 3.8-5.6 3.8` +
           `-2.6 0-4.9-1.2-4.9-3 0-2.3 2.4-4.4 5.2-4.4z" stroke-dasharray="3 2"/>` +
           `<path d="M4.6 10.2 3.4 13.4"/>`)],
  ];
  if (view) {
    MODES.push(["pan", "drag to move the picture",
      icon(`<path d="M8 2.2 8 13.8M2.2 8 13.8 8"/>` +
           `<path d="M8 2.2 6.4 4M8 2.2 9.6 4M8 13.8 6.4 12M8 13.8 9.6 12` +
           `M2.2 8 4 6.4M2.2 8 4 9.6M13.8 8 12 6.4M13.8 8 12 9.6"/>`)]);
  }
  const picks = MODES.map(([name, says, art]) => {
    const b = document.createElement("button");
    b.type = "button";
    b.innerHTML = art;
    b.style.cssText = BUTTON_STYLE;
    hoverLabel(b, says);
    b.addEventListener("click", () => {
      handle.setMode(name);
      render();
    });
    return [name, b];
  });

  const readout = document.createElement("span");
  readout.style.cssText = "font-variant-numeric:tabular-nums;";

  // Why pointing at this plot names nothing, in the reader's words rather than
  // the engine's. The engine ships one word, because it is the only side that
  // knows a mark was moved off its value; the sentence is written here, beside
  // "clear" and "show rows", because this is where the page speaks.
  //
  // Every one of these says the same thing twice over: what the plot did to the
  // position, and therefore why a pointer cannot answer. A reader who knows the
  // first can predict the rest, which is the point of saying it rather than
  // going quiet.
  const WHY = {
    jitter: "`jitter` draws each point beside its value, not on it, so pointing at one cannot say which row it is.",
    repel: "`repel` moves each label until it is clear of the others, so a label no longer sits where its row does.",
    dodge: "`dodge` sets each mark beside its value to clear its neighbors, so pointing at one cannot say which row it is.",
    stack: "`stack` sets each mark on top of the one below, so where a mark sits is not where its value is.",
    summary: "each mark here stands for many rows at once, so there is no one row under the pointer to name.",
    bounds: "these shapes are placed by the bounds you gave them rather than by a row's value.",
    mark: "this mark draws one shape through many rows, so no single row is under the pointer.",
    polar: "a polar plot bends both axes around a circle, and the page reads a value back along straight ones.",
    map: "a map turns longitude and latitude into places on the page before drawing, so the two do not line up.",
  };
  const note = document.createElement("span");
  note.style.cssText = "opacity:.72;";

  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.style.cssText =
    "font:inherit;color:inherit;background:none;border:1px solid currentColor;border-color:color-mix(in srgb, currentColor 34%, transparent);" +
    "border-radius:3px;padding:0 .5em;cursor:pointer;";

  const reset = document.createElement("button");
  reset.type = "button";
  reset.title = "clear the selection";
  reset.textContent = "clear";
  reset.style.cssText = toggle.style.cssText;

  // Stamps are undone by their own control, not by `clear`. The two act on
  // different things, which is the rule the pair beside them already follows:
  // `clear` acts on the selection and `fit` on the view, and each is named for
  // what it acts on. A reader who clears a bound to read their stamped rows
  // against the whole data would lose them to any other arrangement.
  const unstamp = document.createElement("button");
  unstamp.type = "button";
  unstamp.title = "take the stamps off the plot";
  unstamp.textContent = "unstamp";
  unstamp.style.cssText = toggle.style.cssText;
  unstamp.addEventListener("click", () => {
    handle.clearStamps?.();
    render();
  });

  const table = document.createElement("div");
  table.style.cssText =
    "display:none;overflow-x:auto;margin:-8px auto 12px;max-width:100%;" +
    "font:12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;";

  // A page at a time, with a way to the next one. The table used to stop at a
  // dozen rows and say how many it had left out, which is honest but leaves the
  // reader with a count they cannot open. Selecting a group in order to read it
  // is the whole point of `show rows`, so the rest of the group has to be
  // reachable.
  //
  // Ten to a page rather than a dozen: the reader is counting rows against a
  // total, and tens are what a person adds up without stopping to think.
  //
  // Paging rather than a scrolling box, deliberately: a selection has no upper
  // size, and a table with one row per selected datum would grow without bound
  // in a page that also has to hold the plot.
  const pager = document.createElement("div");
  pager.style.cssText =
    "display:none;margin:-8px 0 12px;gap:.5em;align-items:center;" +
    "justify-content:center;color:inherit;" +
    "font:12px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;";
  // An arrow head is a glyph rather than a word, so it earns a label on the same
  // test the icons do: a reader can see which way it points without being told
  // what it moves. The padding and the line box are the only two things a text
  // control needs differently from an icon, so they are stated after the shared
  // rule rather than in place of it.
  const step = (glyph, says) => {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = glyph;
    b.style.cssText = BUTTON_STYLE + "padding:0 .45em;line-height:1.6;";
    hoverLabel(b, says);
    return b;
  };
  const back = step("‹", "the rows before these");
  const forth = step("›", "the rows after these");
  const place = document.createElement("span");
  place.style.cssText = "font-variant-numeric:tabular-nums;";
  pager.append(back, place, forth);

  // The word and its icons are one control, so they sit together rather than
  // spread across the bar's gap like three unrelated buttons.
  const group = document.createElement("span");
  group.style.cssText = "display:inline-flex;gap:.25em;align-items:center;";
  group.append(label, ...picks.map(([, b]) => b));

  // **Two rows, and which control goes in which is the point.** The five view
  // buttons are the only ones a reader meets under *every* plot in the book, so
  // they get a line of their own, first, directly under the picture. On a plain
  // plot that line is the whole bar; on this one it is the same line in the same
  // place with more beneath it. In one row they slid left and right as the
  // selection controls beside them changed width, so the button a reader wanted
  // was never twice in the same spot.
  const viewRow = controlBar("view");
  if (view) addViewControls(viewRow, view, () => render(), handle);
  // A plot that is played *and* brushed gets its transport on this same row. It
  // joins the five rather than the selection controls below, because it is the
  // medium's control like they are, and because the selection row changes width
  // as the readout counts and `unstamp` comes and goes.
  const transport = addTransport(viewRow, container, view);

  // What this plot adds for its own sake. `toggle`, `reset` and `unstamp` stay
  // one child of the row rather than three, so a narrow panel breaks after the
  // readout instead of through the middle of the button strip.
  const controls = document.createElement("span");
  controls.style.cssText = "display:inline-flex;gap:.75em;align-items:center;";
  controls.append(toggle, reset, unstamp);
  bar.append(group, readout, controls);
  placeBar(container, viewRow, bar);
  // Under the bar rather than in it. It is a sentence and the bar is a line of
  // labels, so putting it inline would push the buttons about the moment it
  // appeared, on a plot the reader had only pointed at.
  bar.after(note);
  note.after(table);
  table.after(pager);

  let open = false;
  let page = 0;
  const render = () => {
    // This function brings the whole control line up to date, so the transport's
    // button belongs in it. A selection redraws the picture, which replaces the
    // element the clock lives in, and a button still drawn from the old one would
    // tell the reader the plot was stopped while it ran.
    transport?.refresh();
    const s = handle.selection(page * PAGE_ROWS);
    for (const [name, b] of picks) {
      const on = handle.mode() === name;
      b.style.borderColor = on ? "#666" : "#ccc";
      b.style.background = on ? "#eee" : "none";
      b.style.color = on ? "#222" : "#777";
      b.setAttribute("aria-pressed", on ? "true" : "false");
    }
    readout.textContent = `${s.kept} of ${s.total} selected`;
    const why = handle.unplaced?.();
    note.textContent = why ? `Pointing reads no row here: ${WHY[why] ?? ""}` : "";
    note.style.display = why ? "block" : "none";
    // Nothing to show and nothing to reset when nothing is selected. The
    // buttons go quiet rather than disappearing, so the line does not jump.
    const idle = s.kept === 0 || s.kept === s.total;
    toggle.disabled = idle;
    reset.disabled = idle;
    // Absent rather than dimmed while there is nothing stamped. `clear` and the
    // view buttons go quiet in place because a reader who has selected once will
    // select again, and a jumping line is worse than a dead button; a plot that
    // has never been stamped should not carry a word for it at all.
    const stamped = handle.stamps?.() ?? 0;
    unstamp.style.display = stamped ? "" : "none";
    unstamp.textContent = stamped === 1 ? "unstamp" : `unstamp ${stamped}`;
    toggle.textContent = open ? "hide rows" : "show rows";
    if (!open || idle) {
      table.style.display = "none";
      pager.style.display = "none";
      return;
    }
    const cell = (v) =>
      `<td style="padding:.1em .6em;text-align:${typeof v === "number" ? "right" : "left"}">` +
      `${v === null || v === undefined ? "" : String(v)}</td>`;
    table.innerHTML =
      `<table style="margin:0 auto;border-collapse:collapse"><thead><tr>` +
      s.columns.map((c) => `<th style="padding:.1em .6em;text-align:left;` +
        `border-bottom:1px solid color-mix(in srgb, currentColor 22%, transparent);color:inherit">${c}</th>`).join("") +
      `</tr></thead><tbody>` +
      s.rows.map((r) => `<tr>${r.map(cell).join("")}</tr>`).join("") +
      `</tbody></table>`;
    table.style.display = "block";
    // The line under the table says where you are in the selection rather than
    // only what was left out, and the two arrows are how you leave. It appears
    // only when there is more than one page, so a short selection reads exactly
    // as it did before.
    pager.style.display = s.capped ? "flex" : "none";
    place.textContent = `${s.from}–${s.to} of ${s.kept}`;
    back.disabled = page === 0;
    forth.disabled = s.to >= s.kept;
  };

  const turn = (by) => {
    page = Math.max(0, page + by);
    render();
  };
  back.addEventListener("click", () => turn(-1));
  forth.addEventListener("click", () => turn(1));
  toggle.addEventListener("click", () => { open = !open; render(); });
  reset.addEventListener("click", () => { handle.reset(); render(); });
  render();
  // **The selection moving is not the same event as the reader turning a page.**
  // This is what a redraw calls, so it goes back to the first page: a new
  // selection has new rows, and page four of the last one means nothing. The
  // arrows call `render` directly and keep their place.
  return () => {
    page = 0;
    render();
  };
}

/**
 * Make a plot turnable: render it into `container` and drag to rotate.
 *
 * @param {object} engine from {@link loadEngine}
 * @param {HTMLElement} container the element to draw into
 * @param {object} request the `{spec, data}` wire object
 * @param {object} [options]
 * @param {number} [options.degreesPerPixel=0.5] drag sensitivity
 * @param {(notes: string[]) => void} [options.onNotes] receives the engine's
 *   non-fatal diagnostics from the first render
 * @param {(view: {turn: number, tilt: number}) => void} [options.onView] called
 *   after every redraw with the angle now being shown
 * @returns {{destroy: () => void, view: () => ({turn, tilt}), reset: () => void,
 *   opened: {turn: number, tilt: number}}}
 */
export function attachDrag(engine, container, request, options = {}) {
  const { degreesPerPixel = 0.5, onNotes, onView } = options;

  // Work on a copy. Rotating a plot must not mutate the caller's spec — the
  // same object may be rendered again, statically, somewhere else on the page.
  const req = JSON.parse(JSON.stringify(request));

  // **Every plot in the figure that has an angle**, each remembering the one its
  // own sentence asked for. A page keeps its spaces on its cells, so this is a
  // list of one for an ordinary plot and a list of cells for a composition.
  //
  // The angle a plot opened at is the angle the *sentence* asked for, not the
  // engine's default. `reset` returns there rather than to 30/25 because the
  // prose around a plot is describing the picture the author chose: a chapter
  // that turns a volcano through four views is making an argument about those
  // four, and a reset that landed somewhere else would quietly contradict the
  // paragraph beside it.
  const scenes = eachPlot(req.spec).filter(plotIsSpatial).map((plot) => {
    // Which view this plot carries: the globe's, or the cube's. Each cell
    // keeps its own kind, so a page holding one of each turns both with one
    // drag and rewrites each under its own word.
    const kind =
      plot.coord && typeof plot.coord === "object" && plot.coord.globe ? "globe"
      : plot.coord && typeof plot.coord === "object" && plot.coord.network ? "network"
      : "space";
    const s = plot.coord && typeof plot.coord === "object" ? plot.coord[kind] : null;
    // The globe's default view faces (0, 0), the sphere's own origin; the
    // cube's is the three-quarter view. Each falls back to its own.
    const turn0 = kind === "globe" ? 0 : DEFAULT_TURN;
    const tilt0 = kind === "globe" ? 0 : DEFAULT_TILT;
    return { plot, kind, turn: s?.turn ?? turn0, tilt: s?.tilt ?? tilt0 };
  });

  // **The gesture carries a change, not an angle**, and that is what lets one
  // drag serve a whole composition without overriding anything a cell said. Each
  // panel turns from wherever its own sentence put it, so whatever the panels
  // differed by they still differ by: a four-angle tour is still a four-angle
  // tour after it has been turned. Setting one absolute angle across the page
  // would collapse those four onto one, which is the enclosing expression
  // silently reinterpreting the inner ones that Law 6 forbids.
  const opened = scenes.length
    ? { turn: scenes[0].turn, tilt: scenes[0].tilt }
    : { turn: DEFAULT_TURN, tilt: DEFAULT_TILT };
  let dTurn = 0;
  let dTilt = 0;

  // Tilt stops at straight down and straight up, which is exactly where a written
  // `space(tilt = 90)` stops: a drag reaches every angle a sentence can name and
  // no angle it cannot. It stopped one degree short until a reader dragged to the
  // end and found 89 under a chapter that said 90 — a stop the grammar could not
  // explain, because nothing in the grammar knows about it. The stop itself
  // stays: past 90 the eye goes over the top, the scene turns over, and the axis
  // names pile onto one corner.
  //
  // The limit is **shared**: the range is the one every panel can reach. Clamping
  // each panel on its own would let the steepest hit the stop while the others
  // kept going, and they would drift apart — the one thing carrying a delta
  // exists to prevent.
  const tiltFloor = scenes.length ? Math.max(...scenes.map((s) => -90 - s.tilt)) : -90;
  const tiltCeil = scenes.length ? Math.min(...scenes.map((s) => 90 - s.tilt)) : 90;

  // What the readout says: the first scene's angle, which is *the* angle for an
  // ordinary plot and a true one for a panel of a page.
  const angle = () => ({ turn: (opened.turn + dTurn) % 360, tilt: opened.tilt + dTilt });

  let first = true;
  function draw() {
    for (const s of scenes) {
      s.plot.coord = { [s.kind]: { turn: (s.turn + dTurn) % 360, tilt: s.tilt + dTilt } };
    }
    const { ok, notes } = redraw(engine, container, req);
    if (!ok) return false;
    if (first) {
      first = false;
      if (onNotes && notes.length) onNotes(notes);
    }
    if (onView) onView(angle());
    return true;
  }

  if (!draw()) return { destroy() {}, view: angle };

  let dragging = false;
  let lastX = 0;
  let lastY = 0;
  let queued = false;

  const onDown = (e) => {
    dragging = true;
    lastX = e.clientX;
    lastY = e.clientY;
    // Capture keeps the drag alive when the pointer leaves the plot, and it is
    // an improvement rather than a requirement — dragging works without it. It
    // throws `NotFoundError` for a pointer id that is not active, which a real
    // mouse never produces but a synthetic `PointerEvent` does, so a test
    // driving the plot would otherwise raise an uncaught error mid-drag.
    try {
      container.setPointerCapture?.(e.pointerId);
    } catch {
      /* no active pointer to capture; the drag proceeds without it */
    }
  };
  const onMove = (e) => {
    if (!dragging) return;
    // **The cube follows the pointer; the camera is what moves to achieve it.**
    // Drag right and the face turned toward you goes right; drag down and it
    // tips down, opening the top of the cube. Both signs are negative against
    // the angles because `turn` and `tilt` place the *camera*, and a camera
    // walks the opposite way from the thing it looks at: step to your right and
    // the near face swings left. Driving the camera with the pointer instead is
    // a defensible reading of the same gesture and it is the rarer one — three.js,
    // plotly, Blender and matplotlib all pin the object to the pointer, and a
    // reader arrives here with that convention already in their hand.
    dTurn -= (e.clientX - lastX) * degreesPerPixel;
    // At exactly 90 the floor collapses to a line and the picture has no depth
    // to read, so the stop is a guard rail rather than a limitation.
    dTilt = Math.max(
      tiltFloor,
      Math.min(tiltCeil, dTilt + (e.clientY - lastY) * degreesPerPixel),
    );
    lastX = e.clientX;
    lastY = e.clientY;
    // Coalesce to one redraw per frame. A pointer can fire far more often than
    // the screen refreshes, and rendering per event would do work no one sees.
    if (!queued) {
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        draw();
      });
    }
  };
  const onUp = () => {
    dragging = false;
  };

  container.addEventListener("pointerdown", onDown);
  container.addEventListener("pointermove", onMove);
  container.addEventListener("pointerup", onUp);
  container.addEventListener("pointercancel", onUp);

  return {
    destroy() {
      container.removeEventListener("pointerdown", onDown);
      container.removeEventListener("pointermove", onMove);
      container.removeEventListener("pointerup", onUp);
      container.removeEventListener("pointercancel", onUp);
    },
    view: angle,
    opened,
    // Every panel back to the angle its own sentence named, which is what
    // clearing the change does — there is no per-panel state to restore.
    reset() {
      dTurn = 0;
      dTilt = 0;
      draw();
    },
  };
}

/**
 * The readout under a turnable plot: the angle it is showing, and a way back.
 *
 * Built here rather than emitted by the bindings, for the same reason `mount`
 * is: it is display, and four bindings writing the same HTML is four chances to
 * write it differently. It is also created *by script*, which means a reader
 * with no JavaScript sees no controls at all rather than a dead button beside a
 * plot that cannot move.
 *
 * It is inserted **after** the plot's container, never inside it. Every redraw
 * replaces the container's `innerHTML`, so controls placed within would be
 * destroyed by the first drag they caused.
 */
function addControls(container, handle, view = null) {
  const bar = controlBar("view");

  const hint = document.createElement("span");
  // `turn` rather than `rotate`, for the two reasons the kernel's names follow:
  // it is the plainer English word, and it is the one the readout beside it and
  // `space(turn = )` already use, so the hint teaches the parameter instead of a
  // synonym for it. Shorter also matters here, because this bar has to fit beside
  // six controls in a panel that may be half the page wide.
  hint.textContent = "drag to turn";
  hint.style.cssText = "color:inherit;opacity:.62;";

  const readout = document.createElement("span");
  // `tabular-nums` so the numbers do not shuffle the line's width as they
  // change — a readout that jitters while you drag is harder to read than one
  // digit wider.
  readout.style.cssText = "font-variant-numeric:tabular-nums;";

  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = "reset";
  reset.style.cssText =
    "font:inherit;color:inherit;background:color-mix(in srgb, currentColor 9%, transparent);border:1px solid currentColor;border-color:color-mix(in srgb, currentColor 30%, transparent);" +
    "border-radius:4px;padding:0 .5em;cursor:pointer;";
  reset.addEventListener("click", () => handle.reset());

  const show = ({ turn, tilt }) => {
    readout.textContent = `turn ${Math.round(turn)}° · tilt ${Math.round(tilt)}°`;
    // The button is only meaningful once the view has actually moved.
    const moved =
      Math.round(turn) !== Math.round(handle.opened.turn) ||
      Math.round(tilt) !== Math.round(handle.opened.tilt);
    reset.disabled = !moved;
    reset.style.opacity = moved ? "1" : "0.4";
    reset.style.cursor = moved ? "pointer" : "default";
  };

  // The four view buttons take the first row here for the same reason they do
  // under a brushed plot: they are the row every plot has, so they keep one
  // place under every picture. What is left below is what a cube adds, and it
  // reads as a sentence about the angle: what the angle is, and a way back to
  // the one the plot opened at.
  const viewRow = controlBar("view");
  if (view) addViewControls(viewRow, view);
  // A cube that also plays. The drag is the camera's here, so the transport is
  // the only way to hold a frame still, which makes it worth more in the cube
  // than anywhere else: a reader who wants to turn one moment has to stop the
  // clock first.
  addTransport(viewRow, container, view);

  bar.append(hint, readout);
  // The zoom sits between the readout and `reset`, and it is given **no drag
  // handle**. In the cube the drag is already spoken for: it turns the scene,
  // which is the whole reason this bar exists. So the buttons zoom and the
  // gesture keeps its one meaning, which is the rule the selection chapter
  // states from the other side.
  //
  // Two buttons undo something here, and each is named for *what it acts on*
  // rather than both being called reset: `fit` returns the zoom, `reset` returns
  // the angle. They are now a row apart, which says the same thing more plainly
  // than sitting them side by side did. `fit` belongs with the buttons that
  // changed the zoom, and `reset` belongs beside the readout stating the angle
  // it undoes. Pressing either still leaves the other alone, so a reader who
  // found an angle does not lose it by zooming back out.
  //
  // **The bar wraps, and must** — a panel can be half the page wide in a
  // two-across layout, and nothing may overflow. What it must not do is break
  // *between* buttons, which once left the camera stranded on a line of its own
  // while four icons sat above it. A set of controls that acts together is one
  // child of its row, not five.
  bar.append(reset);
  placeBar(container, viewRow, bar);
  return show;
}

/**
 * Make one already-rendered plot turnable. This is what the emitted HTML calls,
 * and the reason each of the four bindings emits three lines rather than thirty.
 *
 * The container already holds the **static** SVG the binding rendered, and that
 * is deliberate: it is what a reader sees in a PDF, in a notebook viewer that
 * strips JavaScript, and in the moment before the engine finishes loading. This
 * function upgrades that picture in place. If it never runs — no JavaScript, no
 * WebAssembly, a failed fetch — the plot stays exactly the honest still image it
 * already was, which is the same way `play` degrades in print.
 *
 * @param {string|HTMLElement} target the container, or its id
 * @param {object} request the `{spec, data}` wire object
 * @param {object} [options]
 * @param {string|BufferSource} [options.wasm] where the engine is; a URL, a
 *   `data:` URI, or bytes
 * @returns {Promise<object|null>} the drag handle, or null if it did not attach
 */
export async function mount(target, request, options = {}) {
  const container =
    typeof target === "string" ? document.getElementById(target) : target;
  if (!container) return null;

  // Two reasons to load the **engine**: a plot in the cube has an angle worth
  // dragging, and a plot that names a brush has a bound worth moving. Both
  // redraw, so both need the engine.
  const spatial = isSpatial(request?.spec);
  const brushed = hasBrush(request?.spec);

  // **Zoom is not one of them, and it used to be stuck behind them.** Looking
  // closer scales the `viewBox` and recomputes nothing, so it needs no engine at
  // all — 65 KB of this module against 861 KB of WebAssembly. A flat plot was
  // returning here with no controls because the question asked was *do you need
  // the engine*, which is the right question for the drag and the wrong one for
  // the buttons. Every plot can be looked at closely; only some can be turned.
  //
  // `fit` and not `reset`, and the cube's own chapter says why: there are two
  // buttons there **because they undo different things** — `fit` returns the
  // zoom, `reset` returns the angle. A flat plot has no angle, so a second
  // button would undo nothing distinct.
  // **A flat plot never reaches the engine, and now never loads it either.**
  // Looking closer needs no spec and no data — `mountView` takes a container and
  // stops — so this delegates rather than duplicating it, and a binding emitting
  // a flat plot can name `view.js` alone and leave this file behind.
  if (!spatial && !brushed) {
    const handle = mountView(container, options);
    if (handle) {
      container.dataset.gogBuild = BUILD;
      return { ...handle, opened: [] };
    }
    return null;
  }

  try {
    const engine = await engineFor(options.wasm);

    // A brush without a cube: the selection is the only thing to move, so there
    // is no angle readout and no reset-the-view bar. `crosshair` says the panel
    // is the thing to drag, where `grab` says the scene is.
    if (!spatial) {
      let show = () => {};
      const view = attachView(container, options);
      const handle = attachBrush(engine, container, request, {
        ...options,
        view,
        onSelect: () => show(),
      });
      if (options.controls !== false) show = addSelectionBar(container, handle, view);
      show();
      container.style.cursor = "crosshair";
      container.dataset.gogInteractive = "true";
      container.dataset.gogBuild = BUILD;
      return handle;
    }
    // `show` is created before the handle exists but is only ever called from
    // `onView`, which cannot fire until `attachDrag`'s first draw — so the
    // forward reference is safe and saves attaching the controls twice.
    let show = () => {};
    // A cube redraws by replacing the container's contents on every drag, which
    // throws the `viewBox` away with the old element — so the zoom has to be
    // re-applied after each draw or the first turn would snap it back to fit.
    // `onView` fires after the draw, which is the one place that is true.
    const view = attachView(container, options);
    const handle = attachDrag(engine, container, request, {
      ...options,
      onView: (angles) => {
        view.apply();
        show(angles);
      },
    });
    if (options.controls !== false) show = addControls(container, handle, view);
    show(handle.view());
    container.style.cursor = "grab";
    container.dataset.gogInteractive = "true";
    container.dataset.gogBuild = BUILD;
    return handle;
  } catch (e) {
    // Never let a missing engine cost the reader the picture. The static SVG is
    // already on the page and stays there; the failure goes to the console,
    // where someone debugging the page will find it.
    console.warn("gog: interactive engine unavailable, plot stays static —", e);
    return null;
  }
}
