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
export const DEFAULT_TURN = 30;
export const DEFAULT_TILT = 25;

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

/** Does this spec draw in the cube? Only a 3-D plot has an angle to drag. */
export function isSpatial(spec) {
  if (spec?.coord && typeof spec.coord === "object" && spec.coord.space) return true;
  // A `z` binding puts a plot in the cube without naming `space()`, so the
  // coordinate can still read "flat" on a plot that projects. `space_of` makes
  // the same judgment in the engine; this is its browser-side twin, and it is
  // why the check is not simply `coord.space`.
  const layers = spec?.layers ?? [];
  return layers.some((l) => l?.encodings && "z" in l.encodings) || spec?.z != null;
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

  if (clock !== null) {
    const incoming = container.querySelector("svg");
    if (incoming && typeof incoming.setCurrentTime === "function") {
      try {
        incoming.setCurrentTime(clock);
      } catch {
        /* a static plot has no timeline; nothing to restore */
      }
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
  if (axis.cats) {
    const n = axis.cats.length;
    const first = Math.max(0, Math.min(n - 1, Math.floor(f0 * n)));
    const last = Math.max(0, Math.min(n - 1, Math.floor(f1 * n)));
    return { levels: axis.cats.slice(first, last + 1) };
  }
  // A log axis states its domain in log space, because that is the space
  // positions are linear in, so interpolating between its two numbers gives a
  // logarithm rather than a value. Undoing that is the whole of what `log` is
  // for, and without it a drag on a log axis produced a bound in units the
  // engine does not compare against.
  const value = (f) => {
    const v = axis.from + f * (axis.to - axis.from);
    return axis.log ? axis.log ** v : v;
  };
  return { at: [value(f0), value(f1)] };
}

/**
 * Where a value sits on an axis, in the units the panel rectangle is written
 * in. `boundOn` run forwards, and the only other arithmetic the browser needs.
 */
export function placeOn(axis, value) {
  if (axis.cats) {
    const i = axis.cats.indexOf(value);
    if (i < 0) return null;
    return axis.lo + ((i + 0.5) / axis.cats.length) * (axis.hi - axis.lo);
  }
  if (typeof value !== "number" || !Number.isFinite(value)) return null;
  const v = axis.log ? Math.log(value) / Math.log(axis.log) : value;
  const f = (v - axis.from) / (axis.to - axis.from || 1);
  return axis.lo + f * (axis.hi - axis.lo);
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

  let panning = null;

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
    showBand(panel, start, now);
    if (!apply(panel, start, now)) return;
    if (!queued) {
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        draw();
      });
    }
  };
  const onUp = () => {
    held = -1;
    start = null;
    if (panning) container.style.cursor = "grab";
    panning = null;
    hideBand();
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
    /** What the reader has caught: a count, and the rows to read. */
    selection: () => selectedRows(req),
    /** What a plain drag does now. Zooming in switches it, because a reader who
     *  has just magnified something almost always wants to move around in it. */
    mode,
    setMode(next) {
      dragMode = next === "pan" && view ? "pan" : "select";
      // The pointer says what the drag will do before the reader tries it. An
      // open hand is the universal "you can move this"; a crosshair is the
      // universal "you can draw a box here".
      container.style.cursor = dragMode === "pan" ? "grab" : "crosshair";
    },
    reset() {
      // Back to what the sentence said, which for a bare brush is the
      // declaration rather than whatever the last drag turned it into.
      eachPlot(req.spec).forEach((p, i) => {
        if (opened[i]) p.brush = JSON.parse(JSON.stringify(opened[i]));
      });
      draw();
    },
  };
}



/**
 * Looking closer at the picture, without redrawing it.
 *
 * A viewport zoom is *literally* looking closer at the same picture, so scaling
 * and translating the SVG's `viewBox` is not an approximation of one — it is
 * one. That buys three things at once: no engine call, so it costs nothing and
 * runs at any frame rate; it works on a cube and on a composed page with no
 * cases; and it cannot accidentally become the other zoom.
 *
 * **It must not refit anything.** Narrowing a domain and re-running the
 * statistics is `limits`, a different operation with a different answer — a
 * reader looking closer does not expect a histogram to re-bin. A zoom that
 * refitted would be `limits` wearing a magnifying glass, and the two would
 * collapse into one confused feature.
 *
 * Two costs, both consequences of it being a *view* rather than a new plot, and
 * both worth stating rather than discovering: the text scales with the picture,
 * and the ticks stay the ones the engine chose for the whole domain rather than
 * new ones for the part on screen. A reader who wants ticks re-chosen for a
 * range is asking for `limits`.
 *
 * `apply` has to be called after every redraw, because replacing the element
 * throws the `viewBox` away with it — the same lesson the selection band taught
 * one level up.
 */
export function attachView(container, options = {}) {
  const step = options.zoomStep ?? 1.4;
  const maxScale = options.maxZoom ?? 12;
  let base = null;
  let scale = 1;
  let cx = 0;
  let cy = 0;

  const svgEl = () => container.querySelector("svg");

  const learn = () => {
    if (base) return base;
    const svg = svgEl();
    const vb = svg?.getAttribute("viewBox");
    if (!vb) return null;
    const [x, y, w, h] = vb.trim().split(/\s+/).map(Number);
    if (![x, y, w, h].every(Number.isFinite) || w <= 0 || h <= 0) return null;
    base = { x, y, w, h };
    cx = x + w / 2;
    cy = y + h / 2;
    return base;
  };

  const apply = () => {
    const svg = svgEl();
    if (!svg || !learn()) return;
    const w = base.w / scale;
    const h = base.h / scale;
    // The window stays inside the picture, so there is no panning off into
    // blank space and no way to lose the plot entirely.
    cx = Math.min(Math.max(cx, base.x + w / 2), base.x + base.w - w / 2);
    cy = Math.min(Math.max(cy, base.y + h / 2), base.y + base.h - h / 2);
    svg.setAttribute("viewBox", `${cx - w / 2} ${cy - h / 2} ${w} ${h}`);
  };

  /** Pixels of pointer movement, in the units the `viewBox` is written in. */
  const perPixel = () => {
    const svg = svgEl();
    const box = svg?.getBoundingClientRect();
    if (!box || !box.width || !learn()) return 0;
    return base.w / scale / box.width;
  };

  return {
    apply,
    zoomed: () => scale !== 1,
    zoom(by) {
      if (!learn()) return;
      scale = Math.min(Math.max(scale * by, 1), maxScale);
      apply();
    },
    panBy(dxPx, dyPx) {
      const u = perPixel();
      if (!u) return;
      cx -= dxPx * u;
      cy -= dyPx * u;
      apply();
    },
    reset() {
      if (!learn()) return;
      scale = 1;
      cx = base.x + base.w / 2;
      cy = base.y + base.h / 2;
      apply();
    },
  };
}

/**
 * The three buttons, appended to whichever bar the plot already has.
 *
 * A button competes with no gesture, which is why the zoom always gets them
 * while the *drag* has to be earned: the sentence decides what a drag means, so
 * a plot that says `brush` has already given its drag away and pans with a
 * modifier instead.
 */
function addZoomButtons(bar, view, onChange = () => {}, handle = null) {
  const style =
    "font:inherit;color:#555;background:none;border:1px solid #ccc;" +
    "border-radius:3px;padding:0 .5em;cursor:pointer;";
  const make = (label, title, act) => {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = label;
    b.title = title;
    b.style.cssText = style;
    b.addEventListener("click", () => {
      act();
      onChange();
    });
    return b;
  };
  // Zooming in hands the drag to panning and fitting hands it back, because a
  // reader who has just magnified something almost always wants to move around
  // in it, and one who has zoomed all the way out has nothing left to move.
  const follow = () => {
    if (!handle) return;
    handle.setMode(view.zoomed() ? "pan" : "select");
  };
  bar.append(
    make("\u2212", "zoom out", () => { view.zoom(1 / 1.4); follow(); }),
    make("+", "zoom in", () => { view.zoom(1.4); follow(); }),
    make("fit", "zoom out to the whole plot", () => { view.reset(); follow(); }),
  );
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
 */
export function selectedRows(req, limit = 12) {
  const result = { kept: 0, total: 0, columns: [], rows: [], capped: false };
  for (const plot of eachPlot(req.spec)) {
    const bounds = (plot.brush ?? []).filter((b) => b.at || b.levels);
    if (!bounds.length) continue;
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
    const columns = named.filter((f) => floats[f] || strings[f]);

    for (let i = 0; i < rows; i++) {
      const inside = bounds.every((b) => {
        const v = value(b.field, i);
        if (b.at) return typeof v === "number" && Number.isFinite(v) && v >= b.at[0] && v <= b.at[1];
        return b.levels.includes(v);
      });
      result.total++;
      if (!inside) continue;
      result.kept++;
      if (result.rows.length < limit) {
        result.rows.push(columns.map((f) => value(f, i)));
      } else {
        result.capped = true;
      }
    }
    if (!result.columns.length) result.columns = columns;
  }
  return result;
}

/**
 * Put a control bar under its plot, and keep the two together.
 *
 * `container.after(bar)` looks right and is wrong inside a Quarto `layout-ncol`
 * chunk: the layout is a flex row over the *children*, so a new sibling becomes
 * another cell and the bar lands in a narrow column beside the plot rather than
 * under it. Wrapping the pair in one element makes them one cell again, and the
 * bar still survives the redraw, because `innerHTML` is replaced on the
 * container and the bar is its sibling inside the wrapper rather than its child.
 */
function placeBar(container, bar) {
  const parent = container.parentNode;
  if (!parent) return;
  const wrap = document.createElement("div");
  wrap.className = "gog-plot-with-controls";
  parent.insertBefore(wrap, container);
  wrap.appendChild(container);
  wrap.appendChild(bar);
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
  const bar = document.createElement("div");
  bar.className = "gog-selection-controls";
  bar.style.cssText =
    "font:12px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;color:#666;" +
    "text-align:center;margin:-4px 0 12px;display:flex;gap:.75em;" +
    "align-items:center;justify-content:center;flex-wrap:wrap;";

  // A visible mode rather than a modifier nobody discovers. Plotly's modebar is
  // the proven shape here and the reason is not taste: a drag can only mean one
  // thing at a time, so the reader has to be able to see which, and change it.
  const drag = document.createElement("button");
  drag.type = "button";
  drag.title = "what dragging does";
  drag.style.cssText =
    "font:inherit;color:#555;background:none;border:1px solid #ccc;" +
    "border-radius:3px;padding:0 .5em;cursor:pointer;";

  const readout = document.createElement("span");
  readout.style.cssText = "font-variant-numeric:tabular-nums;";

  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.style.cssText =
    "font:inherit;color:#555;background:none;border:1px solid #ccc;" +
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

  bar.append(drag, readout, toggle, reset);
  if (view) addZoomButtons(bar, view, () => render(), handle);
  placeBar(container, bar);
  bar.after(table);

  let open = false;
  const render = () => {
    const s = handle.selection();
    drag.textContent = handle.mode() === "pan" ? "drag: pan" : "drag: select";
    readout.textContent = `${s.kept} of ${s.total} selected`;
    // Nothing to show and nothing to reset when nothing is selected. The
    // buttons go quiet rather than disappearing, so the line does not jump.
    const idle = s.kept === 0 || s.kept === s.total;
    toggle.disabled = idle;
    reset.disabled = idle;
    toggle.textContent = open ? "hide rows" : "show rows";
    if (!open || idle) {
      table.style.display = "none";
      return;
    }
    const cell = (v) =>
      `<td style="padding:.1em .6em;text-align:${typeof v === "number" ? "right" : "left"}">` +
      `${v === null || v === undefined ? "" : String(v)}</td>`;
    // What was left out is said out loud rather than truncated in silence.
    const note = s.capped
      ? `<caption style="caption-side:bottom;color:#999;text-align:center">` +
        `${s.rows.length} of ${s.kept} shown</caption>`
      : "";
    table.innerHTML =
      `<table style="margin:0 auto;border-collapse:collapse">${note}<thead><tr>` +
      s.columns.map((c) => `<th style="padding:.1em .6em;text-align:left;` +
        `border-bottom:1px solid #ddd;color:#555">${c}</th>`).join("") +
      `</tr></thead><tbody>` +
      s.rows.map((r) => `<tr>${r.map(cell).join("")}</tr>`).join("") +
      `</tbody></table>`;
    table.style.display = "block";
  };

  drag.addEventListener("click", () => {
    handle.setMode(handle.mode() === "pan" ? "select" : "pan");
    render();
  });
  toggle.addEventListener("click", () => { open = !open; render(); });
  reset.addEventListener("click", () => { handle.reset(); render(); });
  render();
  return render;
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

  const existing =
    req.spec?.coord && typeof req.spec.coord === "object" ? req.spec.coord.space : null;
  let turn = existing?.turn ?? DEFAULT_TURN;
  let tilt = existing?.tilt ?? DEFAULT_TILT;

  // The angle the plot opened at — which is the angle the *sentence* asked for,
  // not the engine's default. `reset` returns here rather than to 30/25 because
  // the prose around a plot is describing the picture the author chose: a
  // chapter that turns a volcano through four views is making an argument about
  // those four, and a reset that landed somewhere else would quietly contradict
  // the paragraph beside it.
  const opened = { turn, tilt };

  let first = true;
  function draw() {
    req.spec.coord = { space: { turn, tilt } };
    const { ok, notes } = redraw(engine, container, req);
    if (!ok) return false;
    if (first) {
      first = false;
      if (onNotes && notes.length) onNotes(notes);
    }
    if (onView) onView({ turn, tilt });
    return true;
  }

  if (!draw()) return { destroy() {}, view: () => ({ turn, tilt }) };

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
    turn = (turn + (e.clientX - lastX) * degreesPerPixel) % 360;
    // Tilt stops just short of overhead. At exactly 90 the floor collapses to a
    // line and the picture has no depth to read, so the clamp is a guard rail
    // rather than a limitation.
    tilt = Math.max(-89, Math.min(89, tilt - (e.clientY - lastY) * degreesPerPixel));
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
    view: () => ({ turn, tilt }),
    opened,
    reset() {
      turn = opened.turn;
      tilt = opened.tilt;
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
function addControls(container, handle) {
  const bar = document.createElement("div");
  bar.className = "gog-view-controls";
  bar.style.cssText =
    "font:12px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;color:#666;" +
    "text-align:center;margin:-4px 0 12px;display:flex;gap:.75em;" +
    "align-items:center;justify-content:center;flex-wrap:wrap;";

  const hint = document.createElement("span");
  hint.textContent = "drag to rotate";
  hint.style.cssText = "color:#999;";

  const readout = document.createElement("span");
  // `tabular-nums` so the numbers do not shuffle the line's width as they
  // change — a readout that jitters while you drag is harder to read than one
  // digit wider.
  readout.style.cssText = "font-variant-numeric:tabular-nums;";

  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = "reset";
  reset.style.cssText =
    "font:inherit;color:#555;background:#f4f4f6;border:1px solid #d8d8de;" +
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

  bar.append(hint, readout, reset);
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

  // Two reasons to load the engine, and a plot with neither keeps its static
  // SVG and costs nothing — no engine is even fetched for it. A plot in the cube
  // has an angle worth dragging; a plot that names a brush has a bound worth
  // moving. A flat plot with no brush is still the overwhelmingly common case.
  const spatial = isSpatial(request?.spec);
  const brushed = hasBrush(request?.spec);
  if (!spatial && !brushed) return null;

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
      return handle;
    }
    // `show` is created before the handle exists but is only ever called from
    // `onView`, which cannot fire until `attachDrag`'s first draw — so the
    // forward reference is safe and saves attaching the controls twice.
    let show = () => {};
    const handle = attachDrag(engine, container, request, {
      ...options,
      onView: (view) => show(view),
    });
    if (options.controls !== false) show = addControls(container, handle);
    show(handle.view());
    container.style.cursor = "grab";
    container.dataset.gogInteractive = "true";
    return handle;
  } catch (e) {
    // Never let a missing engine cost the reader the picture. The static SVG is
    // already on the page and stays there; the failure goes to the console,
    // where someone debugging the page will find it.
    console.warn("gog: interactive engine unavailable, plot stays static —", e);
    return null;
  }
}
