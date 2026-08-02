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
 * The three buttons, appended to whichever bar the plot already has.
 *
 * A button competes with no gesture, which is why the zoom always gets them
 * while the *drag* has to be earned: the sentence decides what a drag means, so
 * a plot that says `brush` has already given its drag away and pans with a
 * modifier instead.
 */
export function addZoomButtons(bar, view, onChange = () => {}, handle = null) {
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
  };
  const style =
    "font:inherit;color:#555;background:none;border:1px solid #ccc;" +
    "border-radius:3px;padding:.15em .3em;cursor:pointer;line-height:0;";
  const make = (art, title, act) => {
    const b = document.createElement("button");
    b.type = "button";
    b.innerHTML = art;
    // The title is what names the button for a reader who has not met the icon,
    // and for a screen reader. `aria-label` rather than the text content, because
    // the content is a decorative drawing.
    b.title = title;
    b.setAttribute("aria-label", title);
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
    if (handle) handle.setMode(view.zoomed() ? "pan" : (handle.picked?.() ?? "select"));
    refresh();
  };

  const out = make(ART.out, "zoom out", () => { view.zoom(1 / 1.4); follow(); });
  const into = make(ART.in, "zoom in", () => { view.zoom(1.4); follow(); });
  const fit = make(ART.fit, "show the whole plot", () => { view.reset(); follow(); });

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
      [hand, view.zoomed()],
    ]) {
      // A hint has nothing to disable; only a button does.
      if (b.tagName === "BUTTON") b.disabled = !live;
      b.style.opacity = live ? "1" : "0.4";
      b.style.cursor = live && b.tagName === "BUTTON" ? "pointer" : "default";
    }
  }

  // **The hand is a hint, not a button**, and that is the whole of why it is
  // built differently from the three beside it. A drag on a flat plot already
  // moves the picture and nothing competes for the gesture, so a button would
  // have nothing to switch *to* — pressing it could only do what dragging
  // already does. What a reader is missing is not a control but the knowledge
  // that the gesture exists, and that is what a hint is for. The cube's bar says
  // "drag to rotate" in words for the same reason; this says it without a
  // language, which is the ruling the icons made.
  //
  // It dims with the buttons because panning is impossible at full extent — the
  // window is clamped inside the picture — so it lights up at exactly the moment
  // dragging starts to do something.
  const hand = document.createElement("span");
  hand.title = "drag to move the picture";
  hand.setAttribute("aria-label", "drag to move the picture");
  // The same box as the three buttons, so the bar reads as one row of four
  // rather than three controls and a loose drawing. `cursor:default` is the one
  // difference kept on purpose: it wears the outline but does not claim to be
  // pressable, because there is nothing for a press to do that the drag does not
  // already do.
  hand.style.cssText = style.replace("cursor:pointer", "cursor:default");
  hand.innerHTML = icon(
    // A hand with its fingers curled to grip: a palm, three fingers folded over
    // and a thumb closing from the side.
    `<path d="M5.1 8.3V5.9a1 1 0 0 1 2 0v1.6"/>` +
    `<path d="M7.1 7.2V5.2a1 1 0 0 1 2 0v2"/>` +
    `<path d="M9.1 7.4V6a1 1 0 0 1 2 0v1.9"/>` +
    `<path d="M11.1 7.9v-.8a1 1 0 0 1 2 0v3.3c0 2-1.5 3.4-3.5 3.4h-1` +
    `c-1.4 0-2.2-.5-2.9-1.4L3.2 11c-.5-.7-.3-1.4.3-1.7.5-.3 1.1-.2 1.5.3l.9 1"/>`
  );

  bar.append(out, into, fit, hand);
  refresh();
  return refresh;
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
    "font:12px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;color:#666;" +
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

export function placeBar(container, bar) {
  const parent = container.parentNode;
  if (!parent) return;
  const wrap = document.createElement("div");
  wrap.className = "gog-plot-with-controls";
  parent.insertBefore(wrap, container);
  wrap.appendChild(container);
  wrap.appendChild(bar);
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
  const refresh = addZoomButtons(bar, view);
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
