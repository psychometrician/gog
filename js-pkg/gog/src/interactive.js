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
  controlBar,
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

/** Does this **plot** draw in the cube? The leaf question, asked per cell. */
function plotIsSpatial(spec) {
  if (spec?.coord && typeof spec.coord === "object" && spec.coord.space) return true;
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

  const outgoing = container.querySelector("svg");
  let clock = null;
  if (outgoing && typeof outgoing.getCurrentTime === "function") {
    try {
      clock = outgoing.getCurrentTime();
    } catch {
      clock = null;
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
    if (onSelect) onSelect();
    return true;
  }
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
      // y runs down the page and up the axis, so its two ends are swapped
      // against x's. That is the one asymmetry here.
      return { el: g, x0, y0, x1, y1, x: axis("x", x0, x1), y: axis("y", y1, y0) };
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
  // ---------------------------------------------------------------------
  let tip = null;

  const nearest = (panel, at) => {
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
      for (let i = 0; i < n; i++) {
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
    tip.innerHTML = hit.row
      .map(([f, v]) => `<div><span style="color:#888">${f}</span> ${v ?? ""}</div>`)
      .join("");
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

  // Where a pixel falls on an axis, in the column's own units — or, on a column
  // of categories, which slots the drag covered. A category owns an equal share
  // of the panel, so the slot is the fraction times the count, floored.
  // Shorter than this and the reader did not draw a range, they clicked.
  const MIN_DRAG = 3;

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
  const onUp = () => {
    // A click clears the shape, exactly as it clears a bound: too few samples to
    // enclose anything is not a tiny selection, it is a reader asking for none.
    if (trace && trace.length < 3 && clearRegion()) schedule();
    held = -1;
    start = null;
    trace = null;
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
      hideBand();
      hideTip();
    },
    opened,
    /** What the reader has caught: a count, and one page of the rows to read. */
    selection: (offset = 0) => selectedRows(req, PAGE_ROWS, offset),
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
  const MODES = [
    ["select", "select a rectangle",
      icon(`<rect x="2.5" y="4" width="11" height="8" stroke-dasharray="3 2"/>`)],
    ["lasso", "draw a free shape around what you want",
      icon(`<path d="M8 3.2c3.6 0 5.3 1.9 5.3 3.6 0 2.2-2.6 3.8-5.6 3.8` +
           `-2.6 0-4.9-1.2-4.9-3 0-2.3 2.4-4.4 5.2-4.4z" stroke-dasharray="3 2"/>` +
           `<path d="M4.6 10.2 3.4 13.4"/>`)],
  ];
  if (view) {
    MODES.push(["pan", "move the picture",
      icon(`<path d="M8 2.2 8 13.8M2.2 8 13.8 8"/>` +
           `<path d="M8 2.2 6.4 4M8 2.2 9.6 4M8 13.8 6.4 12M8 13.8 9.6 12` +
           `M2.2 8 4 6.4M2.2 8 4 9.6M13.8 8 12 6.4M13.8 8 12 9.6"/>`)]);
  }
  const picks = MODES.map(([name, title, art]) => {
    const b = document.createElement("button");
    b.type = "button";
    b.title = title;
    b.innerHTML = art;
    b.style.cssText =
      "font:inherit;color:inherit;background:none;border:1px solid currentColor;border-color:color-mix(in srgb, currentColor 34%, transparent);" +
      "border-radius:3px;padding:.15em .3em;cursor:pointer;line-height:0;";
    b.addEventListener("click", () => {
      handle.setMode(name);
      render();
    });
    return [name, b];
  });

  const readout = document.createElement("span");
  readout.style.cssText = "font-variant-numeric:tabular-nums;";

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
  const step = (label, title) => {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = label;
    b.title = title;
    b.style.cssText =
      "font:inherit;color:inherit;background:none;border:1px solid currentColor;border-color:color-mix(in srgb, currentColor 34%, transparent);" +
      "border-radius:3px;padding:0 .45em;cursor:pointer;";
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

  // The same grouping the cube's bar uses: `toggle`, `reset` and the view buttons
  // are controls and stay on one line together, so a narrow panel breaks after the
  // readout rather than through the middle of the button strip.
  const controls = document.createElement("span");
  controls.style.cssText = "display:inline-flex;gap:.75em;align-items:center;";
  controls.append(toggle, reset);
  if (view) addViewControls(controls, view, () => render(), handle);
  bar.append(group, readout, controls);
  placeBar(container, bar);
  bar.after(table);
  table.after(pager);

  let open = false;
  let page = 0;
  const render = () => {
    const s = handle.selection(page * PAGE_ROWS);
    for (const [name, b] of picks) {
      const on = handle.mode() === name;
      b.style.borderColor = on ? "#666" : "#ccc";
      b.style.background = on ? "#eee" : "none";
      b.style.color = on ? "#222" : "#777";
      b.setAttribute("aria-pressed", on ? "true" : "false");
    }
    readout.textContent = `${s.kept} of ${s.total} selected`;
    // Nothing to show and nothing to reset when nothing is selected. The
    // buttons go quiet rather than disappearing, so the line does not jump.
    const idle = s.kept === 0 || s.kept === s.total;
    toggle.disabled = idle;
    reset.disabled = idle;
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
    const s = plot.coord && typeof plot.coord === "object" ? plot.coord.space : null;
    return { plot, turn: s?.turn ?? DEFAULT_TURN, tilt: s?.tilt ?? DEFAULT_TILT };
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

  // Tilt stops just short of overhead, and the limit is **shared**: the range is
  // the one every panel can reach. Clamping each panel on its own would let the
  // steepest hit the stop while the others kept going, and they would drift
  // apart — the one thing carrying a delta exists to prevent.
  const tiltFloor = scenes.length ? Math.max(...scenes.map((s) => -89 - s.tilt)) : -89;
  const tiltCeil = scenes.length ? Math.min(...scenes.map((s) => 89 - s.tilt)) : 89;

  // What the readout says: the first scene's angle, which is *the* angle for an
  // ordinary plot and a true one for a panel of a page.
  const angle = () => ({ turn: (opened.turn + dTurn) % 360, tilt: opened.tilt + dTilt });

  let first = true;
  function draw() {
    for (const s of scenes) {
      s.plot.coord = { space: { turn: (s.turn + dTurn) % 360, tilt: s.tilt + dTilt } };
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

  bar.append(hint, readout);
  // The zoom sits between the readout and `reset`, and it is given **no drag
  // handle**. In the cube the drag is already spoken for: it turns the scene,
  // which is the whole reason this bar exists. So the buttons zoom and the
  // gesture keeps its one meaning, which is the rule the selection chapter
  // states from the other side.
  //
  // Two buttons that both undo something sit side by side here, so each is
  // named for *what it acts on* rather than both being called reset: `fit`
  // returns the zoom, `reset` returns the angle. Pressing either leaves the
  // other alone, so a reader who found an angle does not lose it by zooming
  // back out.
  // **The buttons are one unit, so the bar may only break beside them.** The bar
  // wraps, which it must — a panel can be half the page wide in a two-across
  // layout, and nothing may overflow. What it must not do is break *between*
  // buttons, which left the camera and `reset` stranded on a second line while
  // four icons sat on the first. Grouping them is the selection bar's own answer
  // to the same problem, one bar over: a set of controls that acts together is
  // one child of the bar, not five. The readout keeps its own place, so what
  // yields at a narrow width is the words rather than the controls.
  const controls = document.createElement("span");
  controls.style.cssText = "display:inline-flex;gap:.75em;align-items:center;";
  if (view) addViewControls(controls, view);
  controls.append(reset);
  bar.append(controls);
  placeBar(container, bar);
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
