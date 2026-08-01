// atoms.js — the vocabulary: marks, transforms, channels, settings
//
// The mirror of `py-pkg/gog/gog/atoms.py` and `r-pkg/gog/R/atoms.R`, and the
// words are the same words: the grammar is the engine's, not the binding's, so
// anything that differs here is a bug in one of the three front ends. What each
// atom *means* is documented once, in the book and the spec; this file only says
// how JavaScript spells it.
//
// **The kernel keeps its spelling, underscores included** (spec §8). Law 3 names
// `_` as the joiner, so `render_svg`, `x_label`, `y_label`, `z_label` and the
// `border_color` / `border_size` setting keys are the *grammar's* words rather
// than an R or Python accident. Re-spelling them camelCase to suit JavaScript's
// habit is the expert's shortcut, which is the one enemy behind all nine
// laws. Every exported name here is the name the other bindings use.
//
// Two spellings do differ, and both are JavaScript's doing:
//
//   * a column is `col.gdp`, never a bare name (see `columns.js`);
//   * a named argument joins one trailing options object — `x(col.gdp,
//     { scale: "log" })` — which is the only shape JavaScript has for
//     `name = value`. Positional arguments stay positional.
//
// The value checks live here rather than in Rust for the reason R's and
// Python's do: the caller gets the error at the line that wrote it, and a
// misspelling never reaches the wire as an enum serde cannot decode. What is
// *legal* — which mark takes which channel, whether this transform means
// anything on that mark — stays in `legality.rs`, where every binding inherits
// it.

import { Column, columnName, describe } from "./columns.js";
import { GogError } from "./errors.js";
import { epochSeconds } from "./render.js";
import { Atom, asAtom, bareAtom, callableAtom } from "./spec.js";

// ---------------------------------------------------------------------------
// Arguments: positional, then one trailing options object
// ---------------------------------------------------------------------------

function isOptions(value) {
  return (
    value !== null &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    !(value instanceof Column) &&
    !(value instanceof Date) &&
    !asAtom(value)
  );
}

function quoted(names) {
  return names.map((name) => `\`${name}\``).join(", ");
}

// An unknown key is refused rather than ignored, because a setting that does
// nothing and says nothing is the silent drop §12 forbids — and a typo in an
// options object is exactly how one arrives.
function readArgs(raw, atom, names) {
  const out = {};
  let taken = 0;
  for (const value of raw) {
    if (isOptions(value)) {
      for (const [key, given] of Object.entries(value)) {
        if (!names.includes(key)) {
          throw new GogError(
            `gog: \`${atom}()\` has no \`${key}\` — it takes ${quoted(names)}.`
          );
        }
        out[key] = given;
      }
      continue;
    }
    if (taken >= names.length) {
      throw new GogError(
        `gog: \`${atom}()\` takes ${names.length === 1 ? "one argument" : quoted(names)}. ` +
          `Anything named goes in one trailing object, e.g. \`${atom}(…, { ${names[names.length - 1]}: … })\`.`
      );
    }
    out[names[taken]] = value;
    taken += 1;
  }
  return out;
}

function wholeNumber(value, atom, argument, example) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 1 || !Number.isInteger(value)) {
    throw new GogError(
      `gog: \`${atom}({ ${argument}: … })\` needs one positive whole number, e.g. \`${example}\`.`
    );
  }
  return value;
}

// A named reading, checked for *shape* only — which words exist is the engine's
// question, so every binding forwards the string and one refusal covers all four.
function oneWord(value, argument) {
  if (typeof value !== "string") {
    throw new GogError(
      `gog: \`density({ ${argument}: … })\` takes one word — "shape" or "count".`
    );
  }
  return value;
}

function positive(value, atom, argument, example) {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    throw new GogError(
      `gog: \`${atom}({ ${argument}: … })\` needs one positive number, e.g. \`${example}\`.`
    );
  }
  return value;
}

// ---------------------------------------------------------------------------
// Marks — the geometric forms
// ---------------------------------------------------------------------------

export const point = bareAtom("mark", { mark: "point" });
export const line = bareAtom("mark", { mark: "line" });
export const path = bareAtom("mark", { mark: "path" });
export const rule = bareAtom("mark", { mark: "rule" });
export const zone = bareAtom("mark", { mark: "zone" });
export const area = bareAtom("mark", { mark: "area" });
export const bar = bareAtom("mark", { mark: "bar" });
export const step = bareAtom("mark", { mark: "step" });
export const interval = bareAtom("mark", { mark: "interval" });
export const ribbon = bareAtom("mark", { mark: "ribbon" });
export const text = bareAtom("mark", { mark: "text" });
// A sheet through the samples, and the one mark that draws in the cube alone. Its
// rows are nodes: the grid the two position columns describe is recovered rather
// than declared, so it wants one row per (x, y) crossing. Three positions, all
// required and all numeric — a face asserts every value *between* two nodes, and
// between two categories there is nothing to assert (for a mesh over categories,
// `layer(bar, bin)` in `space()`). One transform, `density`, which makes it the
// third geometry of one field: a `zone` paints it as cells, a `path` traces its
// contours, a `surface` raises it with the estimate as height.
export const surface = bareAtom("mark", { mark: "surface" });

// `box` — the box-and-whisker mark, with its one knob.
export const box = callableAtom(new Atom("mark", { mark: "box" }), (...raw) => {
  const { whiskers } = readArgs(raw, "box", ["whiskers"]);
  if (whiskers !== undefined && whiskers !== "tukey" && whiskers !== "range") {
    throw new GogError(
      'gog: `box({ whiskers: … })` is either "tukey" (the default — whiskers to ' +
        '1.5*IQR, points beyond drawn as outliers) or "range" (whiskers to the ' +
        "true min and max, no outliers)."
    );
  }
  const atom = new Atom("mark", { mark: "box" });
  if (whiskers !== undefined) atom.fields.box = { whiskers };
  return atom;
});

// ---------------------------------------------------------------------------
// Transforms — used inside layer():  layer(bar, bin),  layer(line, smooth)
// ---------------------------------------------------------------------------

export const smooth = bareAtom("transform", { transform: "smooth" });
export const count = bareAtom("transform", { transform: "count" });
export const sum = bareAtom("transform", { transform: "sum" });
export const mean = bareAtom("transform", { transform: "mean" });
export const median = bareAtom("transform", { transform: "median" });
export const max = bareAtom("transform", { transform: "max" });
export const min = bareAtom("transform", { transform: "min" });
export const proportion = bareAtom("transform", { transform: "proportion" });
export const range = bareAtom("transform", { transform: "range" });
export const dodge = bareAtom("transform", { transform: "dodge" });

// `stack` — the measure-axis pile. `stack({ share: true })` fills every pile to
// 1: the 100% stacked bar. A parameter here rather than a second reading of
// `proportion` because the two divide by different totals — `proportion` by the
// whole frame's, this by the slot's own — and because it composes with any
// measurement, including a `sum` that `proportion` has no column to take.
//
// `stack({ baseline: … })` says where each pile *hangs*, the other free choice
// once the heights are fixed: "zero" stands every pile on the axis (the
// default), "center" hangs each so its middle is at zero, "wiggle" chooses the
// foot that makes the bands as flat as it can — the streamgraph. Orthogonal to
// `share`, which scales the heights rather than placing them.
export const stack = callableAtom(
  new Atom("transform", { transform: "stack" }),
  (...raw) => {
    const { share, baseline } = readArgs(raw, "stack", ["share", "baseline"]);
    if (share !== undefined && typeof share !== "boolean") {
      throw new GogError(
        "gog: `stack({ share: … })` is true or false — true fills every pile to 1 " +
          "(the 100% stacked bar), false piles the values themselves. For shares of " +
          "the whole plot rather than of each slot, `proportion` is the transform you want."
      );
    }
    if (baseline !== undefined && typeof baseline !== "string") {
      throw new GogError(
        'gog: `stack({ baseline: … })` is one of "zero", "center" or "wiggle" — ' +
          '"zero" stands every pile on the axis, "center" hangs each pile so its ' +
          'middle is at zero, "wiggle" chooses the foot that makes the bands as flat ' +
          "as it can (the streamgraph)."
      );
    }
    return new Atom("transform", {
      transform: "stack",
      share: share ?? null,
      baseline: baseline ?? null,
    });
  }
);

// `bin` — equal-width buckets. How many dimensions it cuts is the mark's answer.
export const bin = callableAtom(new Atom("transform", { transform: "bin" }), (...raw) => {
  const { bins, width, tiling } = readArgs(raw, "bin", ["bins", "width", "tiling"]);
  if (bins !== undefined && width !== undefined) {
    throw new GogError(
      "gog: `bin()` takes either `bins` or `width`, not both. Write `bin(30)` for a " +
        "bin count or `bin({ width: 5 })` for a bin width."
    );
  }
  if (tiling !== undefined && typeof tiling !== "string") {
    throw new GogError('gog: `bin({ tiling: … })` needs one name, `"rect"` or `"hex"`.');
  }
  return new Atom("transform", {
    transform: "bin",
    bins: bins === undefined ? null : wholeNumber(bins, "bin", "bins", "bin(30)"),
    width: width === undefined ? null : positive(width, "bin", "width", "bin({ width: 5 })"),
    tiling: tiling ?? null,
  });
});

// `density` — the smooth estimate; `levels` cuts a field into bands; `compare`
// says what a violin's width means from one slot to the next.
export const density = callableAtom(
  new Atom("transform", { transform: "density" }),
  (...raw) => {
    const { adjust, bandwidth, levels, compare, reach } = readArgs(raw, "density", [
      "adjust",
      "bandwidth",
      "levels",
      "compare",
      "reach",
    ]);
    if (adjust !== undefined && bandwidth !== undefined) {
      throw new GogError(
        "gog: `density()` takes either `adjust` or `bandwidth`, not both. Write " +
          "`density(2)` to scale the automatic bandwidth, or `density({ bandwidth: 5 })` " +
          "to set it in the data's own units."
      );
    }
    return new Atom("transform", {
      transform: "density",
      adjust: adjust === undefined ? null : positive(adjust, "density", "adjust", "density(2)"),
      bandwidth:
        bandwidth === undefined
          ? null
          : positive(bandwidth, "density", "bandwidth", "density({ bandwidth: 5 })"),
      levels:
        levels === undefined
          ? null
          : wholeNumber(levels, "density", "levels", "layer(path, density({ levels: 8 }))"),
      // One of two words, checked here only for *shape* — which words exist is the
      // engine's question, so a typo gets one message in all four bindings rather
      // than four (`legality::check_density_params`).
      compare: compare === undefined ? null : oneWord(compare, "compare"),
      reach:
        reach === undefined
          ? null
          : positive(reach, "density", "reach", "density({ reach: 2.5 })"),
    });
  }
);

// `confidence` — the mean's interval per group, 0.95 unless told otherwise.
export const confidence = callableAtom(
  new Atom("transform", { transform: "confidence" }),
  (...raw) => {
    const { level } = readArgs(raw, "confidence", ["level"]);
    if (
      level !== undefined &&
      (typeof level !== "number" || !Number.isFinite(level) || level <= 0 || level >= 1)
    ) {
      throw new GogError(
        "gog: `confidence({ level: … })` needs one number strictly between 0 and 1, " +
          "e.g. `confidence(0.95)`."
      );
    }
    return new Atom("transform", {
      transform: "confidence",
      level: level ?? null,
    });
  }
);

// `jitter` — the categorical-axis spread, a multiple of the default.
export const jitter = callableAtom(
  new Atom("transform", { transform: "jitter" }),
  (...raw) => {
    const { amount } = readArgs(raw, "jitter", ["amount"]);
    if (
      amount !== undefined &&
      (typeof amount !== "number" || !Number.isFinite(amount) || amount < 0)
    ) {
      throw new GogError(
        "gog: `jitter({ amount: … })` needs one non-negative number — the spread as a " +
          "multiple of the default, e.g. `jitter(0.5)` for half or `jitter(2)` for double."
      );
    }
    return new Atom("transform", {
      transform: "jitter",
      amount: amount ?? null,
    });
  }
);

// Pre-computed bounds: `lower`/`upper` bound the measure axis, `start`/`end` the
// domain. Every argument names a column, which is why this atom reads worst of
// the fifty in a string spelling and reads like the rest with the accessor —
// `{ start: col.start }` says which half is a column where `{ start: "start" }`
// could not (spec §8).
export function bounds(...raw) {
  const given = readArgs(raw, "bounds", ["lower", "upper", "start", "end"]);
  const keys = ["lower", "upper", "start", "end"];
  if (keys.every((key) => given[key] === undefined)) {
    throw new GogError(
      "gog: `bounds()` needs column names — `bounds(col.lo, col.hi)` bounds the " +
        "measure axis, and on a `zone` `bounds({ start: col.a, end: col.b })` bounds " +
        "the domain axis."
    );
  }
  const fields = { transform: "bounds" };
  for (const key of keys) {
    fields[key] = given[key] === undefined ? null : columnName(given[key], "bounds");
  }
  return new Atom("transform", fields);
}

/**
 * Divide a whole among nested parts — one ring per level of a hierarchy.
 *
 * The hierarchy arrives as **columns**, outermost first: one row of the table is
 * one leaf, and `partition(col.group, col.item, col.detail)` says which columns
 * spell the path down to it. A blank level ends that branch early, which is what
 * gives a real hierarchy its ragged rim.
 *
 * `layer(zone, partition(...))` flat is the icicle; the same sentence with
 * `polar()` is the sunburst. `layer(text, partition(...))` with `label(col.name)`
 * names each node where it sits. What each branch is weighed by rides on `x`;
 * bind nothing and every leaf weighs 1.
 *
 * `{ cross: true }` turns the levels across each other instead of down one axis:
 * the first divides the width, the second divides the height within each of those
 * columns, which is the mosaic. It rides the trailing options object every other
 * atom's parameters ride, so the levels stay a plain variadic list.
 */
export function partition(...levels) {
  let options = {};
  if (levels.length && isOptions(levels[levels.length - 1])) {
    options = levels.pop();
  }
  if (levels.length === 0) {
    throw new GogError(
      "gog: `partition()` needs the hierarchy's columns, outermost first — " +
        "`partition(col.group, col.item, col.detail)` puts `group` on the " +
        "innermost ring and `detail` on the rim."
    );
  }
  const { cross = false, ...rest } = options;
  const unknown = Object.keys(rest);
  if (unknown.length) {
    throw new GogError(
      "gog: `partition()` takes `cross` — `partition(col.decade, col.theme, " +
        `{ cross: true })\` is the mosaic. Got: \`${unknown.join("`, `")}\`.`
    );
  }
  if (typeof cross !== "boolean") {
    throw new GogError(
      "gog: `partition({ cross })` is true or false — true crosses the levels " +
        "(the mosaic: the first divides the width, the second the height within " +
        "each column), false nests them down one axis (the icicle, and the " +
        "sunburst in `polar()`)."
    );
  }
  return new Atom("transform", {
    transform: "partition",
    levels: levels.map((level) => columnName(level, "partition")),
    // Sent only when true, so a nested partition's wire form is byte-identical to
    // what it was before this existed — `carry` drops an `undefined`.
    cross: cross ? true : undefined,
  });
}

// ---------------------------------------------------------------------------
// Positions and coordinate spaces — always the plot's, unless a layer says so
// ---------------------------------------------------------------------------

const SCALE_NAMES = ["linear", "log", "time", "category"];

function checkScale(scale) {
  if (scale === undefined || scale === null) return null;
  if (typeof scale !== "string") {
    throw new GogError(
      'gog: `scale` needs a single string, e.g. `x(col.gdp, { scale: "log" })`.'
    );
  }
  if (!SCALE_NAMES.includes(scale)) {
    const names = SCALE_NAMES.map((name) => `"${name}"`).join(", ");
    throw new GogError(`gog: \`scale: "${scale}"\` is not a scale. gog has ${names}.`);
  }
  return scale;
}

function checkBase(base) {
  if (base === undefined || base === null) return null;
  if (typeof base !== "number" || !Number.isFinite(base)) {
    throw new GogError(
      'gog: `base` needs a single number, e.g. `x(col.bits, { scale: "log", base: 2 })`.'
    );
  }
  if (base <= 1) {
    throw new GogError(
      `gog: \`base: ${base}\` is not a base a logarithm can have — it must be greater ` +
        "than 1. Use 10 (the default), 2 for doublings, or `Math.E` for e-foldings."
    );
  }
  return base;
}

// The domain the channel runs over, when the data is not the authority (spec
// §10). Two numbers, either of which may be `null` on its own to leave that end
// to the data: `{ limits: [0, null] }` pins a baseline and lets the top follow.
// JavaScript's `null` is already JSON's, so the spelling and the wire agree.
function checkLimits(limits) {
  if (limits === undefined || limits === null) return null;
  if (!Array.isArray(limits) || limits.length !== 2) {
    throw new GogError(
      "gog: `limits` needs two numbers, e.g. `x(col.hour, { limits: [0, 24] })`. " +
        "Use `null` for an end the data should decide: `[0, null]`."
    );
  }
  const out = limits.map((end) => {
    if (end === null || end === undefined) return null;
    // A domain on a temporal axis is written in dates, not epoch arithmetic:
    // `{ limits: [new Date("2024-01-01"), new Date("2024-12-31")] }`.
    if (end instanceof Date) return epochSeconds(end);
    if (typeof end !== "number" || !Number.isFinite(end)) {
      throw new GogError(
        "gog: `limits` needs two numbers, e.g. `x(col.hour, { limits: [0, 24] })`. " +
          "Use `null` for an end the data should decide: `[0, null]`."
      );
    }
    return end;
  });
  const [lo, hi] = out;
  if (lo !== null && hi !== null && !(lo < hi)) {
    throw new GogError(
      `gog: \`limits: [${lo}, ${hi}]\` runs backwards or has no width — the first ` +
        `number is the low end. Write \`[${Math.min(lo, hi)}, ${Math.max(lo, hi)}]\`.`
    );
  }
  return out;
}

// How many ticks an axis should aim for (spec §10). A *target*, not a promise:
// the count picks a step and the step is then rounded to a human number, so 8 on
// a 0..100 axis gets a step of 10 and nine ticks. Two is the floor — one tick
// shows a place but no direction — and the engine says so as well, because a
// binding is not the only way in.
function checkTickCount(tickCount) {
  if (tickCount === undefined || tickCount === null) return null;
  if (typeof tickCount !== "number" || !Number.isFinite(tickCount)) {
    throw new GogError(
      "gog: `tick_count` needs one number, e.g. `x(col.gdp, { tick_count: 8 })`. " +
        "It is how many ticks the axis aims for.",
    );
  }
  if (!Number.isInteger(tickCount)) {
    throw new GogError(
      `gog: \`tick_count: ${tickCount}\` is not a whole number — an axis cannot ` +
        `have a fraction of a tick. Try \`tick_count: ${Math.round(tickCount)}\`.`,
    );
  }
  if (tickCount < 2) {
    throw new GogError(
      `gog: \`tick_count: ${tickCount}\` — an axis needs at least two ticks to ` +
        "show a direction as well as a place. Ask for 2 or more, or leave " +
        "`tick_count` off for the default of 5.",
    );
  }
  return tickCount;
}

// `free: true` — fit this axis from each panel's own rows (spec §11).
//
// A flag rather than a value, because the rest of the question is answered by
// *where* it was written: `y(col.life, { free: true })` frees y, `x(...)` frees x.
function checkFree(free, name) {
  if (free === undefined || free === null || free === false) return false;
  if (free !== true) {
    throw new GogError(
      `gog: \`free\` is true or false — it says whether this axis is fitted per ` +
        `panel. Which axis is up to which binding you write it on: ` +
        `\`${name}(col.<name>, { free: true })\` frees ${name}.`
    );
  }
  return true;
}

function positionAtom(kind, name, raw) {
  const { field, scale, base, limits, tick_count: tickCount, free } =
    readArgs(raw, name, ["field", "scale", "base", "limits", "tick_count", "free"]);
  return new Atom(kind, {
    field: columnName(field, name),
    scale: checkScale(scale),
    base: checkBase(base),
    limits: checkLimits(limits),
    tick_count: checkTickCount(tickCount),
    free: checkFree(free, name),
  });
}

// Bind the x axis to a column.
export function x(...raw) {
  return positionAtom("coord_x", "x", raw);
}

// Bind the y axis to a column.
export function y(...raw) {
  return positionAtom("coord_y", "y", raw);
}

// Bind the z axis to a column — one more vowel, not a chart type.
export function z(...raw) {
  return positionAtom("coord_z", "z", raw);
}

// `space` — the angle a 3-D plot is viewed from.
export const space = callableAtom(
  new Atom("coord_space", { turn: 30, tilt: 25 }),
  (...raw) => {
    const { turn = 30, tilt = 25 } = readArgs(raw, "space", ["turn", "tilt"]);
    for (const [name, value] of [["turn", turn], ["tilt", tilt]]) {
      if (typeof value !== "number" || !Number.isFinite(value)) {
        throw new GogError(
          `gog: \`space({ ${name}: … })\` needs a single number of degrees, e.g. \`space(45, 20)\`.`
        );
      }
    }
    return new Atom("coord_space", { turn, tilt });
  }
);

// `polar` — the plane bent into a circle: x is the angle, y the radius.
export const polar = callableAtom(new Atom("coord_polar", { start: 0 }), (...raw) => {
  const { start = 0 } = readArgs(raw, "polar", ["start"]);
  if (typeof start !== "number" || !Number.isFinite(start)) {
    throw new GogError(
      "gog: `polar({ start: … })` needs a single number of degrees, e.g. `polar(90)`."
    );
  }
  return new Atom("coord_polar", { start });
});

// `nest` — the panel packed with nested regions: the measure becomes an area.
// No argument, because it has no view to set: `space` and `polar` carry an angle
// you could turn the same picture through, and a packing has nothing underneath
// to turn.
export const nest = callableAtom(new Atom("coord_nest", {}), (...raw) => {
  if (raw.length > 0) {
    throw new GogError("gog: `nest()` takes no arguments — a packing has no view to set.");
  }
  return new Atom("coord_nest", {});
});

// ---------------------------------------------------------------------------
// Channels — they map a column, and earn a legend to decode it
// ---------------------------------------------------------------------------

function scaledChannel(kind) {
  return (...raw) => {
    const { field, scale, base, limits } =
      readArgs(raw, kind, ["field", "scale", "base", "limits"]);
    return new Atom(kind, {
      field: columnName(field, kind),
      scale: checkScale(scale),
      base: checkBase(base),
      limits: checkLimits(limits),
    });
  };
}

function plainChannel(kind) {
  return (...raw) => {
    const { field } = readArgs(raw, kind, ["field"]);
    return new Atom(kind, { field: columnName(field, kind) });
  };
}

function checkSpeed(speed) {
  if (speed === undefined || speed === null) return null;
  if (typeof speed !== "number" || !Number.isFinite(speed)) {
    throw new GogError(
      "gog: `speed` needs a single number, e.g. `play(col.year, { speed: 2 })`. " +
        "It is how many times faster than normal the frames run."
    );
  }
  if (speed <= 0) {
    throw new GogError(
      `gog: \`speed: ${speed}\` — a speed is a multiple of the normal pace, so it has ` +
        "to be above zero. `speed: 2` is twice as fast, `speed: 0.5` half."
    );
  }
  return speed;
}

// Map fill/stroke color to a column.
export const color = scaledChannel("color");
// The British spelling of `color()`, refused with direction. gog writes
// American English throughout and accepts no second spelling — Law 2 applied
// to the vocabulary itself, since two ways to write one word is a silent
// letter the reader pays for. ggplot2 accepts both, so a reader arriving from
// there types `colour`; unexported it would be `undefined is not a function`,
// which names no fix. Exported for the same reason `facet` is (spec §13).
export function colour() {
  throw new GogError(
    "gog: there is no `colour()` channel. gog spells it `color(col.<name>)`: " +
      "American English is the grammar's only spelling, and unlike ggplot2 " +
      "there is no British alternative."
  );
}
// Map size to a numeric column.
export const size = scaledChannel("size");
// Map opacity to a numeric column.
export const opacity = scaledChannel("opacity");
// Group a line/path by a column, without giving each group a color.
export const group = plainChannel("group");
// Map glyph shape to a categorical column.
export const shape = plainChannel("shape");
// Map paint texture to a categorical column — `shape`'s twin.
export const pattern = plainChannel("pattern");
// Draw a column's values as text — the `text` mark's content.
export const label = plainChannel("label");
// Cut the plot into frames and play them — the time dimension.
//
// `play` is `facet` read in time. Both split the rows by a column's distinct
// values; `across(plot, facet(col.continent))` lays the pieces out over the page
// and `play(col.year)` lays them out in sequence. Every scale, the color map and
// every legend are fitted across the whole sequence rather than per frame, so the
// axes hold still and only the data moves; a layer that does not bind `play` is
// drawn in every frame. A static image made from the plot shows the first frame.
export const play = (...raw) => {
  const { field, speed } = readArgs(raw, "play", ["field", "speed"]);
  return new Atom("play", { field: columnName(field, "play"), speed: checkSpeed(speed) });
};

// What `at` was given, and which of the two readings it is. One option rather
// than two, because the *value* answers the question the way a column answers it
// everywhere else in this grammar: numbers are a range, names are a set of slots.
const checkBrushAt = (at) => {
  if (at === undefined || at === null) return {};
  const seq = typeof at === "string" ? [at] : Array.from(at ?? []);
  if (seq.length > 0 && seq.every((v) => typeof v === "string")) return { levels: seq };
  if (seq.length !== 2 || !seq.every((v) => typeof v === "number" && Number.isFinite(v))) {
    throw new GogError(
      "gog: `at` is where the selection opens: two numbers on a column that " +
      "measures, e.g. `brush(col.gdp, { at: [1200, 45000] })`, or the names to " +
      "select on a column of categories.",
    );
  }
  return { at: [seq[0], seq[1]] };
};

// Let the reader select rows, and push back the rest.
//
// `brush` puts a bound on one column's values. Rows inside it keep the plot's
// colors; rows outside it are dimmed, so a selection is read against what it was
// taken from. **It highlights and never removes rows** — removing rows before the
// statistics run is what `limits` does, on the binding, and it counts what it
// dropped. One column per `brush`; write two for a rectangle.
export const brush = callableAtom(new Atom("brush", { field: "" }), (...raw) => {
  const { field, at } = readArgs(raw, "brush", ["field", "at"]);
  const name = field === undefined || field === null ? "" : columnName(field, "brush");
  return new Atom("brush", { field: name, ...checkBrushAt(at) });
});

// ---------------------------------------------------------------------------
// Settings — they fix a value, map nothing, and earn no legend (spec §7)
// ---------------------------------------------------------------------------

const STYLE_STRINGS = ["color", "shape", "border_color"];
const STYLE_NUMBERS = ["opacity", "size", "border_size"];
const STYLE_FLAGS = ["caps", "center"];
const STYLE_VALUES = {
  nudge: ["up", "down", "left", "right"],
  pattern: ["solid", "dashed", "dotted", "hatch", "crosshatch", "grid", "dots"],
  arrow: ["end", "start", "both"],
  reach: ["panel", "edge"],
};
const STYLE_PROPS = [
  ...STYLE_STRINGS,
  ...STYLE_NUMBERS,
  ...STYLE_FLAGS,
  ...Object.keys(STYLE_VALUES),
];

// The British spelling of a setting, and what gog spells it instead. One entry
// per gog word that has a British form; there are three, and `colour()` the
// channel is the fourth word in the grammar with one.
const BRITISH_SETTINGS = {
  colour: "color",
  border_colour: "border_color",
  centre: "center",
};

// Set constant visual properties on the nearest preceding mark.
//
// Channels *map*: `color(col.species)` asks the reader "which species?" and
// earns a legend to answer it. `style()` *sets*: it fixes a property at one
// value for the whole layer, consumes no scale, and produces no legend — there
// is nothing to decode.
export function style(props) {
  if (props === undefined || props === null || !isOptions(props) || !Object.keys(props).length) {
    throw new GogError(
      "gog: `style()` sets nothing. Name at least one property, e.g. " +
        '`style({ color: "tomato" })`.'
    );
  }

  for (const [name, value] of Object.entries(props)) {
    if (!STYLE_PROPS.includes(name)) {
      // The British spelling and the ordinary typo part on the *message* and
      // not on the check: one names the word to write, the other lists what
      // exists.
      // `Object.hasOwn`, not `in`: `in` walks the prototype chain, so
      // `style({ toString: 1 })` would be answered as a British spelling.
      if (Object.hasOwn(BRITISH_SETTINGS, name)) {
        throw new GogError(
          `gog: \`style({ ${name}: … })\` is not a setting. gog spells it ` +
            `\`${BRITISH_SETTINGS[name]}\`: American English is the grammar's ` +
            "only spelling, and unlike ggplot2 there is no British alternative."
        );
      }
      throw new GogError(
        `gog: \`style({ ${name}: … })\` is not a setting. gog sets: ` +
          `${[...STYLE_PROPS].sort().join(", ")}.`
      );
    }
    // A column where a value belongs — the mirror of a string where a column
    // belongs, and the same §7 distinction seen from the other side.
    if (value instanceof Column) {
      throw new GogError(
        ["color", "size", "opacity", "shape", "pattern"].includes(name)
          ? `gog: \`style({ ${name}: … })\` fixes one value for the whole layer, and ` +
            `\`${value}\` is a column. To *map* it — one value per category, with a ` +
            `legend to decode it — that is a channel: \`${name}(${value})\`.`
          : `gog: \`style({ ${name}: … })\` fixes one value, and \`${value}\` is a column.`
      );
    }
    if (STYLE_STRINGS.includes(name) && typeof value !== "string") {
      throw new GogError(
        `gog: \`style({ ${name}: … })\` needs a single string, e.g. ` +
          `\`style({ ${name}: "tomato" })\`.`
      );
    }
    if (STYLE_NUMBERS.includes(name) && (typeof value !== "number" || !Number.isFinite(value))) {
      throw new GogError(
        `gog: \`style({ ${name}: … })\` needs a single number, e.g. \`style({ ${name}: 0.3 })\`.`
      );
    }
    if (STYLE_FLAGS.includes(name) && typeof value !== "boolean") {
      throw new GogError(`gog: \`style({ ${name}: … })\` needs true or false.`);
    }
    if (STYLE_VALUES[name] && !STYLE_VALUES[name].includes(value)) {
      const allowed = STYLE_VALUES[name].map((v) => `"${v}"`).join(", ");
      throw new GogError(`gog: \`style({ ${name}: … })\` needs one of ${allowed}.`);
    }
  }

  return new Atom("style", { props: { ...props } });
}

// ---------------------------------------------------------------------------
// Plot-level atoms
// ---------------------------------------------------------------------------

// Order the categorical axis by a column.
export function order(...raw) {
  const { field, desc = false } = readArgs(raw, "order", ["field", "desc"]);
  return new Atom("order", {
    field: columnName(field, "order"),
    descending: Boolean(desc),
  });
}

// Set the categorical palette — a name, or a list of hex colors.
export function palette(pal) {
  if (typeof pal === "string") {
    return new Atom("palette", { value: { named: pal } });
  }
  if (Array.isArray(pal) && pal.every((c) => typeof c === "string")) {
    return new Atom("palette", { value: { custom: [...pal] } });
  }
  throw new GogError(
    'gog: `palette()` takes a palette name ("gog", "okabe") or an array of hex ' +
      `colors. Got ${describe(pal)}.`
  );
}

export const THEME_PRESETS = ["gog", "minimal", "bw"];
const GRID_VALUES = ["both", "x", "y", "none"];
const FRAME_VALUES = ["full", "axes", "none"];

// Set the plot's furniture — the page rather than the ink.
//
// Everything here maps no column, so each is a *setting*; but none of it belongs
// to a mark either, which is why it is not `style()`. A layer has no gridlines
// and a plot has no fill, so the two property sets are disjoint, and telling them
// apart by where they were written would make a sub-expression mean different
// things in different places (Law 6). Spec §7 is the ruling.
//
// A named preset comes first and the options object adjusts it, because a preset
// you cannot adjust sends you straight back to asking for knobs.
// `font_size` is how many pixels a tick label is, and through it the size of every
// other piece of text the plot draws — the axis names and the title are a fixed
// step above it, so `11` (the default) gives 11, 13 and 16 while `16` gives 16, 19
// and 23. One number rather than three. It is a measurement, not a multiplier, so
// `font_size: 1.5` is refused, and it names no typeface: the engine measures text
// with its own width table and has none to choose.
// `strip` is the facet strip's fill: the band above a panel that names the level it
// holds. Same colors as `background`. `theme("bw")` sets it white, because a gray
// band reproduces poorly in print, which is the one place that preset is for.
// `strip_text` is the ink of the strip's label. Leave it out and gog picks whichever
// of its two defaults reads on the band, so `theme({ strip: "black" })` already gives
// white type; name it when the ink is a real choice, such as navy with gold type.
// `width` and `height` are how many pixels the plot asks for. Alone that is the
// image; composed onto a page with `beside()` or `below()` it is the plot's
// *cell*, and the plots that ask for nothing split what is left — which is how a
// marginal histogram says it is thin. One meaning in both places (Law 6), and
// not to be confused with `ratio`, which shapes the panel inside whatever room
// the plot was given.
export function theme(...raw) {
  const {
    preset, grid, ratio, tick_angle, font_size, background, strip, strip_text,
    frame, width, height,
  } = readArgs(raw, "theme", [
    "preset",
    "grid",
    "ratio",
    "tick_angle",
    "font_size",
    "background",
    "strip",
    "strip_text",
    "frame",
    "width",
    "height",
  ]);

  if (preset === undefined && grid === undefined && ratio === undefined &&
      tick_angle === undefined && font_size === undefined &&
      background === undefined && strip === undefined &&
      strip_text === undefined && frame === undefined &&
      width === undefined && height === undefined) {
    throw new GogError(
      "gog: `theme()` sets nothing. Name a preset or a property, e.g. " +
        '`theme("minimal")` or `theme({ grid: "none", ratio: 1 })`.'
    );
  }
  if (preset !== undefined && typeof preset !== "string") {
    throw new GogError(
      'gog: `theme()` takes a preset name first — `theme("minimal")` — and ' +
        'everything else in one object: `theme({ grid: "none" })`.'
    );
  }
  // Checked in the engine too (`check_theme`), which is what makes the rule the
  // grammar's rather than this binding's. Checking here as well is what puts the
  // error on the line that wrote it.
  if (grid !== undefined && !GRID_VALUES.includes(grid)) {
    throw new GogError(
      `gog: \`theme({ grid: … })\` is one of ${GRID_VALUES.map((v) => `"${v}"`).join(", ")}.`
    );
  }
  if (ratio !== undefined && (typeof ratio !== "number" || !Number.isFinite(ratio) || ratio <= 0)) {
    throw new GogError(
      "gog: `theme({ ratio: … })` is the panel's width divided by its height, so it " +
        "needs one positive number. `ratio: 1` is a square."
    );
  }
  if (tick_angle !== undefined &&
      (typeof tick_angle !== "number" || !Number.isFinite(tick_angle) || Math.abs(tick_angle) > 90)) {
    throw new GogError(
      "gog: `theme({ tick_angle: … })` turns the x tick labels between -90 and 90 " +
        "degrees. `tick_angle: 45` is the usual answer to names that overlap."
    );
  }

  if (font_size !== undefined &&
      (typeof font_size !== "number" || !Number.isFinite(font_size) || font_size < 4)) {
    throw new GogError(
      "gog: `theme({ font_size: … })` is how many pixels a tick label is, not a " +
        "multiplier, so it needs one number of at least 4. The default is 11, and " +
        "the axis names and the title are derived from it."
    );
  }
  if (frame !== undefined && !FRAME_VALUES.includes(frame)) {
    throw new GogError(
      `gog: \`theme({ frame: … })\` is one of ${FRAME_VALUES.map((v) => `"${v}"`).join(", ")} ` +
        '— "full" is a rectangle round the panel, "axes" bottom and left only.'
    );
  }
  if (background !== undefined && typeof background !== "string") {
    throw new GogError(
      'gog: `theme({ background: … })` needs a single color, e.g. ' +
        '`{ background: "white" }` or `"transparent"`.'
    );
  }
  if (strip !== undefined && typeof strip !== "string") {
    throw new GogError(
      'gog: `theme({ strip: … })` needs a single color for the band above each ' +
        'panel, e.g. `{ strip: "white" }`.'
    );
  }
  if (strip_text !== undefined && typeof strip_text !== "string") {
    throw new GogError(
      "gog: `theme({ strip_text: … })` needs a single color for the strip's label. " +
        "Leave it out and gog picks the one that reads on the band."
    );
  }

  // One loop for both, because they are one property asked twice — see the
  // engine's `check_theme`, which states the same rule for every binding.
  for (const [name, value] of [["width", width], ["height", height]]) {
    if (value !== undefined &&
        (typeof value !== "number" || !Number.isFinite(value) || value < 40)) {
      throw new GogError(
        `gog: \`theme({ ${name}: … })\` is how many pixels the plot asks for, so it ` +
          "needs one number of at least 40. On its own it sizes the image; composed " +
          "with `beside()` or `below()` it sizes the plot's cell on the page."
      );
    }
  }

  return new Atom("theme", {
    preset: preset ?? null,
    grid: grid ?? null,
    ratio: ratio ?? null,
    tick_angle: tick_angle ?? null,
    font_size: font_size ?? null,
    background: background ?? null,
    strip: strip ?? null,
    strip_text: strip_text ?? null,
    frame: frame ?? null,
    width: width ?? null,
    height: height ?? null,
  });
}

function textValue(value, atom) {
  if (typeof value !== "string") {
    throw new GogError(
      `gog: \`${atom}()\` needs a string, e.g. \`${atom}("Life expectancy")\`.`
    );
  }
  return value;
}

// Set the plot title.
export function title(value) {
  return new Atom("title", { value: textValue(value, "title") });
}

// Override the x-axis label.
export function x_label(value) {
  return new Atom("x_label", { value: textValue(value, "x_label") });
}

// Override the y-axis label.
export function y_label(value) {
  return new Atom("y_label", { value: textValue(value, "y_label") });
}

// Override the z-axis label.
export function z_label(value) {
  return new Atom("z_label", { value: textValue(value, "z_label") });
}
