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
 * the five their own place. A slider needs two ends, and a column with no stated
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

  // **The frame count is read off `keyTimes`, never counted.** Each frame group
  // carries one `<animate>`, and a played plot has more than one group per
  // frame: the marks are one and the strip naming the moment is another. So
  // counting elements overcounts by however many guides the plot happens to
  // draw. `keyTimes="0;1/nframes"` states the number outright, and `dur` is
  // `nframes` frames long, which gives the length of one.
  const first = svg.querySelector('animate[attributeName="display"]');
  if (!first) return null;
  const share = Number.parseFloat((first.getAttribute("keyTimes") || "").split(";")[1]);
  const span = Number.parseFloat(first.getAttribute("dur"));
  if (!(share > 0) || !(span > 0)) return null;
  const frames = Math.round(1 / share);
  if (frames < 2) return null;
  const seconds = span / frames;

  // Which frame is on screen. The clock runs past `dur` and wraps, so the
  // remainder is the position within one pass, and the floor of it is the frame.
  const at = () => {
    const svgNow = clock();
    if (!svgNow) return 0;
    const t = svgNow.getCurrentTime() % span;
    return Math.min(frames - 1, Math.max(0, Math.floor(t / seconds)));
  };
  // Land in the *middle* of a frame's window rather than on its leading edge,
  // which is a boundary two frames can both claim to a rounding error.
  const show = (i) => clock()?.setCurrentTime(((((i % frames) + frames) % frames) + 0.5) * seconds);

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
      const begin = Number.parseFloat(a.getAttribute("begin"));
      const which = Number.isFinite(begin) ? Math.round(begin / seconds) : -1;
      group?.setAttribute?.("display", which === now ? "inline" : "none");
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
 * @returns {{destroy: () => void, reset: () => void}|null} `null` when the
 *   container is missing or controls were turned off.
 */
export function mountView(target, options = {}) {
  const container =
    typeof target === "string" ? document.getElementById(target) : target;
  if (!container || options.controls === false) return null;

  const view = attachView(container, options);
  const bar = controlBar("view");
  const refresh = addViewControls(bar, view);
  // A played plot needs no engine — the frames are in the file and a clock walks
  // them — so this is where most sequences get their transport, beside the five
  // rather than under them. It returns null and costs nothing on a still plot.
  addTransport(bar, container, view);
  placeBar(container, bar);

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
    },
    reset() {
      view.reset();
      tick();
    },
  };
}
