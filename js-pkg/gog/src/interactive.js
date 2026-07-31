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
    const { svg, error, notes } = renderSpec(engine, req);

    if (error !== null) {
      // A refusal is the engine's to explain. Show it rather than leaving an
      // empty box, and stop — a refused plot has nothing to turn.
      container.textContent = error;
      return false;
    }

    // Read the SMIL clock off the outgoing element before it is destroyed. A
    // `play` plot swaps frames on this timeline, and a fresh element starts at
    // zero — so without this a drag restarts the animation every frame.
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
    container.setPointerCapture?.(e.pointerId);
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
  container.after(bar);
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

  // Only a plot in the cube has an angle worth dragging. A flat plot keeps its
  // static SVG and costs nothing — no engine is even loaded for it.
  if (!isSpatial(request?.spec)) return null;

  try {
    const engine = await engineFor(options.wasm);
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
