// view.js — looking at a picture, without redrawing it
//
// **The half of the interaction layer that needs no engine.** Zooming scales the
// SVG's `viewBox` and panning translates it; neither asks the engine anything,
// because neither changes what was drawn. That is what makes this file separable
// from `interactive.js`, and separating it is not tidiness — it is 8 KB against
// 88 KB for a plot that only wants to look closer.
//
// The seam is the same one the engine gate follows. A drag that turns a cube and
// a drag that moves a brush both re-render, so both need WebAssembly. A drag that
// moves the window does not. `interactive.js` imports this file; nothing here
// imports it back.
//
// The division is also the grammar's. Interrogating the *data* — selecting rows,
// reading the one under the pointer — is `brush`, and it earns a word because it
// states something a printed page can show. Looking closer changes how you look
// and not what the plot claims, so it earns no word and is always available.

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

  // Anything that has to move when the window does. Zoom, pan, fit and the
  // re-application after a redraw all end here, so one list covers every way the
  // picture can shift under something anchored to it.
  const watchers = new Set();
  // What draws itself into the copy the camera writes. Empty on every plot that
  // has nothing outside the picture, which is most of them.
  const pens = new Set();

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
    // Each in its own try, because a watcher that throws must not cost the
    // reader the zoom it was watching.
    for (const fn of watchers) {
      try {
        fn();
      } catch {
        /* a watcher's problem is not the view's */
      }
    }
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
    /// Run this whenever the window moves, and hand back a way to stop. What
    /// wants it is anything positioned against the picture rather than against
    /// the page: it is written in the picture's own units and has to be
    /// re-projected every time those units land somewhere new on the screen.
    onApply(fn) {
      watchers.add(fn);
      return () => watchers.delete(fn);
    },
    /// The picture itself, for the one control that wants the element rather
    /// than the window over it. Looking closer moves the window; saving copies
    /// what the window currently frames, so it needs the SVG.
    svg: svgEl,
    /// Add something to the copy the camera is about to write, without this
    /// file learning what it is.
    ///
    /// **Everything a reader can see is not always inside the `<svg>`.** A stamp
    /// is an HTML card on `document.body`, put there so a redraw cannot destroy
    /// it, and the camera used to write a picture without the annotations the
    /// reader had just arranged on it. It cannot be moved into the element
    /// without giving that up, so instead whoever owns it draws it into the copy.
    ///
    /// This file stays ignorant on purpose. It loads for every plot, including
    /// the ones with no engine and no selection, and a zoom control that knew
    /// what a stamp was would be the wrong shape.
    onSave(fn) {
      pens.add(fn);
      return () => pens.delete(fn);
    },
    decorate(clone) {
      for (const pen of pens) pen(clone);
    },
    zoomed: () => scale !== 1,
    /// Whether a further step in each direction would change anything. The bar
    /// reads these to gray a button out rather than offer one that does nothing.
    canZoomIn: () => scale < maxScale,
    canZoomOut: () => scale > 1,
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
 * The one style a control in a bar wears.
 *
 * **The bar takes its color from the page, and must.** A plot is drawn into
 * whatever is hosting it, and the host decides whether that is a light page or a
 * dark one: a browser with a theme switch, JupyterLab, VS Code, Positron,
 * RStudio. None of them tell us which, and a plot cannot ask.
 *
 * These were `#555` on a `#ccc` border, which is legible on white and close to
 * invisible on anything dark, so every reader in a dark editor had buttons
 * they could not see. `inherit` is the fix rather than a media query:
 * `prefers-color-scheme` reports the *operating system's* preference, and a dark
 * JupyterLab theme on a light desktop is exactly the case it gets wrong.
 * Inheriting follows the text beside it, so the icons are legible wherever the
 * surrounding words are, which is the only guarantee worth having here.
 *
 * The border is `currentColor` thinned down. `color-mix` is stated second so a
 * renderer that does not know it keeps the solid border rather than none, and
 * opacity is deliberately left alone: it is what marks a button disabled.
 *
 * **One constant because there is one control.** The view buttons, the drag
 * modes and the table's pager each wrote this string out by hand, which is the
 * defect `controlBar` was written to fix one level up, arriving again in the
 * buttons *inside* the bar. Three hand-copied strings is how two of them quietly
 * stop matching. A use that needs different padding states it after this and
 * lets the later rule win, rather than copying the whole thing to change one
 * number.
 */
export const BUTTON_STYLE =
  "font:inherit;color:inherit;background:none;" +
  "border:1px solid currentColor;" +
  "border-color:color-mix(in srgb, currentColor 34%, transparent);" +
  "border-radius:3px;padding:.15em .3em;cursor:pointer;line-height:0;";

/** How long the pointer rests before a control says what it is. Long enough
 *  that crossing the bar to reach the camera does not trail four labels behind
 *  it, short enough that a reader who stopped to ask gets an answer. */
const DWELL = 300;

/**
 * Black or white, whichever can be read on `fill`.
 *
 * Relative luminance by WCAG, then the larger of the two contrast ratios rather
 * than a threshold, so a page whose text is mid-gray gets the better of the pair
 * instead of whichever side of a line it happened to fall.
 */
function readableOn(fill) {
  const rgb = /(\d+(?:\.\d+)?)[,\s]+(\d+(?:\.\d+)?)[,\s]+(\d+(?:\.\d+)?)/.exec(fill || "");
  if (!rgb) return null;
  const chan = (v) => {
    const c = Number(v) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  const l = 0.2126 * chan(rgb[1]) + 0.7152 * chan(rgb[2]) + 0.0722 * chan(rgb[3]);
  // Against black the ratio is (l + .05) / .05; against white it is 1.05 / (l + .05).
  return (l + 0.05) / 0.05 >= 1.05 / (l + 0.05) ? "#000" : "#fff";
}

/**
 * Give a control with no word on it a label the pointer can ask for.
 *
 * **An icon is only recognizable to a reader who has met it before**, and this
 * bar is mostly icons: two magnifiers, a frame, a camera, three drag modes, two
 * page arrows and a cross. The browser's own `title` says the same words, and
 * says them after about a second in a box the page does not control, which is
 * long enough that a reader who paused to ask has usually moved on. So the
 * answer is drawn here instead, and `title` is not set beside it — two tooltips
 * for one control is what leaving both would give.
 *
 * A button carrying a word gets none of this. `clear` and `show rows` have
 * already said what they are, and a bubble repeating them is noise.
 *
 * **The label inverts the bar rather than picking a color.** The controls
 * inherit theirs from the page for the reason above, and a filled bubble cannot
 * inherit both halves of that. So it fills with the page's own text color and
 * writes on it in whichever of black or white the eye can read there: both are
 * then correct by construction, in a dark editor on a light desktop as much as
 * anywhere else. Where there is nothing to read the color from, the `Canvas`
 * pair stands, which the browser guarantees is legible against itself.
 *
 * That is the opposite of what the row readout does, and both are right. This
 * label sits over the *page*, so it follows the page. The readout floats over
 * the *plot*, and a plot is drawn light whatever the host looks like.
 *
 * `mouseenter` rather than `pointerenter`, deliberately. A touch screen has no
 * hover, and a tap that raised a label would leave it standing over the plot
 * with nothing to take it down; a tap presses the control, which is the answer
 * to what it does. Keyboard focus raises it too, since a reader arriving by tab
 * has the same question and no pointer to ask it with.
 *
 * @param {Element} el the control to label
 * @param {string} text what it does, in the reader's words
 */
export function hoverLabel(el, text) {
  // What a screen reader announces, whether or not a pointer ever rests here.
  // An attribute rather than a child, so rewriting the control's drawing — which
  // the camera does, swapping it for a tick — cannot take the label with it.
  el.setAttribute("aria-label", text);

  let waiting = null;
  const hide = () => {
    clearTimeout(waiting);
    waiting = null;
    if (showing === el) drop();
  };
  const show = () => {
    if (waiting || showing === el) return;
    waiting = setTimeout(() => {
      waiting = null;
      raise(el, text);
    }, DWELL);
  };

  el.addEventListener("mouseenter", show);
  el.addEventListener("mouseleave", hide);
  // A press has answered the question. Leaving the label up would put it over
  // whatever the press just changed.
  el.addEventListener("mousedown", hide);
  el.addEventListener("focus", show);
  el.addEventListener("blur", hide);
  return hide;
}

/**
 * One label for the whole page, and which control has it raised.
 *
 * Only one can show at a time, so a single element that moves is fewer nodes
 * than eleven that wait, and there is nothing per-control to clean up when a
 * plot is destroyed.
 */
let bubble = null;
let showing = null;

/** Take the label down, whoever raised it. */
function drop() {
  bubble?.remove();
  showing = null;
}

/**
 * Put the label on `el`, in the window rather than in the page.
 *
 * **Fixed to the viewport and parented to the body, for the reason the row
 * readout already is.** Positioned inside the control it describes, it counted
 * toward the scrolling area of whatever ancestor was clipping: a Quarto output
 * cell sets `overflow-x`, and CSS computes `overflow-y` to `auto` along with it,
 * so a label under the *lowest* row of buttons was both cut off at the cell's
 * edge and gave the reader a scrollbar that moved the plot. Nothing fixed to the
 * viewport is in any ancestor's flow or overflow, so neither can happen.
 */
function raise(el, text) {
  if (!bubble) {
    bubble = document.createElement("span");
    bubble.className = "gog-hint";
    bubble.style.cssText =
      "position:fixed;pointer-events:none;z-index:2147483647;" +
      "padding:.32em .5em;border-radius:4px;white-space:nowrap;" +
      "font:11px/1.4 ui-monospace,SFMono-Regular,Menlo,monospace;" +
      "background:CanvasText;color:Canvas;box-shadow:0 1px 4px rgba(0,0,0,.28);";
  }
  bubble.textContent = text;
  const seen = globalThis.getComputedStyle?.(el)?.color;
  const ink = readableOn(seen);
  if (ink) {
    bubble.style.background = seen;
    bubble.style.color = ink;
  }
  // Unconditionally, rather than only when it is detached. Appending a node the
  // document already holds moves it, so this costs nothing and does not depend
  // on the element's memory of a page it may no longer be on.
  document.body.appendChild(bubble);
  showing = el;

  // Measured after it is on the page and filled, because a label is as wide as
  // the words in it and there is no other way to know.
  const box = el.getBoundingClientRect();
  const w = bubble.offsetWidth;
  const h = bubble.offsetHeight;
  // Under the control by preference: these bars sit below their plot, so a label
  // opening upward would cover the picture the reader is holding the pointer
  // still to look at. Above it where the window has no room, which is where the
  // lowest row of the lowest plot on a page ends up.
  const under = box.bottom + 6;
  bubble.style.top =
    under + h <= window.innerHeight ? `${under}px` : `${box.top - 6 - h}px`;
  // Centered on the control, then kept inside the window. A long label on a
  // button near either edge would otherwise push a fixed element off the page
  // and give the reader the scrollbar this placement exists to prevent.
  const mid = box.left + box.width / 2 - w / 2;
  bubble.style.left = `${Math.max(6, Math.min(mid, window.innerWidth - w - 6))}px`;
}

/**
 * The controls every plot gets, appended to whichever bar it already has.
 *
 * A button competes with no gesture, which is why the zoom always gets them
 * while the *drag* has to be earned: the sentence decides what a drag means, so
 * a plot that says `brush` has already given its drag away and pans with a
 * modifier instead.
 *
 * It was `addZoomButtons` while the zoom was all it added, and the name stopped
 * being close when the camera arrived, so it now says what it does: one call,
 * and a bar has the whole set. That matters more than tidiness here — three bars
 * call this, and a control added to one of them by hand is how two bars stop
 * matching.
 */
export function addViewControls(bar, view, onChange = () => {}, handle = null) {
  // Drawn rather than typed, for the reason the selection bar's three modes are:
  // no font carries them, and the same 13px stroke keeps one bar looking like one
  // bar. `currentColor` is what lets a disabled button gray its icon with it,
  // rather than needing a second rule to keep the two in step.
  const icon = (body) =>
    `<svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true" ` +
    `style="display:block;fill:none;stroke:currentColor;stroke-width:1.3">${body}</svg>`;
  // A magnifier for the two that change the magnification, and a frame with its
  // corners drawn in for the one that returns to the whole picture.
  //
  // **A word here is the one piece of English a translated book cannot reach.**
  // The prose around a plot is translated and the grammar deliberately is not,
  // but a button is neither: it is read by the same reader in the same sentence,
  // and `fit` would stay English in all 27 languages. An icon has no language, so
  // this is the same ruling the mode icons already made one bar over.
  const GLASS = `<circle cx="7" cy="7" r="4.4"/><path d="M10.2 10.2 14 14"/>`;
  const ART = {
    out: icon(`${GLASS}<path d="M5 7h4"/>`),
    in: icon(`${GLASS}<path d="M5 7h4M7 5v4"/>`),
    fit: icon(
      `<path d="M2 5.6V2h3.6M14 5.6V2h-3.6M2 10.4V14h3.6M14 10.4V14h-3.6"/>` +
      `<rect x="5.4" y="5.4" width="5.2" height="5.2"/>`
    ),
    // A camera: a body, the raised strip over the lens, and the lens.
    camera: icon(
      `<path d="M1.4 5.6h2.5l1-1.9h4.2l1 1.9h2.5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1` +
      `H1.4a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1z"/>` +
      `<circle cx="8" cy="9.6" r="2.5"/>`
    ),
    // A tick, shown for a moment after the file is written. **A download that
    // opens no dialog needs one**, because on default browser settings the file
    // lands in the downloads folder with no visible event at all, and a reader
    // cannot tell that from a button that did nothing.
    saved: icon(`<path d="M3 8.4 6.4 12 13 4.6"/>`),
  };
  const make = (art, says, act) => {
    const b = document.createElement("button");
    b.type = "button";
    b.innerHTML = art;
    b.style.cssText = BUTTON_STYLE;
    // What names the button for a reader who has not met the icon, and for a
    // screen reader. Its own content is a decorative drawing and says nothing.
    hoverLabel(b, says);
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
    if (handle) handle.setMode(view.zoomed() ? "pan" : (handle.picked?.() ?? "select"));
    refresh();
  };

  const out = make(ART.out, "zoom out", () => { view.zoom(1 / 1.4); follow(); });
  const into = make(ART.in, "zoom in", () => { view.zoom(1.4); follow(); });
  const fit = make(ART.fit, "show the whole plot", () => { view.reset(); follow(); });

  // **The camera saves what the reader is looking at, not what the plot was.**
  // That falls out of how looking closer works rather than needing anything:
  // zoom and pan move the SVG's own `viewBox`, and a cube's angle is already
  // drawn into the element, so copying the element copies the current view.
  //
  // It never grays. The other three can each reach a state where a press would
  // do nothing, and say so; there is always a picture to save.
  let savedFor = null;
  const camera = make(ART.camera, "save as PNG", () => {
    savePng(view.svg?.(), () => {
      camera.innerHTML = ART.saved;
      clearTimeout(savedFor);
      savedFor = setTimeout(() => { camera.innerHTML = ART.camera; }, 1400);
    }, { decorate: view.decorate });
  });

  // **A button that can do nothing says so.** Offering `fit` on a plot already
  // fitted, or `\u2212` at the whole picture, is a control that answers a press with
  // silence \u2014 and a reader cannot tell that from one that is broken. Grayed is
  // the same cue the cube's `reset` has carried since it was written, which is
  // where the three numbers below come from rather than from taste.
  function refresh() {
    for (const [b, live] of [
      [out, view.canZoomOut?.() ?? true],
      [into, view.canZoomIn?.() ?? true],
      [fit, view.zoomed()],
    ]) {
      b.disabled = !live;
      b.style.opacity = live ? "1" : "0.4";
      b.style.cursor = live ? "pointer" : "default";
    }
  }

  // **There is no hand here, and the pointer is why.** A drawing of a hand once
  // sat between `fit` and the camera, as a hint rather than a button: it said
  // that dragging moves the picture, once a reader had looked closer. It was
  // removed because the cursor says the same thing better. `mountView` sets
  // `grab` on the container the moment zooming makes panning possible, so the
  // message arrives under the pointer, at the moment the reader is about to
  // drag, instead of in a bar they are not looking at.
  //
  // Three arguments finished it. It was the one item in this row that was not a
  // button, which cost the book a sentence explaining the exception. A brushed
  // plot already carries a pan mode in its `drag:` switcher, so a brushed and
  // zoomed plot showed the hint and the button for one gesture. And what it told
  // a reader is what everyone tries on a magnified picture unprompted.
  //
  // What went with it is touch, honestly: there is no `grab` cursor on a touch
  // screen and no pinch handling in this file, so a reader there presses `+` and
  // then drags with no cue at all. That was judged the weaker loss, because a
  // dimmed drawing whose label cannot be raised without a hover was never much
  // of a cue on the device that had no cursor.

  // The camera sits last, after the three that change how the picture is looked
  // at. It is the only one that produces something outside the page, so it reads
  // as a separate act rather than a fourth way to move the window.
  bar.append(out, into, fit, camera);
  refresh();
  return refresh;
}

/**
 * Give a played plot the three buttons that work its clock.
 *
 * **This is the medium's control, not the grammar's**, and that is a ruling
 * rather than a shortcut. Every frame of a sequence is already in the file, and
 * a clock decides which one you see; stepping and pausing move that clock. They
 * choose no rows, hide no rows, and change nothing the sentence said, which puts
 * them on the same side of the line as turning a cube. So no atom grows a word
 * for them and no binding changes: the transport is *found* in the drawing, by
 * the markup below, wherever a played plot is mounted.
 *
 * **Three buttons rather than a slider**, and the reason is the one that gave
 * the four their own place. A slider needs two ends, and a column with no stated
 * order has none — the frames of `play(continent)` run in whatever order the
 * rows happened to arrive, so a labeled scale from one continent to another
 * would draw an order the data does not have, with far more authority than a
 * loop claims. A stepper claims nothing. Making it a slider on ordered columns
 * and a stepper on unordered ones was the other way, and it fails a rule this
 * bar already keeps: a control that changes shape with the column is a control
 * the reader has to identify twice.
 *
 * **Nothing displays the moment.** `write_play_strip` already draws a band
 * naming the frame on show, for every played plot, as `play`'s earned guide. A
 * readout here would be the second copy of it.
 *
 * @param {Element} bar the view row, which the buttons join
 * @param {Element} container the element holding the drawn plot
 * @param {object|null} view the window over it, for the camera's sake
 * @returns {{refresh: () => void}|null} `null` when the plot has no clock
 */
export function addTransport(bar, container, view = null) {
  // Re-read on every press rather than captured once. The engine redraws a plot
  // that is *also* brushed or turned, which replaces this element, and a
  // captured reference would leave the buttons driving a picture nobody is
  // looking at.
  const clock = () => container.querySelector("svg");
  const svg = clock();
  if (!svg || typeof svg.pauseAnimations !== "function") return null;

  // **Each frame's window is read off its own `<animate>`, never inferred.** A
  // frame holds from its `begin` for its share of the loop (`keyTimes="0;share"`
  // of `dur`), and frames need not be even: a played column with uneven gaps
  // holds each frame in proportion to the gap it stands for. A played plot has
  // more than one group per frame (the marks are one, the strip naming the
  // moment another), and they share a `begin`, so the windows are collected by
  // their start, which also counts the frames without counting elements.
  const animates = [...svg.querySelectorAll('animate[attributeName="display"]')];
  if (!animates.length) return null;
  const span = Number.parseFloat(animates[0].getAttribute("dur"));
  if (!(span > 0)) return null;
  const windows = new Map();
  for (const a of animates) {
    const begin = Number.parseFloat(a.getAttribute("begin"));
    const share = Number.parseFloat((a.getAttribute("keyTimes") || "").split(";")[1]);
    if (!(begin >= 0) || !(share > 0)) continue;
    const key = begin.toFixed(3);
    if (!windows.has(key)) windows.set(key, { begin, hold: share * span });
  }
  const moments = [...windows.values()].sort((p, q) => p.begin - q.begin);
  const frames = moments.length;
  if (frames < 2) return null;

  // Which frame is on screen. The clock runs past `dur` and wraps, so the
  // remainder is the position within one pass, and the frame is the last one
  // begun by then.
  const at = () => {
    const svgNow = clock();
    if (!svgNow) return 0;
    const t = svgNow.getCurrentTime() % span;
    let i = 0;
    for (let k = 1; k < frames; k++) if (moments[k].begin <= t) i = k;
    return i;
  };
  // Land in the *middle* of a frame's window rather than on its leading edge,
  // which is a boundary two frames can both claim to a rounding error.
  const show = (i) => {
    const m = moments[((i % frames) + frames) % frames];
    clock()?.setCurrentTime(m.begin + m.hold / 2);
  };
  // Which frame an `<animate>` belongs to, by its start.
  const frameOf = (a) => {
    const begin = Number.parseFloat(a.getAttribute("begin"));
    return Number.isFinite(begin) ? moments.findIndex((m) => Math.abs(m.begin - begin) < 5e-4) : -1;
  };

  const solid = (body) =>
    `<svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true" ` +
    `style="display:block;fill:currentColor;stroke:none">${body}</svg>`;
  const BAR = (x) => `<rect x="${x}" y="3.1" width="1.7" height="9.8" rx=".4"/>`;
  const ART = {
    back: solid(`${BAR(2.6)}<path d="M14 3.1v9.8L7.1 8z"/>`),
    forward: solid(`<path d="M2 3.1v9.8L8.9 8z"/>${BAR(11.7)}`),
    play: solid(`<path d="M3.9 2.5v11L13.4 8z"/>`),
    pause: solid(`${BAR(4.6)}${BAR(9.7)}`),
  };

  const make = (art, says, act) => {
    const b = document.createElement("button");
    b.type = "button";
    b.innerHTML = art;
    b.style.cssText = BUTTON_STYLE;
    hoverLabel(b, says);
    b.addEventListener("click", act);
    return b;
  };

  // **Stepping pauses**, which is the convention rather than a derivation: every
  // frame-stepper works this way, because a clock left running carries the
  // reader off the frame they just asked for inside a fifth of a second.
  //
  // **And the ends wrap.** The sequence is written `repeatCount="indefinite"`,
  // so the last frame is *already* followed by the first every time it runs
  // round. A stepper that stopped at the ends would contradict the animation it
  // controls. For a column with no stated order the argument is shorter: the
  // frames are a cycle, so there is no before-the-first to protect.
  const step = (by) => {
    clock()?.pauseAnimations();
    show(at() + by);
    refresh();
  };

  const back = make(ART.back, "step back", () => step(-1));
  const toggle = make(ART.pause, "pause", () => {
    const svgNow = clock();
    if (!svgNow) return;
    if (svgNow.animationsPaused()) svgNow.unpauseAnimations();
    else svgNow.pauseAnimations();
    refresh();
  });
  const forward = make(ART.forward, "step forward", () => step(1));

  function refresh() {
    const paused = clock()?.animationsPaused() ?? false;
    toggle.innerHTML = paused ? ART.play : ART.pause;
    hoverLabel(toggle, paused ? "play" : "pause");
  }

  // **The camera has to be told which moment it is photographing.** It copies
  // the element and serializes it, and no timeline survives that: the copy
  // carries the `display` written on each group before any clock ran, so every
  // saved picture would be the first frame however far the reader had stepped.
  // Rather than a rule about animation, this is the camera's own rule holding —
  // it saves what you are looking at — so the copy is frozen on the frame on
  // show, and its clock is taken out so nothing can move it afterwards.
  view?.onSave?.((clone) => {
    const now = at();
    for (const a of [...clone.querySelectorAll('animate[attributeName="display"]')]) {
      const group = a.parentNode;
      group?.setAttribute?.("display", frameOf(a) === now ? "inline" : "none");
      a.remove();
    }
  });

  // One child of the row rather than three, so the gap that separates the two
  // groups belongs to the group and a narrow plot breaks between them instead of
  // through the middle of the stepper.
  const group = document.createElement("span");
  group.style.cssText =
    "display:inline-flex;gap:.75em;align-items:center;margin-left:1.25em;";
  group.append(back, toggle, forward);
  bar.append(group);
  refresh();
  return { refresh };
}

/**
 * Write what is on screen to a PNG file.
 *
 * **Conversion, never a renderer.** This is the standing rule for anything
 * raster here, and the reason is a scar: a second writer that chose its own
 * ticks and palettes drifted from the first until a binned bar chart drew raw,
 * untransformed rows. Nothing below decides anything. It hands the browser the
 * SVG the reader is already looking at and asks for a bitmap of it, so the file
 * cannot disagree with the plot — it is the same picture in a different
 * container. A `.svg` on disk stays the better artifact where one is accepted,
 * because its text stays text at any size; this is for the reader who has a
 * browser and not the code.
 *
 * `scale` is fixed rather than read from the device. A journal wants 300 DPI,
 * and 3x of an 800x600 canvas is 2400x1800, which is 8 inches wide — clear of
 * the 7.2-inch double-column figure that is the widest common specification.
 * Multiplying by `devicePixelRatio` instead would hand two readers different
 * files from the same button.
 */
export const PNG_SCALE = 3;

/**
 * How large the file comes out, given the canvas and the multiplier.
 *
 * Separated from the writing because it is the one *decision* here, and the one
 * a later edit could change without noticing what it costs. A journal asks for
 * 300 DPI, so the number that matters is inches: 3x of an 800x600 canvas is
 * 2400x1800, which is 8 inches wide and clears the 7.2-inch double-column figure
 * that is the widest common specification. Drop it to 2x and the same plot is
 * 5.3 inches, which no longer covers a full-width figure.
 *
 * @returns {{width: number, height: number}|null} `null` when the element does
 *   not say how big it is, which is the one case there is nothing to compute.
 */
export function pngSize(svg, scale = PNG_SCALE) {
  if (!svg) return null;
  const w = Number(svg.getAttribute("width")) || svg.getBoundingClientRect?.().width;
  const h = Number(svg.getAttribute("height")) || svg.getBoundingClientRect?.().height;
  if (!w || !h) return null;
  return { width: Math.round(w * scale), height: Math.round(h * scale) };
}

export function savePng(svg, done = () => {}, options = {}) {
  if (!svg || typeof document === "undefined") return;
  const size = pngSize(svg, options.scale ?? PNG_SCALE);
  if (!size) return;
  const { width, height } = size;

  // Cloned so the plot on the page is never touched, and given the *target*
  // size. That second part is what makes the file sharp: a browser rasterizes
  // an SVG image at its intrinsic size, so scaling an 800-wide one up on the
  // canvas would enlarge a small bitmap instead of drawing a large picture. The
  // `viewBox` is left exactly as it is, which is what carries the current zoom.
  const clone = svg.cloneNode(true);
  clone.setAttribute("width", String(width));
  clone.setAttribute("height", String(height));
  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  // Anything the reader can see that is not inside the element. It goes on the
  // copy and never on the plot, so nothing here can disturb the picture the
  // reader is still looking at. Written in the *live* element's coordinates,
  // which the copy shares: only `width` and `height` changed above, and the
  // `viewBox` is what fixes the units.
  options.decorate?.(clone);

  const source = new XMLSerializer().serializeToString(clone);
  const svgUrl = URL.createObjectURL(
    new Blob([source], { type: "image/svg+xml;charset=utf-8" })
  );
  const image = new Image();
  image.onload = () => {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    // Every plot draws its own background first, so this only matters for one
    // that somehow does not: a PNG with no background is transparent, and
    // transparent reads as black in most slide software.
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, width, height);
    ctx.drawImage(image, 0, 0, width, height);
    URL.revokeObjectURL(svgUrl);
    canvas.toBlob((blob) => {
      if (!blob) return;
      const href = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = href;
      link.download = options.fileName ?? "plot.png";
      link.click();
      URL.revokeObjectURL(href);
      done();
    }, "image/png");
  };
  image.onerror = () => URL.revokeObjectURL(svgUrl);
  image.src = svgUrl;
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
/**
 * The bar under a plot that carries its controls.
 *
 * **One function because there is one bar.** The cube's angle readout, the
 * selection's mode icons and the zoom buttons all sit in the same strip, and
 * each of them wrote this style out by hand — three copies of one rule, and a
 * fourth arriving with the zoom. A reader must not be able to tell which kind of
 * bar they are looking at, and three hand-copied strings is exactly how two of
 * them quietly stop matching.
 *
 * `kind` is only a hook for a stylesheet that wants to reach one and not the
 * others. Nothing styles them differently today.
 */
export function controlBar(kind) {
  const bar = document.createElement("div");
  bar.className = `gog-${kind}-controls`;
  bar.style.cssText =
    // Inherited for the reason the buttons are: this bar is what they inherit
    // *from*, since `bar.append(out, into, fit, …)` puts them inside it. No
    // `opacity` here for the same reason. Dimming the bar to quiet the readout
    // would dim the four buttons with it, which is the thing being fixed, and
    // it would compound with the 0.4 that marks a button disabled.
    "font:12px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;color:inherit;" +
    // A **positive** top margin, and the reason is a bug rather than taste. It
    // was `-4px`, pulling the bar up into the whitespace an unzoomed plot leaves
    // under its axis labels. Zoom in and that whitespace is gone — the panel
    // fills the frame to its bottom edge — so the buttons ended up sitting
    // against the ink, which reads as the plot covering them. The bar has to
    // clear a picture that reaches the edge, because that is exactly the state a
    // reader is in when they need the buttons most.
    "text-align:center;margin:10px 0 12px;display:flex;gap:.75em;" +
    "align-items:center;justify-content:center;flex-wrap:wrap;";
  return bar;
}

export function placeBar(container, ...bars) {
  const parent = container.parentNode;
  if (!parent) return;
  const wrap = document.createElement("div");
  wrap.className = "gog-plot-with-controls";
  parent.insertBefore(wrap, container);
  wrap.appendChild(container);
  // In the order given, and the order is a decision. The four view buttons go
  // first, because they are the only ones on **every** plot: a flat plot has
  // nothing else, so putting them first is what keeps them in the same place
  // under every picture in the book rather than sliding along as the controls
  // beside them change width. What a plot adds for its own sake comes second.
  for (const bar of bars) wrap.appendChild(bar);
}

/**
 * Give one already-drawn plot its view controls. The whole entry point for a
 * flat plot, and what a binding emits for one.
 *
 * **It takes no spec and no data**, which is the second half of why this file
 * exists. `mount` needs the request because turning a cube and moving a brush
 * both re-render it; looking closer re-renders nothing, so the block a flat plot
 * emits carries a container id and stops. A notebook page went from 88 KB per
 * plot — an inlined engine-side module plus the whole table again as JSON — to
 * one shared module and a line.
 *
 * @param {string|Element} target the container holding the static SVG
 * @param {object} [options]
 * @param {Element[]} [lines] what goes under the view's own bar, in order. A
 *   brushed polar plot puts its count and `show rows` there: its view is the one
 *   it has unbrushed, since no drag there moves the selection.
 * @returns {{destroy: () => void, reset: () => void}|null} `null` when the
 *   container is missing or controls were turned off.
 */
export function mountView(target, options = {}, lines = []) {
  const container =
    typeof target === "string" ? document.getElementById(target) : target;
  if (!container || options.controls === false) return null;
  // Once per container: a script run twice would stack a second control bar.
  if (container.dataset.gogView) return null;
  container.dataset.gogView = "1";

  const view = attachView(container, options);
  const bar = controlBar("view");
  const refresh = addViewControls(bar, view);
  // A played plot needs no engine — the frames are in the file and a clock walks
  // them — so this is where most sequences get their transport, beside the four
  // rather than under them. It returns null and costs nothing on a still plot.
  addTransport(bar, container, view);
  placeBar(container, bar, ...lines);

  // Drag pans, and it needs no button to say so. The selection chapter's rule is
  // that the sentence decides what a drag means; a plot naming no brush has said
  // nothing, so there is one thing left for a drag to be and nothing to choose
  // between. Only once zoomed, because the window is clamped inside the picture
  // and a drag at full extent would answer with silence.
  let from = null;
  const cursor = () => {
    container.style.cursor = view.zoomed() ? (from ? "grabbing" : "grab") : "";
  };
  const onDown = (e) => {
    if (!view.zoomed()) return;
    from = { x: e.clientX, y: e.clientY };
    container.setPointerCapture?.(e.pointerId);
    cursor();
    e.preventDefault();
  };
  const onMove = (e) => {
    if (!from) return;
    view.panBy(e.clientX - from.x, e.clientY - from.y);
    from = { x: e.clientX, y: e.clientY };
  };
  const onUp = (e) => {
    if (!from) return;
    from = null;
    container.releasePointerCapture?.(e.pointerId);
    cursor();
  };
  container.addEventListener("pointerdown", onDown);
  container.addEventListener("pointermove", onMove);
  container.addEventListener("pointerup", onUp);
  container.addEventListener("pointercancel", onUp);

  // The buttons change whether a drag is worth anything, so they refresh the
  // cursor too — otherwise a reader zooms in and the plot still says it cannot be
  // moved until they happen to leave and re-enter it.
  const tick = () => {
    refresh();
    cursor();
  };
  bar.addEventListener("click", tick);
  cursor();

  container.dataset.gogInteractive = "true";
  return {
    destroy() {
      container.removeEventListener("pointerdown", onDown);
      container.removeEventListener("pointermove", onMove);
      container.removeEventListener("pointerup", onUp);
      container.removeEventListener("pointercancel", onUp);
      container.style.cursor = "";
      bar.remove();
      for (const line of lines) line.remove();
    },
    reset() {
      view.reset();
      tick();
    },
  };
}

// ---------------------------------------------------------------------------
// The readout: the row under the pointer
//
// **Here, below `interactive.js`, because two kinds of plot read it.** A brushed
// plot always did, inside `attachBrush`; since 2026-10-15 a plain plot whose
// marks are rows does too (the author: hover "on every plot whose marks are
// rows", reversing the 2026-09-26 "not now"). A plain plot's block loads this file
// alone, with no engine: finding the row under the pointer is arithmetic over
// the table the block carries and the frame the engine wrote (`readPanels`), so a
// host that refuses WebAssembly still reads rows. A helper two modules need moves
// down rather than being copied, which is the rule the second renderer taught.
// ---------------------------------------------------------------------------

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
  // A zero or a negative has no place on a log axis. It came back as `-Infinity`
  // or `NaN`, and a `NaN` met first silenced a whole panel's readout, since no
  // distance is smaller than it. The engine refuses such a value in the column
  // it places, so one arrives only from a table the axis does not measure.
  if (!Number.isFinite(v)) return null;
  const f = (v - axis.from) / (axis.to - axis.from || 1);
  return axis.lo + f * (axis.hi - axis.lo);
}

/** How far from a mark the pointer may rest and still name it: about a glyph's
 *  reach. A stamp is this answer kept, so the same reach decides a stamp. */
export const REACH = 14;

/**
 * The row drawn nearest `at` on one panel, within a glyph's reach, or `null`.
 *
 * `placeOn` for every row the panel drew, keeping the closest. Exported for the
 * reason `placeOn` is: it is arithmetic over the table and the panel, so a test
 * can watch it be wrong without a pointer or a page.
 *
 * The table on the page holds more than the panel drew, and each filter below
 * removes one kind of row that is not on the picture: a row of another cell, of
 * another facet or moment, with a gap the engine dropped, or cut by `limits`.
 *
 * @param {object} panel one parsed panel frame, as `readPanels` reads it
 * @param {{x: number, y: number}} at the pointer, in the panel's own units
 * @param {object} req the `{spec, data}` wire object
 * @param {string|null} now the moment a played panel is showing, or `null`
 */
export function nearestRow(panel, at, req, now = null) {
  // The engine could not promise a position, so there is no honest answer to
  // give. The bar says why the first time a reader asks.
  if (panel.place !== "row" || !at || !panel.x || !panel.y) return null;
  let best = null;
  for (const plot of eachPlot(req.spec)) {
    const df = req.data?.[plot.data];
    if (!df) continue;
    // **Only a plot that places this panel's two columns can have drawn it.** A
    // page is several plots, often over one table, and each cell's rows were
    // placed on the pointed panel's axes alike. A tie then kept the first cell's
    // answer, with the first cell's columns: pointing into a panel whose x is
    // `population` named the row's `gdp`. The positions are asked because the
    // frame carries no cell number, so two cells that place the same two
    // columns still answer as the first.
    const positions = new Set([plot.x?.field, plot.y?.field,
      ...(plot.layers ?? []).flatMap((l) => [l.encodings?.x?.field, l.encodings?.y?.field])]);
    if (!positions.has(panel.x.field) || !positions.has(panel.y.field)) continue;
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
    // A row placed outside the panel is not on the picture: `limits` cut it,
    // or it lies past the axis. Without this a cut row just past the edge was
    // named from inside the panel, within a glyph's reach of where it would
    // have been. Half a pixel of slack keeps a mark drawn on the edge.
    const inside = (px, py) =>
      px >= panel.x0 - 0.5 && px <= panel.x1 + 0.5 &&
      py >= panel.y0 - 0.5 && py <= panel.y1 + 0.5;
    const placed = [];
    for (let i = 0; i < n; i++) {
      if (!drew(i)) continue;
      const px = placeOn(panel.x, get(panel.x.field, i));
      const py = placeOn(panel.y, get(panel.y.field, i));
      if (px === null || py === null || !inside(px, py)) continue;
      placed.push([i, px, py]);
    }
    const shape = shapeOf(plot, panel, df, placed);
    for (const [i, px, py] of placed) {
      // How far the pointer is from the row's place, and from its mark. The
      // two differ only on a bar or a tile, and the first breaks a tie between
      // two marks the pointer is inside, such as two bars of one category.
      const e = (px - at.x) ** 2 + (py - at.y) ** 2;
      const d = shape ? shape.distance(px, py, at) : e;
      if (best === null || d < best.d || (d === best.d && e < best.e)) {
        best = { d, e, px, py, row: named.map((f) => [f, get(f, i)]),
                 // The same row written as the plot writes it: a stated format,
                 // a date. The card shows this one; `row` keeps the stored values.
                 shown: named.map((f) => [f, writeValue(get(f, i), formatFor(plot, f), df.dates?.[f])]) };
      }
    }
  }
  return best && best.d <= REACH * REACH ? best : null;
}

/**
 * **A bar or a tile is the whole rectangle, not one point of it.** A row is
 * placed at its value, which on a bar is the end away from the axis and on a
 * `zone` tile its center, so the card showed within a glyph's reach of that
 * point and nowhere else on the mark (0.6.2's test round). This is the
 * rectangle the engine draws for a row, as arithmetic. A bar's foot is at zero,
 * or at the axis's own start on a log axis, and its width is four fifths of the
 * slot, or what a written `gap` leaves. A tile with no transform stands on two
 * axes of categories and fills its cell. `null` when no such layer of the
 * plot's own table places its rows, and then a row is the point at its place.
 *
 * Which axis measures a bar is the engine's rule, `slot_orient`: a bar lies on
 * its side when `y` holds the categories, or holds dates against numbers on `x`.
 *
 * @param {object} plot one plot of the request, as `eachPlot` gives it
 * @param {object} panel the panel, as `readPanels` reads it
 * @param {object} df that plot's table on the wire
 * @param {Array<[number, number, number]>} placed each drawn row and its place
 * @returns {{distance: (px: number, py: number, at: {x: number, y: number}) => number}|null}
 */
function shapeOf(plot, panel, df, placed) {
  const own = (mark) => (plot.layers ?? []).find((l) =>
    l.mark === mark && !(l.transforms ?? []).length && !l.bounds &&
    (!l.data || l.data === plot.data));
  const step = (axis) => Math.abs(axis.hi - axis.lo) / Math.abs(axis.to - axis.from || 1);
  const past = (v, lo, hi) => (v < lo ? lo - v : v > hi ? v - hi : 0);
  const shapes = [];
  const bar = own("bar");
  if (bar) {
    const dated = (axis) => Boolean(df.dates?.[axis.field]);
    const sideways = panel.y.cats
      ? !panel.x.cats
      : !panel.x.cats && dated(panel.y) && !dated(panel.x);
    const [measure, slot] = sideways ? [panel.x, panel.y] : [panel.y, panel.x];
    const low = Math.min(measure.lo, measure.hi);
    const high = Math.max(measure.lo, measure.hi);
    const foot = measure.log ? measure.lo : Math.min(high, Math.max(low, placeOn(measure, 0)));
    const fill = typeof bar.style?.gap === "number" ? 1 - bar.style.gap : 0.8;
    // A slot of categories is one step of the axis. A measured one is the
    // smallest step between two bars, as the engine measures it, and a lone bar
    // gets a fifth of the axis.
    let width;
    if (slot.cats) {
      width = step(slot) * fill;
    } else {
      const at = placed.map(([, px, py]) => (sideways ? py : px)).sort((a, b) => a - b);
      let least = Infinity;
      for (let k = 1; k < at.length; k++) {
        if (at[k] - at[k - 1] > 1e-9) least = Math.min(least, at[k] - at[k - 1]);
      }
      width = Number.isFinite(least) ? least * fill : Math.abs(slot.hi - slot.lo) * 0.25 * fill;
    }
    shapes.push((px, py, at) => {
      const [middle, end] = sideways ? [py, px] : [px, py];
      const [across, along] = sideways ? [at.y, at.x] : [at.x, at.y];
      const off = Math.max(0, Math.abs(across - middle) - width / 2);
      return off ** 2 + past(along, Math.min(foot, end), Math.max(foot, end)) ** 2;
    });
  }
  if (own("zone") && panel.x.cats && panel.y.cats) {
    const [w, h] = [step(panel.x) / 2, step(panel.y) / 2];
    shapes.push((px, py, at) =>
      past(at.x, px - w, px + w) ** 2 + past(at.y, py - h, py + h) ** 2);
  }
  if (!shapes.length) return null;
  return { distance: (px, py, at) => Math.min(...shapes.map((f) => f(px, py, at))) };
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
export function eachPlot(spec, out = []) {
  if (!spec || typeof spec !== "object") return out;
  const cells = spec.cells ?? spec.plots;
  if (Array.isArray(cells) && cells.length) {
    for (const cell of cells) eachPlot(cell, out);
  } else {
    out.push(spec);
  }
  return out;
}

/**
 * Every panel on the page, in document order, with its two domains parsed: the
 * frame the engine writes into a plot whose rows a page reads (a brushed plot,
 * or one whose block reads the row under the pointer). `data-gog-panel` is the
 * rectangle; each axis states its column, its two ends, its categories or its
 * log base; facets and play say which slice and moment the panel drew.
 *
 * Each panel is measured against **its own** transform rather than the outer
 * `<svg>`'s. A composed page nests one `<svg>` per cell, so a cell's panel
 * rectangle is written in that cell's user space; reading it against the outer
 * element would put every panel but the first in the wrong place.
 */
export function readPanels(container) {
  return Array.from(container.querySelectorAll("[data-gog-panel]")).map((g) => {
    const [x0, y0, x1, y1] = g.getAttribute("data-gog-panel").split(" ").map(Number);
    const axis = (name, lo, hi) => {
      const span = g.getAttribute(`data-${name}`);
      if (!span) return null;
      const [from, to] = span.split(" ").map(Number);
      const cats = g.getAttribute(`data-${name}-cats`);
      const log = g.getAttribute(`data-${name}-log`);
      const drawn = g.getAttribute(`data-${name}-fields`);
      return {
        field: g.getAttribute(`data-${name}-field`),
        // Every column the axis draws, where that is more than one: a
        // `bounds` pair lies along one axis, which therefore names no single
        // field, and a brush on either column moves along it.
        fields: drawn === null ? [] : drawn.split("|"),
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
    const levels = (g.getAttribute("data-play-levels") ?? "").split("|");
    // Frames held unevenly carry their own starts, written exactly as each
    // frame's `<animate begin>` is; evenly held ones carry one length only.
    const begins = (g.getAttribute("data-play-begins") ?? "").split("|").map(Number);
    const loop = Number(g.getAttribute("data-play-loop"));
    const play = playField === null ? null : {
      field: playField,
      levels,
      seconds: Number(g.getAttribute("data-play-seconds")),
      begins: begins.length === levels.length && begins.every(Number.isFinite) && loop > 0
        ? begins : null,
      loop,
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
      // `none` on a frame written only so the page can read rows under the
      // pointer: no brush is declared there, so no drag selects.
      select: g.getAttribute("data-gog-select"),
    };
  });
}

/** Where the pointer is, in this panel's own user space. */
export function pointInPanel(panel, event) {
  const owner = panel.el.ownerSVGElement;
  if (!owner || typeof panel.el.getScreenCTM !== "function") return null;
  const ctm = panel.el.getScreenCTM();
  if (!ctm) return null;
  const pt = owner.createSVGPoint();
  pt.x = event.clientX;
  pt.y = event.clientY;
  return pt.matrixTransform(ctm.inverse());
}

/** Is that point inside the panel? */
export function panelHolds(panel, at) {
  return at !== null && at.x >= panel.x0 && at.x <= panel.x1 &&
    at.y >= panel.y0 && at.y <= panel.y1;
}

// The moment showing now. Every frame is in the document and the clock chooses
// between them: frame `i` is displayed over `[i*s, i*s + s)` and the sequence
// repeats, so the index is the elapsed time divided by one frame's length.
// Frames held in proportion to uneven gaps are found by their own starts
// instead: the last one begun by this point of the loop is the one on show.
// Read off the same `<svg>` a redraw reads and writes, so there is one
// timeline even on a page of nested cells.
export function momentOf(container, panel) {
  if (!panel.play || !(panel.play.seconds > 0)) return null;
  const svg = container.querySelector("svg");
  const t = typeof svg?.getCurrentTime === "function" ? svg.getCurrentTime() : 0;
  const { levels, begins, loop } = panel.play;
  const n = levels.length;
  if (begins) {
    const u = ((t % loop) + loop) % loop;
    let at = 0;
    for (let i = 1; i < n; i++) if (begins[i] <= u) at = i;
    return levels[at];
  }
  return levels[((Math.floor(t / panel.play.seconds) % n) + n) % n];
}

// ---------------------------------------------------------------------------
// Writing a value the way the plot writes it
//
// **A port of `gog-core/src/render/number.rs` (`NumberFormat::label`) and of
// `time::fmt_moment`, pinned to them by a test** that draws `label()` text
// through the engine and compares it with these, value by value. Two copies of a
// rule are a drift surface, which is why the test exists; the readout cannot ask
// the engine, because a plain plot's page loads no engine.
//
// Only what the sentence asks for: a column whose binding states `decimals`,
// `short` or `percent` is written that way, a date column as a date, and every
// other value as the table holds it (ruled 2026-10-15, "only what is asked for").
// ---------------------------------------------------------------------------

/** `x` to `d` places, rounded as Rust's `{:.N}` rounds: from the double's exact
 *  value, ties to even (`toFixed` sends ties away from zero), and with no minus
 *  sign on a value that rounds to zero. */
function fixedPlaces(x, d) {
  if (!Number.isFinite(x)) return "";
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, Math.abs(x));
  const bits = view.getBigUint64(0);
  const exp = Number((bits >> 52n) & 0x7ffn);
  let mant = bits & ((1n << 52n) - 1n);
  let e;
  if (exp === 0) {
    e = -1074;
  } else {
    mant |= 1n << 52n;
    e = exp - 1075;
  }
  // |x| = n / 10^k exactly.
  let n;
  let k;
  if (e >= 0) {
    n = mant << BigInt(e);
    k = 0;
  } else {
    k = -e;
    n = mant * 5n ** BigInt(k);
  }
  let q;
  if (k <= d) {
    q = n * 10n ** BigInt(d - k);
  } else {
    const div = 10n ** BigInt(k - d);
    q = n / div;
    const twice = 2n * (n % div);
    if (twice > div || (twice === div && (q & 1n) === 1n)) q += 1n;
  }
  let text = q.toString();
  if (d > 0) {
    text = text.padStart(d + 1, "0");
    text = `${text.slice(0, -d)}.${text.slice(-d)}`;
  }
  return x < 0 && /[1-9]/.test(text) ? `-${text}` : text;
}

/** The zeros after the point dropped, and the point when nothing is left. */
function trimZeros(text) {
  return text.includes(".") ? text.replace(/0+$/, "").replace(/\.$/, "") : text;
}

/** A label's default number: whole numbers bare, others to three places. */
function labelDefault(v) {
  if (!Number.isFinite(v)) return "";
  if (Number.isInteger(v) && Math.abs(v) < 1e15) return fixedPlaces(v, 0);
  return trimZeros(fixedPlaces(v, 3));
}

const UNITS = [[1e12, "T"], [1e9, "B"], [1e6, "M"], [1e3, "K"], [1, ""]];

/** `number.rs`'s `label_number`: fixed places, or the short form. */
function labelNumber(v, places, short) {
  if (!Number.isFinite(v)) return "";
  if (!short) return places === null || places === undefined ? labelDefault(v) : fixedPlaces(v, places);
  const significant = (x) => {
    if (x === 0) return "0";
    const d = Math.min(10, Math.max(0, 2 - Math.floor(Math.log10(Math.abs(x)))));
    return trimZeros(fixedPlaces(x, d));
  };
  const write = (i) => {
    const [m, unit] = UNITS[i];
    const n = places === null || places === undefined ? significant(v / m) : fixedPlaces(v / m, places);
    return `${n}${unit}`;
  };
  let i = UNITS.findIndex(([m]) => Math.abs(v) >= m);
  if (i < 0) i = UNITS.length - 1;
  const written = write(i);
  const front = Number(written.replace(/^-/, "").replace(/[A-Za-z]+$/, ""));
  return front >= 1000 && i > 0 ? write(i - 1) : written;
}

/** `number.rs`'s `percent_text`: one place by default, its zero dropped. */
function percentText(v, places) {
  if (!Number.isFinite(v)) return "";
  const p = v * 100;
  return places === null || places === undefined
    ? `${trimZeros(fixedPlaces(p, 1))}%`
    : `${fixedPlaces(p, places)}%`;
}

/** `time::civil_from_days`: days since 1970-01-01 to (year, month, day). */
function civilFromDays(z0) {
  const z = z0 + 719468;
  const era = Math.floor((z >= 0 ? z : z - 146096) / 146097);
  const doe = z - era * 146097;
  const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365);
  const y = yoe + era * 400;
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100));
  const mp = Math.floor((5 * doy + 2) / 153);
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1;
  const m = mp < 10 ? mp + 3 : mp - 9;
  return [m <= 2 ? y + 1 : y, m, d];
}

/** `time::fmt_moment`: a date as `2024-03-04`, a timestamp with its time. */
function fmtMoment(secs, unit) {
  const day = Math.floor(secs / 86400);
  const [y, m, d] = civilFromDays(day);
  const pad = (n, w = 2) => String(n).padStart(w, "0");
  const date = `${pad(y, 4)}-${pad(m)}-${pad(d)}`;
  if (unit !== "second") return date;
  const tod = Math.round(secs - day * 86400);
  const [h, min, s] = [Math.floor(tod / 3600), Math.floor((tod % 3600) / 60), tod % 60];
  return s === 0 ? `${date} ${pad(h)}:${pad(min)}` : `${date} ${pad(h)}:${pad(min)}:${pad(s)}`;
}

/** The first binding of `field` in this plot that states how its numbers are
 *  written, or `null`: positions, then plot channels, then each layer's. */
export function formatFor(plot, field) {
  const defs = [plot?.x, plot?.y, plot?.z, ...Object.values(plot?.channels ?? {}),
    ...(plot?.layers ?? []).flatMap((l) => Object.values(l.encodings ?? {}))];
  for (const d of defs) {
    if (d && d.field === field &&
        ((d.decimals !== null && d.decimals !== undefined) || d.short === true || d.percent === true)) {
      return { decimals: d.decimals ?? null, short: d.short === true, percent: d.percent === true };
    }
  }
  return null;
}

/**
 * One value as the plot writes it: a date column as a date, a number whose
 * binding states a format in that format, anything else as the table holds it.
 *
 * @param {*} v the stored value
 * @param {{decimals: ?number, short: boolean, percent: boolean}|null} format
 * @param {"day"|"second"|null|undefined} unit the column's date marker
 */
export function writeValue(v, format = null, unit = null) {
  if (typeof v !== "number") return v;
  if (unit) return fmtMoment(v, unit);
  if (format) return format.percent ? percentText(v, format.decimals) : labelNumber(v, format.decimals, format.short);
  return v;
}

/**
 * **Escaped, because a value comes out of the reader's own table.** A column
 * holding `a < b`, or a country name with an ampersand in it, was being pasted
 * into markup, so the contents of a cell decided what the card was made of.
 * Nothing malicious is needed for that to go wrong, only a `<`.
 */
export function escapeText(s) {
  return String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** One row as the card's lines: each column's name, then its value. */
export function rowHtml(row) {
  return row
    .map(([f, v]) => `<div><span style="color:#888">${escapeText(f)}</span> ` +
                     `${escapeText(v)}</div>`)
    .join("");
}

/**
 * The card that names the row under the pointer: one small element on
 * `document.body`, shown beside the mark and kept inside the window.
 */
export function makeTip() {
  let tip = null;
  const hide = () => {
    tip?.remove();
    tip = null;
  };
  const show = (panel, hit) => {
    const owner = panel.el.ownerSVGElement;
    const ctm = panel.el.getScreenCTM();
    if (!owner || !ctm) return hide();
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
        // than text that has to survive an unknown backdrop. A brush's band is
        // fixed for the same reason: it is drawn over the plot, never the page.
        "background:rgba(255,255,255,0.96);border:1px solid #ccc;border-radius:3px;" +
        "padding:.25em .5em;color:#333;box-shadow:0 1px 4px rgba(0,0,0,.12);white-space:nowrap;";
      document.body.appendChild(tip);
    }
    if (!tip.isConnected) document.body.appendChild(tip);
    tip.innerHTML = rowHtml(hit.shown ?? hit.row);
    // Kept inside the viewport, so a point near the right edge does not push a
    // fixed element off the page and give the reader a scrollbar.
    const w = tip.offsetWidth;
    tip.style.left = `${Math.min(at.x + 12, window.innerWidth - w - 8)}px`;
    tip.style.top = `${at.y + 12}px`;
  };
  return { show, hide };
}

/**
 * Read the row under the pointer on a plot that names no brush: the card, and
 * nothing else. A drag stays the view's (it pans once zoomed), and a click does
 * nothing, so a plain plot gains a readout and loses no gesture it had.
 *
 * @param {Element} container the block's plot
 * @param {object} request the `{spec, data}` the block carries, its named
 *   columns only (the binding's `--prune`)
 * @returns {{destroy: () => void}}
 */
export function attachReadout(container, request) {
  const tip = makeTip();
  const onMove = (e) => {
    for (const panel of readPanels(container)) {
      const at = pointInPanel(panel, e);
      if (!panelHolds(panel, at)) continue;
      const hit = nearestRow(panel, at, request, momentOf(container, panel));
      if (hit) return tip.show(panel, hit);
      return tip.hide();
    }
    tip.hide();
  };
  const onLeave = () => tip.hide();
  container.addEventListener("pointermove", onMove);
  container.addEventListener("pointerleave", onLeave);
  return {
    destroy() {
      container.removeEventListener("pointermove", onMove);
      container.removeEventListener("pointerleave", onLeave);
      tip.hide();
    },
  };
}

/**
 * The whole entry point for a plain plot whose marks are rows: the view every
 * plot has, and the row under the pointer. Like `mountView` it needs no engine;
 * unlike it, it takes the request, because naming a row needs the table.
 */
export function mountReadout(target, request, options = {}) {
  const container =
    typeof target === "string" ? document.getElementById(target) : target;
  if (!container) return null;
  const handle = mountView(container, options);
  const readout = attachReadout(container, request);
  return {
    ...(handle ?? {}),
    destroy() {
      readout.destroy();
      handle?.destroy();
    },
    reset() {
      handle?.reset();
    },
  };
}
