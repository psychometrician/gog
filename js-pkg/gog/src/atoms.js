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
// anything on that mark — stays in the engine's `legality`, where every binding
// inherits it.

import { Column, columnName, describe } from "./columns.js";
import { GogError, refuseUnknownOptions } from "./errors.js";
import { dateSeconds } from "./render.js";
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

function positive(value, atom, argument, example, unit = "", purpose = "") {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    throw new GogError(
      `gog: \`${atom}({ ${argument}: … })\` needs one positive number${unit}, ` +
        `e.g. \`${example}\`${purpose}.`
    );
  }
  return value;
}

// ---------------------------------------------------------------------------
// Marks — the geometric forms
// ---------------------------------------------------------------------------

/**
 * A mark: one dot (or another symbol, see `shape()`) for each row, to show how
 * two measurements are related.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * Write `point` bare, without parentheses. Calling it, as `point()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // A mark plus its positions is the smallest sentence that draws.
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg));
 */
export const point = bareAtom("mark", { mark: "point" });
/**
 * A mark: joins the rows in the order of `x`, to show how one value changed.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * To join the rows in the order of the table instead, use `path`. To join each
 * group's rows apart from the others, add `group()` or `color()`.
 *
 * Write `line` bare, without parentheses. Calling it, as `line()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const sales = {
 *   year: [2019, 2020, 2021, 2022, 2023],
 *   sales: [120, 135, 128, 150, 162],
 * };
 *
 * const p = plot(data(sales), line, x(col.year), y(col.sales));
 *
 * // The same data, a different shape: one word is all that changes.
 * const q = plot(data(sales), step, x(col.year), y(col.sales));
 */
export const line = bareAtom("mark", { mark: "line" });
/**
 * A mark: joins the rows in the order of the table, rather than in the order of
 * `x`, so the path can turn back and cross itself.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * On a `map()` or a `globe()` it is a route through places. With
 * `style({ arrow: "end" })` its last end carries an arrowhead.
 *
 * Write `path` bare, without parentheses. Calling it, as `path()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95, 4.35,
 *               1.833, 3.917],
 *   waiting: [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84],
 * };
 *
 * // Each eruption joined to the next, in the order they were recorded.
 * const p = plot(data(geyser), path, x(col.eruptions), y(col.waiting));
 */
export const path = bareAtom("mark", { mark: "path" });
/**
 * A mark: a reference line across the panel at each value of its column, the
 * threshold everything else is compared against.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * A rule is placed by one position and spans the whole panel on the other, so
 * its values usually come from a small table of their own, written with a
 * second `data()`. With `style({ reach: "edge" })` each rule is a short tick at
 * the start of the axis instead: a rug. A rule takes no transform.
 *
 * Write `rule` bare, without parentheses. Calling it, as `rule()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // The threshold, in a table of its own.
 * const threshold = { mpg: [20] };
 *
 * // The points, and a line across the panel at 20 miles per gallon.
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   data(threshold), rule);
 */
export const rule = bareAtom("mark", { mark: "rule" });
/**
 * A mark: shades a rectangular region of the panel.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * With `bounds()` it is a band between two values, or a rectangle with four
 * sides; a side with no `bounds()` column spans the panel. With `bin` on both
 * positions it is a field of tiles, shaded by how many rows fall in each.
 *
 * Write `zone` bare, without parentheses. Calling it, as `zone()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95, 4.35,
 *               1.833, 3.917],
 *   waiting: [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84],
 * };
 *
 * // Tiles, shaded by how many eruptions fall in each.
 * const p = plot(data(geyser), layer(zone, bin), x(col.eruptions),
 *   y(col.waiting));
 */
export const zone = bareAtom("mark", { mark: "zone" });
/**
 * A mark: fills the region between a line and the baseline.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * Write `area` bare, without parentheses. Calling it, as `area()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const sales = {
 *   year: [2019, 2020, 2021, 2022, 2023],
 *   sales: [120, 135, 128, 150, 162],
 * };
 *
 * const p = plot(data(sales), area, x(col.year), y(col.sales));
 */
export const area = bareAtom("mark", { mark: "area" });
/**
 * A mark: a rectangle from the baseline to the value, to show how large each
 * category is.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * With a transform it draws a summary: `layer(bar, count)` counts the rows in
 * each category, and `layer(bar, bin)` is a histogram.
 *
 * Write `bar` bare, without parentheses. Calling it, as `bar()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const cars = {
 *   car: ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
 *         "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // One bar for each car, as tall as its miles per gallon.
 * const p = plot(data(cars), bar, x(col.car), y(col.mpg));
 */
export const bar = bareAtom("mark", { mark: "bar" });
/**
 * A mark: a line that stays level until the value changes, then goes straight
 * up or down.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * Write `step` bare, without parentheses. Calling it, as `step()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const sales = {
 *   year: [2019, 2020, 2021, 2022, 2023],
 *   sales: [120, 135, 128, 150, 162],
 * };
 *
 * const p = plot(data(sales), step, x(col.year), y(col.sales));
 */
export const step = bareAtom("mark", { mark: "step" });
/**
 * A mark: a segment from a low value to a high one.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * It needs a pair of values, so it is usually written with a transform that
 * makes one, such as `range`, `confidence` or `deviation`, or with `bounds()`
 * for a pair already in the table. With `style({ caps: false })` it is a bare
 * line without end caps.
 *
 * Write `interval` bare, without parentheses. Calling it, as `interval()`,
 * throws a `GogError`, because a mark takes no arguments.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // The shortest and the longest petal of each species.
 * const p = plot(data(flowers), layer(interval, range), x(col.species),
 *   y(col.petal_length));
 */
export const interval = bareAtom("mark", { mark: "interval" });
/**
 * A mark: a band between two values across a span.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * Like `interval`, it needs a pair of values, so it is written with a transform
 * that makes one, such as `smooth_band` or `range`, or with `bounds()`.
 *
 * Write `ribbon` bare, without parentheses. Calling it, as `ribbon()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95, 4.35,
 *               1.833, 3.917],
 *   waiting: [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84],
 * };
 *
 * // A trend, and the band around it.
 * const p = plot(data(geyser), x(col.eruptions), y(col.waiting), point,
 *   layer(ribbon, smooth_band), layer(line, smooth));
 */
export const ribbon = bareAtom("mark", { mark: "ribbon" });
/**
 * A mark: writes the text that `label()` gives at each position.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * `label()` is its required channel. For a labeled scatter plot, write `point`
 * and `text` as two marks with the same positions. `layer(text, repel)` moves
 * labels apart when they overlap.
 *
 * Write `text` bare, without parentheses. Calling it, as `text()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const cars = {
 *   car: ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
 *         "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // Each car's name, written at its weight and miles per gallon.
 * const p = plot(data(cars), text, x(col.wt), y(col.mpg), label(col.car));
 */
export const text = bareAtom("mark", { mark: "text" });
// A sheet through the samples, and the one mark that draws in the cube alone. Its
// rows are nodes: the grid the two position columns describe is recovered rather
// than declared, so it wants one row per (x, y) crossing. Three positions, all
// required and all numeric — a face asserts every value *between* two nodes, and
// between two categories there is nothing to assert (for a mesh over categories,
// `layer(bar, bin)` in `space()`). One transform, `density`, which makes it the
// third geometry of one field: a `zone` paints it as cells, a `path` traces its
// contours, a `surface` raises it with the estimate as height.
/**
 * A mark: a sheet over a grid of samples, for three-dimensional data.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * It takes three positions, `x`, `y` and `z`, all numbers, and wants one row
 * for each crossing of `x` and `y`: the grid is read from the rows. It draws
 * only in the cube. With `layer(surface, density)` the height is a density
 * estimate instead of a column.
 *
 * Write `surface` bare, without parentheses. Calling it, as `surface()`, throws
 * a `GogError`, because a mark takes no arguments.
 *
 * @example
 * const hill = {
 *   x: [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4],
 *   y: [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4],
 *   height: [0.223, 0.435, 0.435, 0.223, 0.435, 0.846, 0.846, 0.435, 0.435,
 *            0.846, 0.846, 0.435, 0.223, 0.435, 0.435, 0.223],
 * };
 *
 * const p = plot(data(hill), surface, x(col.x), y(col.y), z(col.height));
 */
export const surface = bareAtom("mark", { mark: "surface" });
// The stroke whose two endpoints the layout supplies — one row is one edge of
// a graph. Nothing binds to its positions; its minimum syllable is
// `layer(edge, layout(col.a, col.b))` inside `network()`, and like `surface`
// it does not draw flat.
/**
 * A mark: the line between two nodes of a graph, one for each row.
 *
 * A mark is the shape a layer draws, and each layer draws one. A mark holds no
 * data of its own: channels such as `x()` and `y()` say which column goes to
 * which part of the picture, and the engine draws the mark from those columns.
 * A mark and its required positions together are the smallest sentence that
 * draws. Asking a mark for a channel it cannot carry is refused, with a message
 * that says what to write instead.
 *
 * A `layout()` supplies both of its ends, so nothing binds to its positions,
 * and it draws inside `network()`: `layer(edge, layout(col.from, col.to))`.
 * Like `surface`, it does not draw in the ordinary space: it needs `network()`.
 *
 * Write `edge` bare, without parentheses. Calling it, as `edge()`, throws a
 * `GogError`, because a mark takes no arguments.
 *
 * @example
 * const links = {
 *   sender: ["Ann", "Ann", "Bob", "Cy", "Dee"],
 *   receiver: ["Bob", "Cy", "Cy", "Dee", "Ann"],
 * };
 *
 * // Who sends work to whom: the links, and the people as points.
 * const p = plot(data(links),
 *   layer(edge, layout(col.sender, col.receiver)),
 *   layer(point, layout(col.sender, col.receiver)),
 *   network());
 */
export const edge = bareAtom("mark", { mark: "edge" });

// `box` — the box-and-whisker mark, with its one knob.
/**
 * A mark: the box-and-whisker plot, one box for each group.
 *
 * Each box spans the middle half of its group, from the 25th to the 75th
 * percentile, with a line at the median and a whisker at each end. A mark is
 * the shape a layer draws, and each layer draws one. A mark holds no data of
 * its own: channels such as `x()` and `y()` say which column goes to which part
 * of the picture, and the engine draws the mark from those columns. A mark and
 * its required positions together are the smallest sentence that draws. Asking
 * a mark for a channel it cannot carry is refused, with a message that says
 * what to write instead.
 *
 * Write `box` bare for the standard box plot. Call it to choose the whiskers:
 * `box({ whiskers: "range" })`, or `box("range")`.
 *
 * @param {object} [options] Settings, by name.
 * @param {"tukey"|"range"} [options.whiskers] `"tukey"` (the default) runs the
 *   whiskers to the most extreme value no more than 1.5 times the box's length
 *   (the interquartile range) beyond the box, and draws the values past that as
 *   outliers: the standard box plot. `"range"` runs them to the smallest and
 *   the largest value, with no outliers.
 * @returns {Atom} The mark, with its whiskers chosen.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 * };
 *
 * // One box for each species: the median, the quartiles and the whiskers.
 * const b = plot(data(flowers), box, x(col.species), y(col.sepal_length));
 *
 * // Whiskers that reach the smallest and the largest value.
 * const r = plot(data(flowers), box({ whiskers: "range" }), x(col.species),
 *   y(col.sepal_length));
 */
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

/**
 * A transform: a fitted curve through the points, a trend, rather than the
 * points themselves.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * The curve is a local regression (LOESS), so it may bend.
 * `layer(ribbon, smooth_band)` draws the band around it; for a straight line,
 * use `straight`.
 *
 * Write `smooth` bare, without parentheses. Calling it, as `smooth()`, throws a
 * `GogError`, because it takes no arguments.
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95, 4.35,
 *               1.833, 3.917],
 *   waiting: [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84],
 * };
 *
 * // The points, and the trend through them.
 * const p = plot(data(geyser), x(col.eruptions), y(col.waiting), point,
 *   layer(line, smooth));
 */
export const smooth = bareAtom("transform", { transform: "smooth" });
/**
 * A transform: the least-squares line through the points.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * A straight fit, where `smooth` may bend. It is refused on a log `y` axis,
 * where the line would be drawn as a curve. `layer(ribbon, straight_band)`
 * draws the band around it.
 *
 * Write `straight` bare, without parentheses. Calling it, as `straight()`,
 * throws a `GogError`, because it takes no arguments.
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95, 4.35,
 *               1.833, 3.917],
 *   waiting: [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84],
 * };
 *
 * const p = plot(data(geyser), x(col.eruptions), y(col.waiting), point,
 *   layer(line, straight));
 */
export const straight = bareAtom("transform", { transform: "straight" });
/**
 * A transform: the number of rows for each value of `x` (or of `y`).
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * The count is a new column, `count`, so `label(col.count)` writes it on a
 * `text` layer.
 *
 * Write `count` bare, without parentheses. Calling it, as `count()`, throws a
 * `GogError`, because it takes no arguments.
 *
 * @example
 * const cars = {
 *   cylinders: ["6", "6", "4", "6", "8", "6", "8", "4"],
 * };
 *
 * // The number of cars with each number of cylinders.
 * const h = plot(data(cars), layer(bar, count), x(col.cylinders));
 *
 * // The same statistic, a different shape.
 * const l = plot(data(cars), layer(line, count), x(col.cylinders));
 */
export const count = bareAtom("transform", { transform: "count" });
/**
 * A transform: the sum of the measure within each group.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * Write `sum` bare, without parentheses. Calling it, as `sum()`, throws a
 * `GogError`, because it takes no arguments.
 *
 * @example
 * const titanic = {
 *   travel_class: ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
 *   people: [122, 167, 528, 673, 203, 118, 178, 212],
 * };
 *
 * // The people aboard in each class, added up over survival.
 * const p = plot(data(titanic), layer(bar, sum), x(col.travel_class),
 *   y(col.people));
 */
export const sum = bareAtom("transform", { transform: "sum" });
/**
 * A transform: the mean of the measure within each group.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * Write `mean` bare, without parentheses. Calling it, as `mean()`, throws a
 * `GogError`, because it takes no arguments.
 *
 * @example
 * const cars = {
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 *   cylinders: ["6", "6", "4", "6", "8", "6", "8", "4"],
 * };
 *
 * // The mean miles per gallon of the cars with each number of cylinders.
 * const p = plot(data(cars), layer(bar, mean), x(col.cylinders),
 *   y(col.mpg));
 */
export const mean = bareAtom("transform", { transform: "mean" });
/**
 * A transform: the median of the measure within each group.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * Write `median` bare, without parentheses. Calling it, as `median()`, throws a
 * `GogError`, because it takes no arguments.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * const p = plot(data(flowers), layer(bar, median), x(col.species),
 *   y(col.petal_length));
 */
export const median = bareAtom("transform", { transform: "median" });
/**
 * A transform: the largest value of the measure within each group.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * Write `max` bare, without parentheses. Calling it, as `max()`, throws a
 * `GogError`, because it takes no arguments.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * const p = plot(data(flowers), layer(bar, max), x(col.species),
 *   y(col.petal_length));
 */
export const max = bareAtom("transform", { transform: "max" });
/**
 * A transform: the smallest value of the measure within each group.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * Write `min` bare, without parentheses. Calling it, as `min()`, throws a
 * `GogError`, because it takes no arguments.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * const p = plot(data(flowers), layer(point, min), x(col.species),
 *   y(col.petal_length));
 */
export const min = bareAtom("transform", { transform: "min" });
/**
 * A transform: each group's share of the total, rather than its count.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * The shares are of the whole plot's total, so they add up to 1. For each
 * pile's own shares, use `stack({ share: true })`.
 *
 * Write `proportion` bare, without parentheses. Calling it, as `proportion()`,
 * throws a `GogError`, because it takes no arguments.
 *
 * @example
 * const cars = {
 *   cylinders: ["6", "6", "4", "6", "8", "6", "8", "4"],
 * };
 *
 * // The share of the cars with each number of cylinders.
 * const p = plot(data(cars), layer(bar, proportion), x(col.cylinders));
 */
export const proportion = bareAtom("transform", { transform: "proportion" });
// `deviation` — the spread band per group, one standard deviation unless told.
// `confidence`'s twin: the same pair with a center, and a different question.
/**
 * A transform: the mean of each group, plus and minus a number of standard
 * deviations.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * Like `confidence`, it gives a low and a high value around the mean, for a
 * different question. `confidence` asks how precisely the mean is known;
 * `deviation` asks how far the values spread. Write it bare for one standard
 * deviation, or call it to choose.
 *
 * @param {number} [multiplier] How many standard deviations reach each side of
 *   the mean, one positive number: `deviation(2)`. 1 unless given.
 * @returns {Atom} The transform, with its multiplier.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // The mean of each species, plus and minus one standard deviation.
 * const p = plot(data(flowers), layer(interval, deviation), x(col.species),
 *   y(col.petal_length));
 *
 * // Two standard deviations.
 * const q = plot(data(flowers), layer(interval, deviation(2)), x(col.species),
 *   y(col.petal_length));
 */
export const deviation = callableAtom(
  new Atom("transform", { transform: "deviation" }),
  (...raw) => {
    const { multiplier } = readArgs(raw, "deviation", ["multiplier"]);
    if (
      multiplier !== undefined &&
      (typeof multiplier !== "number" || !Number.isFinite(multiplier) || multiplier <= 0)
    ) {
      throw new GogError(
        "gog: `deviation({ multiplier: … })` needs one positive number — it counts " +
          "standard deviations out from the mean. `deviation` is one, " +
          "`deviation(2)` is two."
      );
    }
    return new Atom("transform", {
      transform: "deviation",
      multiplier: multiplier ?? null,
    });
  }
);

// `quantile` — the value at one probability. No default: 0.5 is `median`.
/**
 * A transform: the value of `y` at one probability, for each group.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * `quantile(0.9)` is the 90th percentile. There is no default, so call it with
 * the probability. At 0, 0.5 and 1 the plot draws, and gog says that `min`,
 * `median` and `max` are the plain names for the same numbers.
 *
 * @param {number} p The probability, one number from 0 to 1: `quantile(0.9)`.
 *   Required: bare `quantile` is refused.
 * @returns {Atom} The transform, with its probability.
 *
 * @example
 * const trees = {
 *   age: [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
 *   circumference: [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108],
 * };
 *
 * // The 90th percentile and the median of the trees' circumference at each age.
 * const p = plot(data(trees), x(col.age), y(col.circumference),
 *   layer(line, quantile(0.9)), layer(line, median));
 */
export const quantile = callableAtom(
  new Atom("transform", { transform: "quantile" }),
  (...raw) => {
    const { p } = readArgs(raw, "quantile", ["p"]);
    if (p !== undefined) {
      if (typeof p !== "number" || !Number.isFinite(p)) {
        throw new GogError(
          "gog: `quantile()` takes one number between 0 and 1, the probability it " +
            "reduces to, e.g. `quantile(0.9)`."
        );
      }
      if (p < 0 || p > 1) {
        throw new GogError(
          `gog: \`quantile(${p})\` is not a probability — a quantile is between 0 ` +
            "and 1. `quantile(0.9)` is the 90th percentile, `quantile(0.5)` the middle."
        );
      }
    }
    return new Atom("transform", { transform: "quantile", p: p ?? null });
  }
);

// `range` — the band per group, the whole group unless told otherwise. Two
// positional ends, because a band has two and neither is the more common to set.
/**
 * A transform: a band for each group, between two of its quantiles.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * It makes a low and a high value, which is what `interval` and `ribbon` draw.
 * Write `range` bare for the whole group, from its smallest value to its
 * largest, or call it with the two ends: `range(0.25, 0.75)` is the middle
 * half. An end left out is that side's extreme, so `range({ high: 0.9 })` runs
 * from the smallest value to the 90th percentile.
 *
 * @param {number} [low] The band's low end, as a probability from 0 to 1.
 * @param {number} [high] The band's high end, as a probability from 0 to 1.
 * @returns {Atom} The transform, with its two ends.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // The middle half of each species, from the 25th to the 75th percentile.
 * const p = plot(data(flowers), layer(interval, range(0.25, 0.75)),
 *   x(col.species), y(col.petal_length));
 */
export const range = callableAtom(
  new Atom("transform", { transform: "range" }),
  (...raw) => {
    const { low, high } = readArgs(raw, "range", ["low", "high"]);
    for (const [name, value] of [
      ["low", low],
      ["high", high],
    ]) {
      if (value === undefined) continue;
      if (typeof value !== "number" || !Number.isFinite(value)) {
        throw new GogError(
          "gog: `range()` takes the band's two ends, each one number between 0 " +
            "and 1, e.g. `range(0.25, 0.75)`."
        );
      }
      if (value < 0 || value > 1) {
        throw new GogError(
          `gog: \`range({ ${name}: ${value} })\` is not a probability — the ` +
            "band's ends are quantiles, so each is between 0 and 1. " +
            "`range(0.25, 0.75)` is the middle half, `range(0.1, 0.9)` the middle " +
            "80 percent, and bare `range` the whole group."
        );
      }
    }
    return new Atom("transform", {
      transform: "range",
      low: low ?? null,
      high: high ?? null,
    });
  }
);
/**
 * Place the marks of each group side by side, so that each is measured from the
 * same baseline.
 *
 * When several marks are drawn in the same place, `stack`, `dodge`, `jitter`
 * and `repel` decide where each one goes. They compute no statistic: they move
 * marks that would otherwise cover each other. Like transforms they go in
 * `layer()`, after any statistic, so `layer(bar, sum, stack)` adds up the
 * values and then piles the sums.
 *
 * On bars, boxes and intervals the groups come from a `color()`, `pattern()`
 * or `group()` binding; without one there is nothing to dodge, and gog says
 * so. On `point` it moves each point across its category only as far as
 * needed so that no two points overlap: the beeswarm, which needs no groups.
 *
 * Write `dodge` bare, without parentheses. Calling it, as `dodge()`, throws a
 * `GogError`, because it takes no arguments.
 *
 * @example
 * const titanic = {
 *   travel_class: ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
 *   survived: ["No", "No", "No", "No", "Yes", "Yes", "Yes", "Yes"],
 *   people: [122, 167, 528, 673, 203, 118, 178, 212],
 * };
 *
 * // Side by side: each bar measured from the same baseline.
 * const d = plot(data(titanic), layer(bar, sum, dodge),
 *   x(col.travel_class), y(col.people), color(col.survived));
 */
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
/**
 * Pile the marks of each group end to end, so that the height of the pile is
 * the total.
 *
 * When several marks are drawn in the same place, `stack`, `dodge`, `jitter`
 * and `repel` decide where each one goes. They compute no statistic: they move
 * marks that would otherwise cover each other. Like transforms they go in
 * `layer()`, after any statistic, so `layer(bar, sum, stack)` adds up the
 * values and then piles the sums.
 *
 * On bars and areas the groups come from a `color()`, `pattern()` or `group()`
 * binding; without one there is nothing to stack, and gog says so. Write
 * `stack` bare to pile the values themselves from zero, or call it to change
 * that.
 *
 * @param {object} [options] Settings, by name.
 * @param {boolean} [options.share] `true` fills every pile to 1, so the plot
 *   shows each group's share of its pile rather than its amount: the 100%
 *   stacked bar. `false` unless given. For shares of the whole plot instead,
 *   use `proportion`.
 * @param {"zero"|"center"|"wiggle"} [options.baseline] Where each pile starts.
 *   `"zero"` (the default) starts every pile at zero. `"center"` places each
 *   pile so that its middle is at zero. `"wiggle"` chooses the start that keeps
 *   the bands as flat as it can: the streamgraph.
 * @returns {Atom} The transform, with its settings.
 *
 * @example
 * const titanic = {
 *   travel_class: ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
 *   survived: ["No", "No", "No", "No", "Yes", "Yes", "Yes", "Yes"],
 *   people: [122, 167, 528, 673, 203, 118, 178, 212],
 * };
 *
 * // Piled: the height of the pile is the total.
 * const s = plot(data(titanic), layer(bar, sum, stack), x(col.travel_class),
 *   y(col.people), color(col.survived));
 *
 * // Every pile the same height, so the plot shows shares.
 * const f = plot(data(titanic), layer(bar, sum, stack({ share: true })),
 *   x(col.travel_class), y(col.people), color(col.survived));
 */
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

// `cumulative` — the running total: each position's value added to the ones
// before it, in the order the axis draws them, within each split. It adds up what
// the layer already measures, and every mark draws the total as it draws any
// measure. `cumulative({ share: true })` ends every group's total at 1, which
// `proportion` cannot say: it divides by the whole plot's total.
/**
 * A transform: the running total of a measure, along its axis.
 *
 * `cumulative` adds each position's value to the values before it, in the order
 * the axis draws its positions: left to right on numbers, in the categories'
 * own order on categories. It adds up what the layer already measures, the
 * column named on `y` or what a transform before it made, and every mark draws
 * the total as it draws any measure. The total is the column `cumulative`,
 * while the measure's own column keeps each row's value: with `count` before
 * it, `label(col.count)` and `label(col.cumulative)` write the two.
 *
 * When `color()` or `group()` splits the rows, each group adds up its own
 * total. A category axis is added up in its declared order, so give the order
 * in the table, with `ordered()`, rather than with `order()`.
 *
 * @param {object} [options] Settings, by name.
 * @param {boolean} [options.share] `true` divides each group's total by its
 *   last, so every curve ends at 1: the empirical distribution function of each
 *   group. `false` unless given.
 * @returns {Atom} The transform, with its setting.
 *
 * @example
 * const sales = {
 *   year: [2019, 2020, 2021, 2022, 2023],
 *   sales: [120, 135, 128, 150, 162],
 * };
 *
 * // Each year's sales added to the years before it.
 * const r = plot(data(sales), layer(line, cumulative), x(col.year),
 *   y(col.sales));
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 * };
 *
 * // The cumulative histogram, and one distribution function per species.
 * const h = plot(data(flowers), layer(step, bin, cumulative),
 *   x(col.sepal_length));
 * const e = plot(data(flowers), layer(step, bin, cumulative({ share: true })),
 *   x(col.sepal_length), color(col.species));
 */
export const cumulative = callableAtom(
  new Atom("transform", { transform: "cumulative" }),
  (...raw) => {
    const { share } = readArgs(raw, "cumulative", ["share"]);
    if (share !== undefined && typeof share !== "boolean") {
      throw new GogError(
        "gog: `cumulative({ share: … })` is true or false — true ends every group's " +
          "total at 1, false adds the values themselves."
      );
    }
    return new Atom("transform", { transform: "cumulative", share: share ?? null });
  }
);

// `bin` — equal-width buckets. How many dimensions it cuts is the mark's answer.
/**
 * A transform: cut a column of numbers into equal-width bins, and count the
 * rows in each. The histogram.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * The mark decides whether it cuts in one direction or two: `layer(bar, bin)`
 * cuts one position into bars, and `layer(zone, bin)` with both `x` and `y`
 * given cuts the plane into tiles. Write `bin` bare for the default number of
 * bins, or call it to choose.
 *
 * @param {number} [bins] How many bins, one positive whole number: `bin(30)`.
 *   Not together with `width`.
 * @param {object} [options] Settings, by name.
 * @param {number} [options.width] How wide each bin is, in the column's own
 *   units: `bin({ width: 5 })`. Not together with `bins`.
 * @param {"rect"|"hex"} [options.tiling] How the plane is cut when both
 *   positions are binned: `"rect"` (the default) or `"hex"`, as in
 *   `layer(zone, bin({ tiling: "hex" }))`.
 * @returns {Atom} The transform, with its bins chosen.
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95, 4.35,
 *               1.833, 3.917],
 * };
 *
 * // A histogram: each bar counts the rows that fall in its bin.
 * const h = plot(data(geyser), layer(bar, bin), x(col.eruptions));
 *
 * // Six bins, or bins half a minute wide.
 * const h6 = plot(data(geyser), layer(bar, bin(6)), x(col.eruptions));
 * const hw = plot(data(geyser), layer(bar, bin({ width: 0.5 })),
 *   x(col.eruptions));
 */
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
/**
 * A transform: a smooth estimate of how a column of numbers is spread (a kernel
 * density estimate).
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * It has three readings, decided by the sentence. With one position it is a
 * curve: `layer(line, density)`. With a category on one position and numbers on
 * the other it is a violin for each category:
 * `layer(ribbon, density), x(col.<category>), y(col.<number>)`. With numbers on
 * both it is a field over the plane, drawn as tiles by `zone`, as contours by
 * `path` and as a sheet by `surface`. Write `density` bare for the automatic
 * bandwidth, or call it to adjust.
 *
 * @param {number} [adjust] A multiplier on the automatic bandwidth, one
 *   positive number: `density(2)` doubles the bandwidth, so the curve is
 *   smoother; `density(0.5)` halves it. Not together with `bandwidth`.
 * @param {object} [options] Settings, by name.
 * @param {number} [options.bandwidth] The bandwidth in the column's own units,
 *   one positive number: `density({ bandwidth: 5 })`. Not together with
 *   `adjust`. On a two-dimensional density, such as `layer(zone, density)`,
 *   the two axes measure different things, so one width cannot fit both. gog
 *   refuses it there and points you to `adjust`.
 * @param {number} [options.levels] How many levels to cut a
 *   two-dimensional density into, one positive whole number:
 *   `layer(path, density({ levels: 8 }))` traces their edges, and
 *   `layer(zone, density({ levels: 8 }))` fills between them. Refused on a
 *   curve, which is one line with nothing to cut.
 * @param {"count"|"shape"} [options.compare] What a violin's width means from
 *   one category to the next. `"count"` (the default) scales each violin by its
 *   group's number of rows, so a thin violin is a small group. `"shape"` draws
 *   every violin with the same area, comparing shapes and nothing else.
 *   Refused on a curve and on a two-dimensional density, which have no slots
 *   to compare.
 * @param {number} [options.reach] How far each violin reaches from its
 *   category's line, in slots (a slot is the space one category takes on the
 *   axis), one positive number. `0.4` (the default) keeps every shape inside
 *   its own slot; above `0.5` they overlap their neighbors, which is the
 *   ridgeline plot:
 *   `layer(area, density({ reach: 2.5 })), x(col.<number>), y(col.<category>)`.
 *   A `ribbon` reaches this far on each side and an `area` on one. Like
 *   `compare`, it needs a category to give it slots.
 * @returns {Atom} The transform, with its settings.
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95, 4.35,
 *               1.833, 3.917],
 * };
 *
 * // A smooth estimate of how eruption times are spread.
 * const d = plot(data(geyser), layer(line, density), x(col.eruptions));
 *
 * // Twice the bandwidth: a smoother curve.
 * const d2 = plot(data(geyser), layer(line, density(2)), x(col.eruptions));
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 * };
 *
 * // A violin for each species.
 * const v = plot(data(flowers), layer(ribbon, density), x(col.species),
 *   y(col.sepal_length));
 */
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
          : positive(reach, "density", "reach", "density({ reach: 2.5 })",
            " of slots", " for overlapping ridges"),
    });
  }
);

// `confidence` — the mean's interval per group, 0.95 unless told otherwise.
/**
 * A transform: the mean of each group, with its confidence interval.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * It makes a low and a high value around a center. An `interval` draws them as
 * a dot at the mean and a line across the interval; a `ribbon` fills the band
 * between them.
 * `style({ center: false })` hides the dot. Write `confidence` bare for a 95%
 * interval, or call it to choose the level.
 *
 * @param {number} [level] The confidence level, one number between 0 and 1, not
 *   either end: `confidence(0.99)`. 0.95 unless given.
 * @returns {Atom} The transform, with its level.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // The mean of each species, with its 95% confidence interval.
 * const p = plot(data(flowers), layer(interval, confidence), x(col.species),
 *   y(col.petal_length));
 *
 * // A wider interval, at 99%.
 * const q = plot(data(flowers), layer(interval, confidence(0.99)),
 *   x(col.species), y(col.petal_length));
 */
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

// `smooth_band` — the band around a `smooth` line, 0.95 unless told otherwise.
// `smooth`'s pair form, as `confidence` is `mean`'s: `layer(ribbon, smooth_band)`
// beside `layer(line, smooth)` draws the trend with its band.
/**
 * A transform: the band around a `smooth` curve, the fit's confidence band.
 *
 * It gives the low and high ends around `smooth`'s curve, as `confidence` does
 * around `mean`. At each point of the LOESS curve it gives the fit's low and
 * high ends, so `layer(ribbon, smooth_band)` written with `layer(line, smooth)`
 * draws the trend with its band. Write it bare for a 95% band, or call it to
 * choose the level.
 *
 * @param {number} [level] The confidence level, one number between 0 and 1, not
 *   either end: `smooth_band(0.99)`. 0.95 unless given.
 * @returns {Atom} The transform, with its level.
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95, 4.35,
 *               1.833, 3.917],
 *   waiting: [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84],
 * };
 *
 * // The trend, and how precisely it is known.
 * const p = plot(data(geyser), x(col.eruptions), y(col.waiting), point,
 *   layer(ribbon, smooth_band), layer(line, smooth));
 */
export const smooth_band = callableAtom(
  new Atom("transform", { transform: "smooth_band" }),
  (...raw) => {
    const { level } = readArgs(raw, "smooth_band", ["level"]);
    if (
      level !== undefined &&
      (typeof level !== "number" || !Number.isFinite(level) || level <= 0 || level >= 1)
    ) {
      throw new GogError(
        "gog: `smooth_band({ level: … })` needs one number strictly between 0 and 1, " +
          "e.g. `smooth_band(0.95)`."
      );
    }
    return new Atom("transform", {
      transform: "smooth_band",
      level: level ?? null,
    });
  }
);

// `straight_band` — the band around a `straight` line, 0.95 unless told otherwise.
// `straight`'s pair form, as `smooth_band` is `smooth`'s: the interval of the
// least-squares line's mean, so `layer(ribbon, straight_band)` beside
// `layer(line, straight)` draws the line with its band.
/**
 * A transform: the band around a `straight` line, the interval of the fitted
 * line's mean.
 *
 * It gives the low and high ends around `straight`'s line, as `smooth_band`
 * does around `smooth`'s curve. At each point of the least-squares line it
 * gives the confidence interval for the line's mean (the band R's
 * `predict(lm(...), interval = "confidence")` gives). So
 * `layer(ribbon, straight_band)` written with `layer(line, straight)` draws the
 * line with its band. It needs three rows in each group, and two different
 * values of `x`.
 *
 * @param {number} [level] The confidence level, one number between 0 and 1, not
 *   either end: `straight_band(0.99)`. 0.95 unless given.
 * @returns {Atom} The transform, with its level.
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883, 4.7, 3.6, 1.95, 4.35,
 *               1.833, 3.917],
 *   waiting: [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84],
 * };
 *
 * // The least-squares line, with its 95% confidence band.
 * const p = plot(data(geyser), x(col.eruptions), y(col.waiting), point,
 *   layer(ribbon, straight_band), layer(line, straight));
 *
 * // A wider band, at 99%.
 * const q = plot(data(geyser), x(col.eruptions), y(col.waiting), point,
 *   layer(ribbon, straight_band(0.99)), layer(line, straight));
 */
export const straight_band = callableAtom(
  new Atom("transform", { transform: "straight_band" }),
  (...raw) => {
    const { level } = readArgs(raw, "straight_band", ["level"]);
    if (
      level !== undefined &&
      (typeof level !== "number" || !Number.isFinite(level) || level <= 0 || level >= 1)
    ) {
      throw new GogError(
        "gog: `straight_band({ level: … })` needs one number strictly between 0 and 1, " +
          "e.g. `straight_band(0.95)`."
      );
    }
    return new Atom("transform", {
      transform: "straight_band",
      level: level ?? null,
    });
  }
);

// `jitter` — the categorical-axis spread, a multiple of the default.
/**
 * Move points a little across their category, so that points with the same
 * value do not hide each other.
 *
 * When several marks are drawn in the same place, `stack`, `dodge`, `jitter`
 * and `repel` decide where each one goes. They compute no statistic: they move
 * marks that would otherwise cover each other. Like transforms they go in
 * `layer()`, after any statistic, so `layer(bar, sum, stack)` adds up the
 * values and then piles the sums.
 *
 * It moves points only along an axis of categories, never along one that
 * measures, so no value is changed. Write `jitter` bare for the usual spread,
 * or call it to scale the spread.
 *
 * @param {number} [amount] The spread as a multiple of the usual one, from 0 to
 *   1.25: `jitter(0.5)` is half. 1 unless given. At 1.25 a point reaches the
 *   edge of the space its category takes on the axis, and more is refused.
 * @returns {Atom} The transform, with its spread.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 * };
 *
 * // Points moved a little sideways, so that they do not hide each other.
 * const p = plot(data(flowers), layer(point, jitter), x(col.species),
 *   y(col.sepal_length));
 */
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
          "multiple of the default, e.g. `jitter(0.5)` for half, up to `jitter(1.25)`, the " +
          "widest spread that keeps every point in its own category."
      );
    }
    return new Atom("transform", {
      transform: "jitter",
      amount: amount ?? null,
    });
  }
);

// `repel` — the fourth collision modifier, and the one whose collision is ink.
// The other three move marks that share a position; a label is as wide as the word
// it draws, so two labels overlap where their points do not. `text` only, and bare
// like `dodge`: a label moves as far as the overlap requires and no further, so
// there is no free spread for a knob to scale.
/**
 * Move overlapping labels apart until each can be read.
 *
 * When several marks are drawn in the same place, `stack`, `dodge`, `jitter`
 * and `repel` decide where each one goes. They compute no statistic: they move
 * marks that would otherwise cover each other. Like transforms they go in
 * `layer()`, after any statistic, so `layer(bar, sum, stack)` adds up the
 * values and then piles the sums.
 *
 * `layer(text, repel)` reads where the labels and the points are, and pushes
 * each label a different way until they stop overlapping, drawing a thin line
 * back to its point when the label has moved away from it. Where no arrangement
 * fits, every label is still drawn, and gog says how many are still crowded. It
 * works on `text` only. For a fixed offset, use `style({ nudge })`.
 *
 * Write `repel` bare, without parentheses. Calling it, as `repel()`, throws a
 * `GogError`, because it takes no arguments.
 *
 * @example
 * const cars = {
 *   car: ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
 *         "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // Each car's name, pushed away from the others so that no two overlap.
 * const p = plot(data(cars), point, layer(text, repel), x(col.wt),
 *   y(col.mpg), label(col.car));
 */
export const repel = bareAtom("transform", { transform: "repel" });

// Pre-computed bounds: `lower`/`upper` bound the measure axis, `start`/`end` the
// domain. Every argument names a column, which is why this atom reads worst of
// the fifty in a string spelling and reads like the rest with the accessor —
// `{ start: col.start }` says which half is a column where `{ start: "start" }`
// could not (spec §8).
/**
 * A transform: low and high values already in the table, one column for each
 * end.
 *
 * A transform replaces the rows with a summary of them, and the mark then draws
 * that summary. Join it to a mark with `layer()`: `layer(bar, count)` draws
 * bars whose heights are counts. A transform computes the same numbers on every
 * mark that accepts it. Change the mark, and the same numbers draw as a line,
 * an area or a step.
 *
 * Where `range` and `confidence` compute a low and a high value, `bounds()`
 * reads them from columns you computed before the plot. Each argument names a
 * column. `lower` and `upper` give the two ends on the measure axis, which is
 * the span `ribbon`, `interval`, `line` and `step` draw. `start` and `end` give
 * the two ends on the other axis, and only a `zone` takes them: they give a
 * rectangle its other two sides. A `zone` with no pair on an axis spans the
 * panel there.
 *
 * @param {Column} [lower] The column of low ends on the measure axis.
 * @param {Column} [upper] The column of high ends on the measure axis.
 * @param {object} [options] Settings, by name. Every argument may be written
 *   here as well: `bounds({ lower: col.lo, upper: col.hi })`.
 * @param {Column} [options.start] On a `zone`, the column where each rectangle
 *   starts on the other axis.
 * @param {Column} [options.end] On a `zone`, the column where each rectangle
 *   ends on the other axis.
 * @returns {Atom} The transform, with its columns.
 *
 * @example
 * // Intervals computed before the plot: the 95% confidence intervals of the
 * // coefficients of a model fitted to R's mtcars, `mpg ~ wt + am + qsec`.
 * const estimates = {
 *   term: ["wt", "am", "qsec"],
 *   lo: [-5.37, 0.05, 0.63],
 *   hi: [-2.46, 5.83, 1.82],
 * };
 * const p = plot(data(estimates), layer(interval, bounds(col.lo, col.hi)),
 *   y(col.term));
 */
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
 * A transform: divide a whole among the nested parts of a hierarchy.
 *
 * The hierarchy is given as columns, outermost first: one row of the table is
 * one leaf, and `partition(col.group, col.item, col.detail)` names the columns
 * that lead from the top level down to it. A missing value ends that branch
 * early, so the branches of a real hierarchy can end at different rings.
 *
 * `layer(zone, partition(...))` in the ordinary space is the icicle; the same
 * sentence with `polar()` is the sunburst, one ring for each level.
 * `layer(text, partition(...))` with `label(col.name)` writes each node's name
 * on it. Give `x` the column that sizes each branch: `x(col.amount)` divides
 * the whole by that column, and with no `x` every leaf counts as 1. Every
 * number belongs to a leaf: an inner node with a number of its own is refused,
 * since its share would be ambiguous.
 *
 * `{ cross: true }` puts the levels on different axes instead of one: the first
 * divides the width into strips, and the second divides each of those strips by
 * height. That is the mosaic, and only the leaves are drawn.
 *
 * @param {...(Column|object)} levels The hierarchy's columns, outermost first,
 *   and then, if you need it, an options object.
 * @param {object} [options] Settings, by name, after the columns.
 * @param {boolean} [options.cross] `true` crosses the levels instead of nesting
 *   them: the mosaic. `false` unless given.
 * @returns {Atom} The transform, with its levels.
 *
 * @example
 * const titanic = {
 *   travel_class: ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
 *   survived: ["No", "No", "No", "No", "Yes", "Yes", "Yes", "Yes"],
 *   people: [122, 167, 528, 673, 203, 118, 178, 212],
 * };
 *
 * // Rings: the people aboard by class, and within each class by survival.
 * const r = plot(data(titanic),
 *   layer(zone, partition(col.travel_class, col.survived)),
 *   x(col.people), color(col.travel_class), polar());
 *
 * // A mosaic: each class as wide as its count, and split by survival.
 * const m = plot(data(titanic),
 *   layer(zone, partition(col.travel_class, col.survived, { cross: true })),
 *   x(col.people), color(col.survived),
 *   style({ border_color: "white", border_size: 2 }));
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

/**
 * A transform: show how an amount moves through a series of stages (the flow
 * diagram).
 *
 * The stages are given as columns, in reading order: one row of the table is
 * one path through all of them, and `flow(col.class, col.sex, col.survived)`
 * runs each row from its class to its survival. Rows with the same path are
 * added together; other columns are ignored.
 *
 * Three marks can draw it. `layer(ribbon, flow(...))` draws the bands,
 * `layer(zone, flow(...))` the stacked slots of each stage, and
 * `layer(text, flow(...))` with `label(col.name)` writes each slot's name on
 * it. Give `y` the column that sizes each path; with no `y`, every path counts
 * as 1. Given to `x` instead, the stages run from top to bottom. On the band
 * layer, `color()` of a stage colors every band by the category its path holds
 * at that stage. `color()` of any other categorical column splits each band
 * by its values. Set the slots' color with `style()`.
 *
 * The stage axis comes from the columns given to `flow()`, so `x()` or `y()`
 * carries only the count, never a stage. To reorder the stages, reorder the
 * arguments.
 *
 * Two columns that name one set of places take `{ shared: true }`:
 * `flow(col.exporter, col.importer, { shared: true })` reads each row as an
 * amount leaving one place and arriving at another, and gives each place one
 * slot. In the ordinary space, that draws the arc diagram; in `polar()` it
 * draws the chord diagram. Each slot is in two parts. The first part, in the
 * slot's color, holds the bands that leave the place; the second, a shade
 * darker, holds the bands that arrive at it.
 *
 * A list of links whose places are arranged in layers takes
 * `{ layered: true }`. A layer here is a column of places in the drawing, not a
 * `layer()`. `flow(col.source, col.target, { layered: true })` puts each place
 * in the first layer after every place that sends to it. That draws the Sankey
 * diagram. Links that form a cycle have no layers, and are refused.
 *
 * @param {...(Column|object)} stages The stage columns, in reading order, at
 *   least two, and then, if you need it, an options object.
 * @param {object} [options] Settings, by name, after the columns.
 * @param {boolean} [options.shared] `true` when the two columns name one set of
 *   places, the place each amount leaves and the place it arrives at. `false`
 *   unless given.
 * @param {boolean} [options.layered] `true` when each row is a link between two
 *   places, and the places are arranged in layers worked out from the links:
 *   the Sankey diagram. `false` unless given. Not together with `shared: true`.
 * @returns {Atom} The transform, with its stages.
 *
 * @example
 * const titanic = {
 *   travel_class: ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
 *   survived: ["No", "No", "No", "No", "Yes", "Yes", "Yes", "Yes"],
 *   people: [122, 167, 528, 673, 203, 118, 178, 212],
 * };
 *
 * // The people aboard, from class to survival. Each band is as wide as
 * // its count.
 * const p = plot(data(titanic),
 *   layer(ribbon, flow(col.travel_class, col.survived)), y(col.people));
 *
 * // The bands, the slots, and the name of each slot.
 * const q = plot(data(titanic), y(col.people),
 *   layer(ribbon, flow(col.travel_class, col.survived)),
 *   color(col.travel_class),
 *   layer(zone, flow(col.travel_class, col.survived)),
 *   layer(text, flow(col.travel_class, col.survived)), label(col.name));
 */
export function flow(...stages) {
  let options = {};
  if (stages.length && isOptions(stages[stages.length - 1])) {
    options = stages.pop();
  }
  if (stages.length < 2) {
    throw new GogError(
      "gog: `flow()` needs at least two stage columns, in reading order — " +
        "`flow(col.class, col.sex, col.survived)` runs each row from its " +
        "`col.class` to its `col.survived`. One column has no between."
    );
  }
  const { shared = false, layered = false, ...rest } = options;
  const unknown = Object.keys(rest);
  if (unknown.length) {
    throw new GogError(
      "gog: `flow()` takes `shared` and `layered` — `flow(col.exporter, col.importer, " +
        `{ shared: true })\` lays two columns that name one set of places on one axis, ` +
        "and `flow(col.source, col.target, { layered: true })` stands them in layers " +
        `worked out from the links. Got: \`${unknown.join("`, `")}\`.`
    );
  }
  if (typeof shared !== "boolean") {
    throw new GogError(
      "gog: `flow({ shared })` is true or false — true lays two columns that name " +
        "one set of places on one axis, `flow(col.exporter, col.importer, " +
        "{ shared: true })`; false runs the columns as stages side by side."
    );
  }
  if (typeof layered !== "boolean") {
    throw new GogError(
      "gog: `flow({ layered })` is true or false — true reads each row as a link " +
        "and stands the places in layers worked out from the links, " +
        "`flow(col.source, col.target, { layered: true })`; false runs the " +
        "columns as stages side by side."
    );
  }
  return new Atom("transform", {
    transform: "flow",
    stages: stages.map((stage) => columnName(stage, "flow")),
    // Sent only when true, as `partition`'s `cross` is, so a stage flow's wire
    // form is the bytes it always was — `carry` drops an `undefined`.
    shared: shared ? true : undefined,
    layered: layered ? true : undefined,
  });
}

/**
 * A transform: place the nodes of a graph, from a table of its edges.
 *
 * The two columns name each row's ends, and one row of the table is one edge
 * between its two values. The nodes come from the rows: their names from the
 * two columns together, in the column `name`, and their `degree` from counting
 * the rows that name them. The engine computes every position, the same way in
 * every language, so one table is one picture wherever it is drawn.
 *
 * Three marks can draw this placement, inside `network()`:
 * `layer(edge, layout(col.from, col.to))` draws the connections,
 * `layer(point, layout(...))` the nodes, and `layer(text, layout(...))` with
 * `label(col.name)` names them. On the node layers, `size(col.degree)` and
 * `color(col.degree)` show that count. The positions mean nothing as quantities,
 * so nothing binds to `x`, `y` or `z`. For three dimensions, give the space an
 * angle: `network({ turn: 30, tilt: 25 })`.
 *
 * @param {Column} from The column naming each edge's first end.
 * @param {Column} to The column naming each edge's second end.
 * @returns {Atom} The transform, with its two columns.
 *
 * @example
 * const links = {
 *   sender: ["Ann", "Ann", "Bob", "Cy", "Dee"],
 *   receiver: ["Bob", "Cy", "Cy", "Dee", "Ann"],
 * };
 *
 * // Who sends work to whom: the links, the people as points, and their names.
 * const p = plot(data(links),
 *   layer(edge, layout(col.sender, col.receiver)),
 *   layer(point, layout(col.sender, col.receiver)),
 *   layer(text, layout(col.sender, col.receiver), repel), label(col.name),
 *   network());
 */
export function layout(from, to) {
  if (from === undefined || to === undefined) {
    throw new GogError(
      "gog: `layout()` takes the two endpoint columns, in order — " +
        "`layout(col.from, col.to)`. One row of the table is one edge, and " +
        "those columns name the two nodes it connects."
    );
  }
  return new Atom("transform", {
    transform: "layout",
    from: columnName(from, "layout"),
    to: columnName(to, "layout"),
  });
}

// Join the closest leaves, one pair at a time — the cluster tree. The leaves are
// the levels of the bound categorical position, each described by its `value`
// at every level of `over`; the engine joins the closest profiles until one
// tree holds them all, and the unbound position carries the merge distance.
// `path(cluster(col.amount, { over: col.nutrient }), x(col.food))` draws the
// tree; `zone(cluster({ over: col.nutrient }), ...)` reorders a tile plot's
// slots to the tree's leaf order, the values already being on `color`. The
// statistic is fixed: Euclidean distance on the values as given, joined at
// average distance.
/**
 * A transform: join the closest leaves, one pair at a time, into a tree.
 *
 * The leaves are the categories on the position you name, such as `x(col.car)`,
 * and each is described by a profile: its `value` at every level of the `over`
 * column. The engine measures the distance between every pair of profiles,
 * joins the closest two, and keeps joining until one tree holds them all. Leave
 * the other position out: it carries the distance at which each pair joined.
 *
 * Two marks can draw the tree. `layer(path, cluster(...))` draws it, each join
 * at the height where its two branches meet; bind `y` instead of `x` to lay it
 * on its side. `layer(zone, cluster({ over: col.<profile> }))` is a tile plot
 * with its leaves reordered to the tree's order, so similar rows are next to
 * each other; its values are already on `color()`, which is why `value` is left
 * out there.
 *
 * The statistic is fixed: distances are Euclidean on the values as given, and
 * branches join at their average distance. If the units should not affect the
 * distances, rescale the value column in the table first.
 *
 * @param {Column|object} [value] The column of numbers the distances are measured on.
 *   Left out on a tile plot, where `color()` names the values, and then the
 *   options object comes first: `cluster({ over: col.measure })`.
 * @param {object} [options] Settings, by name.
 * @param {Column} [options.over] The profile's column, of text: each leaf holds
 *   one value at every level of it. Leave it out only when the table has one
 *   row for each leaf.
 * @returns {Atom} The transform, with its columns.
 *
 * @example
 * const cars = {
 *   car: ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
 *         "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // The cars joined by how close their miles per gallon are.
 * const p = plot(data(cars), layer(path, cluster(col.mpg)), x(col.car));
 *
 * @example
 * const cars = {
 *   car: ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
 *         "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 *   hp: [110, 110, 93, 110, 175, 105, 245, 62],
 * };
 *
 * // Each car measured three ways: one row for each car and measure.
 * const measures = ["wt", "mpg", "hp"];
 * const long = {
 *   car: measures.flatMap(() => cars.car),
 *   measure: measures.flatMap((m) => cars.car.map(() => m)),
 *   value: measures.flatMap((m) => cars[m]),
 * };
 *
 * // The tree, on its side. The horsepower is in larger units than the
 * // others, so it counts most in the distances.
 * const tree = plot(data(long),
 *   layer(path, cluster(col.value, { over: col.measure })), y(col.car));
 */
export function cluster(value, options = {}) {
  // The tile reading omits the value, so the one argument may be the options
  // object itself — the trailing-options idiom reading from either slot. A
  // column proxy is a `Column`; a plain object here is the options.
  if (value !== undefined && value !== null && typeof value === "object"
      && !(value instanceof Column)) {
    options = value;
    value = undefined;
  }
  refuseUnknownOptions(options, "cluster", ["over"]);
  const fields = { transform: "cluster" };
  if (value !== undefined && value !== null) {
    fields.value = columnName(value, "cluster");
  }
  if (options.over !== undefined && options.over !== null) {
    fields.over = columnName(options.over, "cluster");
  }
  return new Atom("transform", fields);
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
        "On a date axis use dates: `[new Date(2024, 0, 1), new Date(2024, 11, 31)]`. " +
        "Use `null` for an end the data should decide: `[0, null]`."
    );
  }
  // A domain on a temporal axis is written in dates, not epoch arithmetic:
  // `{ limits: [new Date(2024, 0, 1), new Date(2024, 11, 31)] }`. Each end is read
  // by the rule a column is (`dateSeconds`), on the local clock, so an end and the
  // column it bounds agree about which clock a date is on.
  const out = limits.map((end) => {
    if (end === null || end === undefined) return null;
    if (end instanceof Date) return dateSeconds([end]).seconds[0];
    if (typeof end !== "number" || !Number.isFinite(end)) {
      throw new GogError(
        "gog: `limits` needs two numbers, e.g. `x(col.hour, { limits: [0, 24] })`. " +
          "On a date axis use dates: `[new Date(2024, 0, 1), new Date(2024, 11, 31)]`. " +
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

// How many decimal places a label's numbers are printed with (spec §10): fixed
// places, so a column of labels reads as one column of figures. Checked here as a
// count; the engine says where it is drawn and refuses it on text.
function checkDecimals(decimals) {
  if (decimals === undefined || decimals === null) return null;
  if (typeof decimals !== "number" || Number.isNaN(decimals)) {
    throw new GogError(
      "gog: `decimals` needs one number, e.g. `label(col.life, { decimals: 1 })`. " +
      "It is how many decimal places each number is printed with.");
  }
  if (!Number.isInteger(decimals) || decimals < 0) {
    throw new GogError(
      `gog: \`decimals: ${decimals}\` is not a count of decimal places. ` +
      "Write a whole number from 0 up, e.g. `decimals: 1`.");
  }
  return decimals;
}

// Whether a channel's numbers are written as percents (spec §10):
// `y(col.proportion, { percent: true })` prints 0.25 as 25%. Checked here as true or
// false; the engine says where it is drawn and refuses it beside `short`.
function checkPercent(percent) {
  if (percent === undefined || percent === null) return null;
  if (percent !== true && percent !== false) {
    throw new GogError(
      "gog: `percent` is true or false — true writes a number as a percent, e.g. " +
        "`y(col.proportion, { percent: true })` prints 0.25 as 25%."
    );
  }
  return percent;
}

// Whether a label's large numbers are written short, with K, M, B or T
// (spec §10): `label(col.population, { short: true })` prints 3.81B. Checked here as
// true or false; the engine says where it is drawn and refuses it on text.
function checkShort(short) {
  if (short === undefined || short === null) return null;
  if (short !== true && short !== false) {
    throw new GogError(
      "gog: `short` is true or false — true writes a large number short, with K, M, " +
      "B or T, e.g. `label(col.population, { short: true })` prints " +
      "3811953827 as 3.81B.");
  }
  return short;
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
//
// Every channel takes it and forwards it, as every channel does `legend`, and
// the engine decides where it means something: only a position draws an axis
// in each panel, and on any other channel the engine refuses it with that
// direction. Leaving it out of `color()` would answer
// `color(col.continent, { free: true })` with "has no `free`", which names no
// fix. Only the shape is checked here, and its example is a position's.
function checkFree(free, name) {
  if (free === undefined || free === null || free === false) return false;
  if (free !== true) {
    const axis = ["x", "y", "z"].includes(name) ? name : "y";
    throw new GogError(
      `gog: \`free\` is true or false — it says whether this axis is fitted per ` +
        `panel. Which axis is up to which binding you write it on: ` +
        `\`${axis}(col.<name>, { free: true })\` frees ${axis}.`
    );
  }
  return true;
}

// `legend: false` — leave this channel's legend out of the plot (spec §10).
//
// Every channel takes it and forwards it, positions included, and the engine
// decides where it means something: a legend is drawn for `color`, `size`,
// `shape`, `pattern` and `opacity`, and on any other channel the engine refuses
// it with that direction. Deciding here would be a rule the other three bindings
// get wrong, and leaving it out of `x()` would answer `x(col.gdp, { legend:
// false })` with "has no `legend`", which names no fix. Only the shape is
// checked here, so the error lands on the line that wrote it.
function checkLegend(legend) {
  if (legend === undefined || legend === null) return null;
  if (legend !== true && legend !== false) {
    throw new GogError(
      "gog: `legend` is true or false — false leaves this channel's legend out of " +
        "the plot, e.g. `color(col.continent, { legend: false })` when the names " +
        "are written on the plot instead. To choose the side of the plot the " +
        'legends sit on, write `theme({ legend: "bottom" })`.'
    );
  }
  return legend;
}

// `axis: false` — leave this position's axis out of the plot (spec §10): its tick
// marks, its numbers and its name.
//
// `legend`'s rule one guide over: every channel takes it and forwards it, and the
// engine decides where it means something. Only `x`, `y` and `z` draw an axis,
// and on any other channel the engine refuses it with that direction, in all four
// bindings at once. Only the shape is checked here.
function checkAxis(axis) {
  if (axis === undefined || axis === null) return null;
  if (axis !== true && axis !== false) {
    throw new GogError(
      "gog: `axis` is true or false — false leaves this position's axis out of the " +
        "plot, its numbers and its name, e.g. `x(col.lon, { axis: false })` on a map. " +
        'To remove the gridlines as well, write `theme({ grid: "none" })`.'
    );
  }
  return axis;
}

// Every channel takes every parameter a binding can carry, checks its shape and
// forwards it: the engine decides where one means nothing and says so in the same
// words in all four bindings, where JavaScript would only say "has no `speed`".
const CHANNEL_ARGS = ["field", "scale", "base", "limits", "tick_count", "speed", "free", "legend",
                      "axis", "decimals", "short", "percent"];

function channelAtom(kind, name, raw, names = CHANNEL_ARGS) {
  const { field, scale, base, limits, tick_count: tickCount, speed, free, legend, axis,
    decimals, short, percent } = readArgs(raw, name, names);
  return new Atom(kind, {
    field: columnName(field, name),
    scale: checkScale(scale),
    base: checkBase(base),
    limits: checkLimits(limits),
    tick_count: checkTickCount(tickCount),
    speed: checkSpeed(speed),
    free: checkFree(free, name),
    legend: checkLegend(legend),
    axis: checkAxis(axis),
    decimals: checkDecimals(decimals),
    short: checkShort(short),
    percent: checkPercent(percent),
  });
}

function positionAtom(kind, name, raw) {
  return channelAtom(kind, name, raw);
}

// Bind the x axis to a column.
/**
 * Place the rows along the horizontal axis, by a column.
 *
 * Written before any mark, a position is the whole plot's, and every mark uses
 * it. Written after a mark, it belongs to that mark, which is how a second
 * table says where its own rows go. Every other layer that names no column of
 * its own for this axis reads it too. Either way there is one axis, with the
 * plot's scale, ticks and name.
 *
 * @param {Column} field The column to place along the horizontal axis.
 * @param {object} [options] Settings for the axis, by name.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] How a number
 *   becomes a position: `"linear"` (the default) or `"log"`. `"time"` and
 *   `"category"` are accepted only when the column already holds dates or
 *   categories, and there they change nothing. To show numbers as categories,
 *   store them as text first, or use `ordered()`. A log scale keeps the data in
 *   its own units and labels the ticks 1, 10, 100. It is not the same as
 *   plotting the logarithm of the column, which would run the axis 0, 1, 2,
 *   and the reader would have to convert each number back.
 * @param {number} [options.base] The base of a `"log"` scale; 10 unless given.
 *   Use 2 when each step doubles, and `Math.E` for the natural logarithm.
 *   Every base puts the data in the same places; it chooses only where the
 *   gridlines go and how they are labeled.
 * @param {Array<(number|Date|null)>} [options.limits] The two ends of the axis,
 *   `[low, high]`. `null` at either end leaves that end to the data, so
 *   `[0, null]` starts the axis at zero. On a date axis the ends are dates:
 *   `[new Date(2024, 0, 1), new Date(2024, 11, 31)]`. Rows outside the limits
 *   are left out, and gog says how many.
 * @param {number} [options.tick_count] About how many ticks to draw, a whole
 *   number of 2 or more; 5 unless given. The axis may draw a few more or fewer,
 *   because it rounds the tick values to readable numbers.
 * @param {number} [options.speed] Not taken: only `play()` has a speed, because
 *   only it shows frames one after another. gog refuses it when the plot is
 *   drawn, and says to write it in `play()`.
 * @param {boolean} [options.free] When the plot is split into panels, `true`
 *   lets each panel fit this axis to its own rows, instead of sharing one scale
 *   across all of them. On a plot with one panel it is refused.
 * @param {boolean} [options.legend] Not taken by a position, which is read off
 *   its axis rather than a legend. gog refuses it when the plot is drawn, and
 *   names the five channels that draw a legend: `color`, `size`, `shape`,
 *   `pattern` and `opacity`.
 * @param {boolean} [options.axis] `false` leaves this axis out of the plot: its
 *   tick marks, its numbers and its name, and the margin they used. The
 *   scale is unchanged, so the gridlines and the frame stay;
 *   `theme({ grid: "none" })` and `theme({ frame: "none" })` remove those.
 * @param {number} [options.decimals] How many decimal places each number on the
 *   axis is printed with, a whole number from 0 to 10. If the axis writes its
 *   numbers with a letter (K, M, B or T), the letter stays. If rounding would
 *   print one number on two ticks, the axis draws fewer ticks.
 * @param {boolean} [options.short] `true` writes each number on the axis short,
 *   with its own K, M, B or T. With it, an axis that reads 1000K, 1500K reads
 *   1M, 1.5M.
 * @param {boolean} [options.percent] `true` writes each number on the axis as a
 *   percent: 0.25 is 25%. `limits` stays in the column's own units. Not
 *   together with `short`.
 * @returns {Atom} The position, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 *   hp: [110, 110, 93, 110, 175, 105, 245, 62],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg));
 *
 * // A logarithmic axis, in powers of 10 or of 2.
 * const q = plot(data(cars), point, x(col.hp, { scale: "log" }), y(col.mpg));
 * const r = plot(data(cars), point, x(col.hp, { scale: "log", base: 2 }),
 *   y(col.mpg));
 */
export function x(...raw) {
  return positionAtom("coord_x", "x", raw);
}

// Bind the y axis to a column.
/**
 * Place the rows along the vertical axis, by a column.
 *
 * Written before any mark, a position is the whole plot's, and every mark uses
 * it. Written after a mark, it belongs to that mark, which is how a second
 * table says where its own rows go. Every other layer that names no column of
 * its own for this axis reads it too. Either way there is one axis, with the
 * plot's scale, ticks and name.
 *
 * @param {Column} field The column to place along the vertical axis.
 * @param {object} [options] Settings for the axis, by name.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] How a number
 *   becomes a position: `"linear"` (the default) or `"log"`. `"time"` and
 *   `"category"` are accepted only when the column already holds dates or
 *   categories, and there they change nothing. To show numbers as categories,
 *   store them as text first, or use `ordered()`. A log scale keeps the data in
 *   its own units and labels the ticks 1, 10, 100. It is not the same as
 *   plotting the logarithm of the column, which would run the axis 0, 1, 2,
 *   and the reader would have to convert each number back.
 * @param {number} [options.base] The base of a `"log"` scale; 10 unless given.
 *   Use 2 when each step doubles, and `Math.E` for the natural logarithm.
 *   Every base puts the data in the same places; it chooses only where the
 *   gridlines go and how they are labeled.
 * @param {Array<(number|Date|null)>} [options.limits] The two ends of the axis,
 *   `[low, high]`. `null` at either end leaves that end to the data, so
 *   `[0, null]` starts the axis at zero. On a date axis the ends are dates:
 *   `[new Date(2024, 0, 1), new Date(2024, 11, 31)]`. Rows outside the limits
 *   are left out, and gog says how many.
 * @param {number} [options.tick_count] About how many ticks to draw, a whole
 *   number of 2 or more; 5 unless given. The axis may draw a few more or fewer,
 *   because it rounds the tick values to readable numbers.
 * @param {number} [options.speed] Not taken: only `play()` has a speed, because
 *   only it shows frames one after another. gog refuses it when the plot is
 *   drawn, and says to write it in `play()`.
 * @param {boolean} [options.free] When the plot is split into panels, `true`
 *   lets each panel fit this axis to its own rows, instead of sharing one scale
 *   across all of them. On a plot with one panel it is refused.
 * @param {boolean} [options.legend] Not taken by a position, which is read off
 *   its axis rather than a legend. gog refuses it when the plot is drawn, and
 *   names the five channels that draw a legend: `color`, `size`, `shape`,
 *   `pattern` and `opacity`.
 * @param {boolean} [options.axis] `false` leaves this axis out of the plot: its
 *   tick marks, its numbers and its name, and the margin they used. The
 *   scale is unchanged, so the gridlines and the frame stay;
 *   `theme({ grid: "none" })` and `theme({ frame: "none" })` remove those.
 * @param {number} [options.decimals] How many decimal places each number on the
 *   axis is printed with, a whole number from 0 to 10. If the axis writes its
 *   numbers with a letter (K, M, B or T), the letter stays. If rounding would
 *   print one number on two ticks, the axis draws fewer ticks.
 * @param {boolean} [options.short] `true` writes each number on the axis short,
 *   with its own K, M, B or T. With it, an axis that reads 1000K, 1500K reads
 *   1M, 1.5M.
 * @param {boolean} [options.percent] `true` writes each number on the axis as a
 *   percent: 0.25 is 25%. `limits` stays in the column's own units. Not
 *   together with `short`.
 * @returns {Atom} The position, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 *   hp: [110, 110, 93, 110, 175, 105, 245, 62],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg));
 *
 * // A logarithmic axis.
 * const q = plot(data(cars), point, x(col.wt), y(col.hp, { scale: "log" }));
 *
 * // An axis that starts at zero.
 * const r = plot(data(cars), point, x(col.wt), y(col.mpg, { limits: [0, 40] }));
 */
export function y(...raw) {
  return positionAtom("coord_y", "y", raw);
}

// Bind the z axis to a column — one more vowel, not a chart type.
/**
 * Place the rows along a third axis, by a column, and draw the plot in a cube.
 *
 * A third position is one more channel, not a chart type: bind `z` and the same
 * marks draw in three dimensions. On a web page, `html_block()` lets the reader
 * turn the cube; `space()` sets the angle it is first seen from. Written before
 * any mark, a position is the whole plot's, and every mark uses it. Written
 * after a mark, it belongs to that mark, which is how a second table says where
 * its own rows go. Every other layer that names no column of its own for this
 * axis reads it too. Either way there is one axis, with the plot's scale, ticks
 * and name.
 *
 * @param {Column} field The column to place along the third axis.
 * @param {object} [options] Settings for the axis, by name.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] How a number
 *   becomes a position: `"linear"` (the default) or `"log"`. `"time"` and
 *   `"category"` are accepted only when the column already holds dates or
 *   categories, and there they change nothing. To show numbers as categories,
 *   store them as text first, or use `ordered()`. A log scale keeps the data in
 *   its own units and labels the ticks 1, 10, 100. It is not the same as
 *   plotting the logarithm of the column, which would run the axis 0, 1, 2,
 *   and the reader would have to convert each number back.
 * @param {number} [options.base] The base of a `"log"` scale; 10 unless given.
 *   Use 2 when each step doubles, and `Math.E` for the natural logarithm.
 *   Every base puts the data in the same places; it chooses only where the
 *   gridlines go and how they are labeled.
 * @param {Array<(number|Date|null)>} [options.limits] The two ends of the axis,
 *   `[low, high]`. `null` at either end leaves that end to the data, so
 *   `[0, null]` starts the axis at zero. On a date axis the ends are dates:
 *   `[new Date(2024, 0, 1), new Date(2024, 11, 31)]`. Rows outside the limits
 *   are left out, and gog says how many.
 * @param {number} [options.tick_count] About how many ticks to draw, a whole
 *   number of 2 or more; 5 unless given. The axis may draw a few more or fewer,
 *   because it rounds the tick values to readable numbers.
 * @param {number} [options.speed] Not taken: only `play()` has a speed, because
 *   only it shows frames one after another. gog refuses it when the plot is
 *   drawn, and says to write it in `play()`.
 * @param {boolean} [options.free] When the plot is split into panels, `true`
 *   lets each panel fit this axis to its own rows, instead of sharing one scale
 *   across all of them. On a plot with one panel it is refused.
 * @param {boolean} [options.legend] Not taken by a position, which is read off
 *   its axis rather than a legend. gog refuses it when the plot is drawn, and
 *   names the five channels that draw a legend: `color`, `size`, `shape`,
 *   `pattern` and `opacity`.
 * @param {boolean} [options.axis] `false` leaves this axis out of the plot: its
 *   tick marks, its numbers and its name, and the margin they used. The
 *   scale is unchanged, so the gridlines and the frame stay;
 *   `theme({ grid: "none" })` and `theme({ frame: "none" })` remove those.
 * @param {number} [options.decimals] How many decimal places each number on the
 *   axis is printed with, a whole number from 0 to 10. If the axis writes its
 *   numbers with a letter (K, M, B or T), the letter stays. If rounding would
 *   print one number on two ticks, the axis draws fewer ticks.
 * @param {boolean} [options.short] `true` writes each number on the axis short,
 *   with its own K, M, B or T. With it, an axis that reads 1000K, 1500K reads
 *   1M, 1.5M.
 * @param {boolean} [options.percent] `true` writes each number on the axis as a
 *   percent: 0.25 is 25%. `limits` stays in the column's own units. Not
 *   together with `short`.
 * @returns {Atom} The position, to write into `plot()`.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 *   sepal_width: [3.5, 3, 3.2, 3.1, 3.2, 3.2, 3.1, 2.3, 3.3, 2.7, 3, 2.9],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // Three measurements of each flower, in a cube.
 * const p = plot(data(flowers), point, x(col.sepal_length), y(col.sepal_width),
 *   z(col.petal_length), color(col.species));
 */
export function z(...raw) {
  return positionAtom("coord_z", "z", raw);
}

// `space` — the angle a 3-D plot is viewed from.
/**
 * View a three-dimensional plot from an angle.
 *
 * Binding `z` is what makes a plot three-dimensional. `space()` sets the angle
 * that plot is viewed from: a setting of the coordinate space, as a polar
 * plot's start angle is, rather than a channel, since it maps no column. Leave
 * it out and a plot in the cube is seen from above and to one side, a view that
 * shows all three axes. Write `space` bare for that view, or call it to
 * choose. The two angles may also be given in order: `space(60, 20)`.
 *
 * @param {object} [options] Settings, by name.
 * @param {number} [options.turn] Degrees the scene is turned around the
 *   vertical axis: which side it is viewed from. 30 unless given.
 * @param {number} [options.tilt] Degrees the eye is lifted above the floor: how
 *   steeply it looks down. 25 unless given.
 * @returns {Atom} The space, with its view.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 *   sepal_width: [3.5, 3, 3.2, 3.1, 3.2, 3.2, 3.1, 2.3, 3.3, 2.7, 3, 2.9],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // Three measurements of each flower, in a cube turned 60 degrees.
 * const p = plot(data(flowers), point, x(col.sepal_length), y(col.sepal_width),
 *   z(col.petal_length), color(col.species), space({ turn: 60 }));
 */
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
/**
 * Bend the plane into a circle: `x` becomes the angle and `y` the radius.
 *
 * The `x` axis of a bar chart becomes the circle of a rose. Nothing else about
 * the sentence changes: `layer(bar, count), x(col.month)` is a bar chart in the
 * ordinary space and a rose in polar, from the same words. One turn spans
 * exactly the range of `x`, so the end of the range meets its start with no
 * gap, and angles run clockwise from twelve o'clock. Write `polar` bare, or
 * call it to choose where the circle begins.
 *
 * @param {number} [start] Degrees clockwise from the top where the circle
 *   begins. 0 (the default) starts the axis at twelve o'clock; `-22.5` centers
 *   the first of eight categories there instead of starting it there.
 * @returns {Atom} The space, with its start.
 *
 * @example
 * const cars = {
 *   cylinders: ["6", "6", "4", "6", "8", "6", "8", "4"],
 * };
 *
 * // Counts of cars by number of cylinders, bent into a circle.
 * const p = plot(data(cars), layer(bar, count), x(col.cylinders), polar());
 */
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
/**
 * Pack the panel with nested regions: the treemap.
 *
 * Every row's measure becomes an area rather than a length, and the areas
 * divide the panel: a region's share of the picture is its share of the total.
 * The mark, the statistic and the channels stay the same, which is what makes
 * this a space and not a chart type. `layer(bar, sum, stack), y(col.revenue),
 * color(col.region)` is a stacked bar in the ordinary space and a pie in
 * `polar()`. Here the same sentence without `stack` is a treemap: the packing
 * gives every region its own place, so `stack` has nothing to do and is
 * refused.
 *
 * A packed panel has no axes: its two directions carry no variable, so the
 * color legend is what decodes the regions. Bind a position to split the
 * packing one level further. `dodge`, `stack` and `jitter` are refused here,
 * since in a packing no two marks land in the same place. `nest()` takes no
 * arguments.
 *
 * @returns {Atom} The space.
 *
 * @example
 * const titanic = {
 *   travel_class: ["1st", "2nd", "3rd", "Crew", "1st", "2nd", "3rd", "Crew"],
 *   people: [122, 167, 528, 673, 203, 118, 178, 212],
 * };
 *
 * // Each class aboard the Titanic as an area in proportion to its people.
 * const p = plot(data(titanic), layer(bar, sum), y(col.people),
 *   color(col.travel_class), nest());
 */
export const nest = callableAtom(new Atom("coord_nest", {}), (...raw) => {
  if (raw.length > 0) {
    throw new GogError("gog: `nest()` takes no arguments — a packing has no view to set.");
  }
  return new Atom("coord_nest", {});
});

// `globe` — the sphere itself, viewed: x is longitude, y is latitude.
//
// The marks that draw on a `map` draw here, standing at their places on the
// sphere's surface: a point is a place, a path bends along great circles, a
// rule is a meridian or a parallel, and a zone with `group` fills each region
// of a boundary. `turn` and `tilt` name the place the view faces — a bearing
// that wraps and a latitude that stops at the poles — and rows on the far half
// are hidden behind the sphere, with the count said out loud. The graticule is
// the panel grid.
/**
 * View the sphere itself: `x` is longitude and `y` is latitude, both in
 * degrees.
 *
 * Each mark is drawn at its place on the sphere's surface. The marks that draw
 * on a `map()` draw here: `point` is a place, `path` a route that bends along
 * great circles, `text` a name at a place, `rule` a meridian or a parallel, and
 * `zone` with `group()` fills each region of a boundary. `bar` with `z()` also
 * draws here, as a spike at each place. The graticule is the panel grid,
 * removed like any other with `theme({ grid: "none" })`.
 *
 * A globe shows one half of the earth. `turn` and `tilt` name the place the
 * view faces: `globe({ turn: 178, tilt: -18 })` puts Fiji at the center. Rows
 * on the far half are hidden behind the sphere, and gog says how many. On a web
 * page, `html_block()` lets the reader turn the globe. The two angles may also
 * be given in order: `globe(178, -18)`.
 *
 * `x` and `y` are both used for the place. A `bar` can still measure along the
 * radius with `z()`, one spike for each row; any other quantity goes on
 * `size()` or `color()`. A binned field on the sphere is not drawn yet: its
 * correct tiling is hexagonal, because rectangular bins do not cover equal
 * areas on a sphere, and that grid is not built.
 *
 * @param {object} [options] Settings, by name.
 * @param {number} [options.turn] Degrees of longitude the view faces. 0 unless
 *   given. Any number works: 360 gives the same view as 0.
 * @param {number} [options.tilt] Degrees of latitude the view faces, from -90
 *   to 90. 0 unless given.
 * @returns {Atom} The space, with its view.
 *
 * @example
 * const quakes = {
 *   lat: [-20.42, -20.62, -26, -17.97, -20.42, -19.68, -11.7, -28.11],
 *   long: [181.62, 181.03, 184.1, 181.66, 181.96, 184.31, 166.1, 181.93],
 * };
 *
 * // Earthquakes near Fiji, on a globe turned to face them.
 * const p = plot(data(quakes), point, x(col.long), y(col.lat),
 *   globe({ turn: 178, tilt: -18 }));
 */
export const globe = callableAtom(
  new Atom("coord_globe", { turn: 0, tilt: 0 }),
  (...raw) => {
    const { turn = 0, tilt = 0 } = readArgs(raw, "globe", ["turn", "tilt"]);
    for (const [name, value] of [["turn", turn], ["tilt", tilt]]) {
      if (typeof value !== "number" || !Number.isFinite(value)) {
        throw new GogError(
          `gog: \`globe({ ${name}: … })\` needs a single number of degrees, e.g. \`globe(178, -18)\`.`
        );
      }
    }
    return new Atom("coord_globe", { turn, tilt });
  }
);

/**
 * The space a graph is drawn in, where a `layout()` places its nodes.
 *
 * A layout's positions are the graph's, not the data's, so this space draws no
 * axes, no ticks and no grid: there is nothing there for them to number. It is
 * the one space a `layout()` draws in, and a `layout()` is the only thing drawn
 * in it.
 *
 * Giving a viewing angle makes the graph three-dimensional. `network()` with no
 * angle draws the graph in two dimensions; `network({ turn: 30, tilt: 25 })`
 * computes the placement in a cube and draws it from that angle, and an angle
 * left out takes the standard view. On a web page, `html_block()` lets the
 * reader turn that cube. The two angles may also be given in order:
 * `network(30, 25)`.
 *
 * @param {object} [options] Settings, by name. Leave both out for a graph in
 *   two dimensions.
 * @param {number} [options.turn] Degrees the scene is turned around the
 *   vertical axis, for the cube.
 * @param {number} [options.tilt] Degrees of elevation, for the cube.
 * @returns {Atom} The space, with its view if one was given.
 *
 * @example
 * const links = {
 *   sender: ["Ann", "Ann", "Bob", "Cy", "Dee"],
 *   receiver: ["Bob", "Cy", "Cy", "Dee", "Ann"],
 * };
 *
 * const p = plot(data(links),
 *   layer(edge, layout(col.sender, col.receiver)),
 *   layer(point, layout(col.sender, col.receiver)),
 *   network());
 *
 * // The same network, turned and tilted to give it depth.
 * const q = plot(data(links),
 *   layer(edge, layout(col.sender, col.receiver)),
 *   layer(point, layout(col.sender, col.receiver)),
 *   network({ turn: 35, tilt: 20 }));
 */
export const network = callableAtom(
  new Atom("coord_network", {}),
  (...raw) => {
    const { turn, tilt } = readArgs(raw, "network", ["turn", "tilt"]);
    const fields = {};
    for (const [name, value] of [["turn", turn], ["tilt", tilt]]) {
      if (value === undefined) continue;
      if (typeof value !== "number" || Number.isNaN(value)) {
        throw new GogError(
          `gog: \`network({ ${name}: … })\` needs a single number of degrees, e.g. \`network(30, 25)\`.`
        );
      }
      fields[name] = value;
    }
    return new Atom("coord_network", fields);
  }
);

// `map` — the sphere flattened onto the page: x is longitude, y is latitude.
//
// Both positions are spent on the place, so a mark that measures along an axis
// has none left; a quantity goes on a channel instead, which is what makes
// `size(col.x)` the proportional-symbol map.
//
// A sphere cannot be laid flat without giving something up, and area and angle
// cannot both survive. `preserve` names which one does: "area" (the default,
// Equal Earth) keeps every region's true size, which is what a map read by area
// needs; "angle" (Mercator) keeps every small shape's true form and pays for it
// in area.
/**
 * Flatten the sphere onto the page: `x` is longitude and `y` is latitude, both
 * in degrees.
 *
 * The projection decides where each place lands. Nothing else about the
 * sentence changes: `point` is a place, `path` a route through places in the
 * table's order, `text` a name at a place, and `rule` a meridian or a parallel.
 * `x` and `y` already give the place, so a quantity goes on a channel instead:
 * `size()` gives the proportional-symbol map, and `color()` shades each place.
 *
 * No flat map of a sphere keeps both true areas and true angles. The panel is
 * shaped by the projection, so `theme({ ratio })` is refused here. Write `map`
 * bare for the default, or call it to choose.
 *
 * @param {object} [options] Settings, by name.
 * @param {"area"|"angle"} [options.preserve] What the flattening keeps.
 *   `"area"` (the default, the Equal Earth projection) gives every region an
 *   area in proportion to its true size, which is what a map read by area
 *   needs. `"angle"` (Mercator) keeps the true shape of small regions, but
 *   areas grow toward the poles. On Mercator the poles are infinitely far
 *   away, so the map stops at 85.05 degrees of latitude. A row beyond that is
 *   drawn at the edge, and gog says how many were.
 * @returns {Atom} The space, with what it keeps.
 *
 * @example
 * const quakes = {
 *   lat: [-20.42, -20.62, -26, -17.97, -20.42, -19.68, -11.7, -28.11],
 *   long: [181.62, 181.03, 184.1, 181.66, 181.96, 184.31, 166.1, 181.93],
 * };
 *
 * // Earthquakes near Fiji, at their longitude and latitude.
 * const p = plot(data(quakes), point, x(col.long), y(col.lat), map());
 *
 * // The same places, keeping their shapes instead of their areas.
 * const q = plot(data(quakes), point, x(col.long), y(col.lat),
 *   map({ preserve: "angle" }));
 */
export const map = callableAtom(new Atom("coord_map", { preserve: "area" }), (...raw) => {
  const { preserve = "area" } = readArgs(raw, "map", ["preserve"]);
  // Validated at the line the caller wrote, rather than at the wire. The engine
  // checks it too — a rule implemented in one binding is a rule the other three
  // get wrong — but a reader is owed the error where they typed it.
  if (preserve !== "area" && preserve !== "angle") {
    throw new GogError(
      'gog: `map({ preserve: … })` takes "area" or "angle". "area" keeps every ' +
        "region's true size, which is what a map read by area needs; \"angle\" keeps " +
        "every small shape's true form and pays for it in area. A sphere cannot do both."
    );
  }
  return new Atom("coord_map", { preserve });
});

// ---------------------------------------------------------------------------
// Channels — they map a column, and earn a legend to decode it
// ---------------------------------------------------------------------------

function scaledChannel(kind) {
  return (...raw) => channelAtom(kind, kind, raw);
}

// The same parameters as a scaled channel; the name says which channels the
// engine refuses a scale on, not which arguments the function takes.
function plainChannel(kind) {
  return (...raw) => channelAtom(kind, kind, raw);
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
/**
 * Color each mark by a column.
 *
 * A categorical column (text, or a column made with `ordered()`) gives each
 * category its own color from the palette, and a legend to decode it. A column
 * of numbers runs along a ramp from low to high. `color()` maps a column; to
 * paint every mark one color, use the setting instead,
 * `style({ color: "tomato" })`. `palette()` chooses the colors.
 *
 * @param {Column} field The column to map. Categories pick colors from the
 *   palette; numbers run along a ramp.
 * @param {object} [options] Settings, by name.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] How a number is
 *   read along the color scale: `"linear"` (the default) or `"log"`. `"time"`
 *   and `"category"` are accepted only when the column already holds dates or
 *   categories, and there they change nothing. To show numbers as categories,
 *   store them as text first, or use `ordered()`.
 * @param {number} [options.base] The base of a `"log"` scale; 10 unless given.
 *   Given without `scale: "log"`, it is refused. The base does not change
 *   the picture: the scale is fitted to its own range, and the legend shows
 *   the same values whatever the base.
 * @param {Array<(number|null)>} [options.limits] The two ends of the color
 *   scale, `[low, high]`, for a column of numbers. `null` at either end leaves
 *   that end to the data.
 * @param {number} [options.tick_count] Not taken: only a position, `x`, `y` or
 *   `z`, has an axis with ticks to count. gog refuses it when the plot is
 *   drawn.
 * @param {number} [options.speed] Not taken: only `play()` has a speed, because
 *   only it shows frames one after another. gog refuses it when the plot is
 *   drawn, and says to write it in `play()`.
 * @param {boolean} [options.free] Not taken: `free` fits an axis to each
 *   panel's own rows, and only a position, `x`, `y` or `z`, has an axis. gog
 *   refuses it when the plot is drawn, and says to write it on a position.
 * @param {boolean} [options.legend] `false` leaves this channel's legend out of
 *   the plot. The column is still mapped; only the legend that decodes it is
 *   not drawn, for a plot that says the same thing another way, such as names
 *   written at the ends of the lines.
 * @param {boolean} [options.axis] Not taken: only a position, `x`, `y` or `z`,
 *   draws an axis. gog refuses it when the plot is drawn, and names the three
 *   positions.
 * @param {number} [options.decimals] How many decimal places each number in the
 *   legend is printed with, a whole number from 0 to 10.
 * @param {boolean} [options.short] `true` writes each number in the legend
 *   short, with K, M, B or T, as `label()` does.
 * @param {boolean} [options.percent] `true` writes each number in the legend as
 *   a percent: 0.25 is 25%. Not together with `short`.
 * @returns {Atom} The channel, to write into `plot()`. Written before any mark
 *   it is the whole plot's; written after a mark, that mark's alone.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // A category: one color for each species, and a legend.
 * const p = plot(data(flowers), point, x(col.sepal_length), y(col.petal_length),
 *   color(col.species));
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 *   hp: [110, 110, 93, 110, 175, 105, 245, 62],
 * };
 *
 * // A number: colors that run from low to high.
 * const q = plot(data(cars), point, x(col.wt), y(col.mpg), color(col.hp));
 */
export const color = scaledChannel("color");
// The British spelling of `color()`, refused with direction. gog writes
// American English throughout and accepts no second spelling — Law 2 applied
// to the vocabulary itself, since two ways to write one word is a silent
// letter the reader pays for. ggplot2 accepts both, so a reader arriving from
// there types `colour`; unexported it would be `undefined is not a function`,
// which names no fix. Exported for the same reason `facet` is (spec §13).
/**
 * Not a word in gog: write `color()`.
 *
 * gog writes American English throughout and accepts no second spelling, since
 * two ways to write one word is one more thing for every reader to learn.
 * ggplot2 accepts both `colour` and `color`, so someone who has used ggplot2
 * may type `colour`. It is exported only so that the mistake gets a message
 * naming `color()`, rather than "colour is not defined". The settings work the
 * same way: `style({ colour })` and `style({ border_colour })` are refused,
 * naming `color` and `border_color`.
 *
 * @returns {never} Nothing: it always throws.
 * @throws {GogError} Always, naming `color()`.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * try {
 *   colour(col.species);
 * } catch (error) {
 *   console.log(error.message); // gog spells it color(col.<name>)
 * }
 *
 * // What to write instead.
 * const p = plot(data(flowers), point, x(col.sepal_length), y(col.petal_length),
 *   color(col.species));
 */
export function colour() {
  throw new GogError(
    "gog: there is no `colour()` channel. gog spells it `color(col.<name>)`: " +
      "American English is the grammar's only spelling, and unlike ggplot2 " +
      "there is no British alternative."
  );
}
// Map size to a numeric column.
/**
 * Size each point or text label by a column of numbers.
 *
 * On `point` it is the radius, so a larger value draws a larger point; on
 * `text` it is the size of the letters. A legend decodes the sizes. To make
 * every mark one size, use the setting instead, `style({ size: 6 })`. On a
 * `map()` it gives the proportional-symbol map.
 *
 * @param {Column} field The column of numbers to map.
 * @param {object} [options] Settings, by name.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] How a number is
 *   read along the size scale: `"linear"` (the default) or `"log"`. `"time"`
 *   is accepted only on a column of dates, where it changes nothing;
 *   `"category"` is refused, since this channel needs numbers.
 * @param {number} [options.base] The base of a `"log"` scale; 10 unless given.
 *   Given without `scale: "log"`, it is refused. The base does not change
 *   the picture: the scale is fitted to its own range, and the legend shows
 *   the same values whatever the base.
 * @param {Array<(number|null)>} [options.limits] The two ends of the size
 *   scale, `[low, high]`, for a column of numbers. `null` at either end leaves
 *   that end to the data.
 * @param {number} [options.tick_count] Not taken: only a position, `x`, `y` or
 *   `z`, has an axis with ticks to count. gog refuses it when the plot is
 *   drawn.
 * @param {number} [options.speed] Not taken: only `play()` has a speed, because
 *   only it shows frames one after another. gog refuses it when the plot is
 *   drawn, and says to write it in `play()`.
 * @param {boolean} [options.free] Not taken: `free` fits an axis to each
 *   panel's own rows, and only a position, `x`, `y` or `z`, has an axis. gog
 *   refuses it when the plot is drawn, and says to write it on a position.
 * @param {boolean} [options.legend] `false` leaves this channel's legend out of
 *   the plot. The column is still mapped; only the legend that decodes it is
 *   not drawn, for a plot that says the same thing another way, such as names
 *   written at the ends of the lines.
 * @param {boolean} [options.axis] Not taken: only a position, `x`, `y` or `z`,
 *   draws an axis. gog refuses it when the plot is drawn, and names the three
 *   positions.
 * @param {number} [options.decimals] How many decimal places each number in the
 *   legend is printed with, a whole number from 0 to 10.
 * @param {boolean} [options.short] `true` writes each number in the legend
 *   short, with K, M, B or T, as `label()` does.
 * @param {boolean} [options.percent] `true` writes each number in the legend as
 *   a percent: 0.25 is 25%. Not together with `short`.
 * @returns {Atom} The channel, to write into `plot()`. Written before any mark
 *   it is the whole plot's; written after a mark, that mark's alone.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 *   hp: [110, 110, 93, 110, 175, 105, 245, 62],
 * };
 *
 * // Cars with more horsepower are drawn larger.
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg), size(col.hp));
 */
export const size = scaledChannel("size");
// Map opacity to a numeric column.
/**
 * Set each mark's opacity by a column of numbers.
 *
 * A larger value draws a more opaque mark, and a legend decodes the opacities.
 * To make every mark equally see-through, use the setting instead,
 * `style({ opacity: 0.3 })`.
 *
 * @param {Column} field The column of numbers to map.
 * @param {object} [options] Settings, by name.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] How a number is
 *   read along the opacity scale: `"linear"` (the default) or `"log"`.
 *   `"time"` is accepted only on a column of dates, where it changes nothing;
 *   `"category"` is refused, since this channel needs numbers.
 * @param {number} [options.base] The base of a `"log"` scale; 10 unless given.
 *   Given without `scale: "log"`, it is refused. The base does not change
 *   the picture: the scale is fitted to its own range, and the legend shows
 *   the same values whatever the base.
 * @param {Array<(number|null)>} [options.limits] The two ends of the opacity
 *   scale, `[low, high]`, for a column of numbers. `null` at either end leaves
 *   that end to the data.
 * @param {number} [options.tick_count] Not taken: only a position, `x`, `y` or
 *   `z`, has an axis with ticks to count. gog refuses it when the plot is
 *   drawn.
 * @param {number} [options.speed] Not taken: only `play()` has a speed, because
 *   only it shows frames one after another. gog refuses it when the plot is
 *   drawn, and says to write it in `play()`.
 * @param {boolean} [options.free] Not taken: `free` fits an axis to each
 *   panel's own rows, and only a position, `x`, `y` or `z`, has an axis. gog
 *   refuses it when the plot is drawn, and says to write it on a position.
 * @param {boolean} [options.legend] `false` leaves this channel's legend out of
 *   the plot. The column is still mapped; only the legend that decodes it is
 *   not drawn, for a plot that says the same thing another way, such as names
 *   written at the ends of the lines.
 * @param {boolean} [options.axis] Not taken: only a position, `x`, `y` or `z`,
 *   draws an axis. gog refuses it when the plot is drawn, and names the three
 *   positions.
 * @param {number} [options.decimals] How many decimal places each number in the
 *   legend is printed with, a whole number from 0 to 10.
 * @param {boolean} [options.short] `true` writes each number in the legend
 *   short, with K, M, B or T, as `label()` does.
 * @param {boolean} [options.percent] `true` writes each number in the legend as
 *   a percent: 0.25 is 25%. Not together with `short`.
 * @returns {Atom} The channel, to write into `plot()`. Written before any mark
 *   it is the whole plot's; written after a mark, that mark's alone.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 *   hp: [110, 110, 93, 110, 175, 105, 245, 62],
 * };
 *
 * // Cars with more horsepower are drawn more opaque.
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg), opacity(col.hp));
 */
export const opacity = scaledChannel("opacity");
// Group a line/path by a column, without giving each group a color.
/**
 * Split the rows into groups by a column, without giving each group a color.
 *
 * A `line` or a `path` joins the rows within each group, so `group()` draws one
 * line for each value of the column, all in one color. `color()` splits the
 * rows too, and colors each group as well: use it when the groups should look
 * different. A transform runs within each group, and `stack` and `dodge` use
 * the groups to know which mark is which.
 *
 * @param {Column} field The column whose values name the groups.
 * @param {object} [options] Settings, by name.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] Not taken: this
 *   channel tells categories apart rather than measuring them, so there is no
 *   scale along it. gog refuses it when the plot is drawn.
 * @param {number} [options.base] Not taken, for the same reason.
 * @param {Array<(number|null)>} [options.limits] Not taken, for the same
 *   reason.
 * @param {number} [options.tick_count] Not taken: only a position, `x`, `y` or
 *   `z`, has an axis with ticks to count. gog refuses it when the plot is
 *   drawn.
 * @param {number} [options.speed] Not taken: only `play()` has a speed, because
 *   only it shows frames one after another. gog refuses it when the plot is
 *   drawn, and says to write it in `play()`.
 * @param {boolean} [options.free] Not taken: `free` fits an axis to each
 *   panel's own rows, and only a position, `x`, `y` or `z`, has an axis. gog
 *   refuses it when the plot is drawn, and says to write it on a position.
 * @param {boolean} [options.legend] Not taken: `group` splits the rows without
 *   encoding anything, so it draws no legend. gog refuses it when the plot is
 *   drawn, and names the five channels that do.
 * @param {boolean} [options.axis] Not taken: only a position, `x`, `y` or `z`,
 *   draws an axis. gog refuses it when the plot is drawn, and names the three
 *   positions.
 * @param {number} [options.decimals] Not taken: this channel tells categories
 *   apart and prints no numbers. gog refuses it when the plot is drawn.
 * @param {boolean} [options.short] Not taken, for the same reason.
 * @param {boolean} [options.percent] Not taken, for the same reason.
 * @returns {Atom} The channel, to write into `plot()`. Written before any mark
 *   it is the whole plot's; written after a mark, that mark's alone.
 *
 * @example
 * const trees = {
 *   tree: ["1", "1", "1", "1", "2", "2", "2", "2", "3", "3", "3", "3"],
 *   age: [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
 *   circumference: [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108],
 * };
 *
 * // One line for each tree, all drawn in one color.
 * const p = plot(data(trees), line, x(col.age), y(col.circumference),
 *   group(col.tree));
 */
export const group = plainChannel("group");
// Map glyph shape to a categorical column.
/**
 * Give each category its own point shape.
 *
 * Each value of the column gets a shape, such as a circle, a square or a
 * triangle, and a legend decodes them. `shape` answers "which one?", so a scale
 * is refused: there is no distance between a circle and a square for a scale to
 * run along. To give every point one shape, use the setting instead,
 * `style({ shape: "square" })`.
 *
 * @param {Column} field The categorical column to map.
 * @param {object} [options] Settings, by name.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] Not taken: this
 *   channel tells categories apart rather than measuring them, so there is no
 *   scale along it. gog refuses it when the plot is drawn.
 * @param {number} [options.base] Not taken, for the same reason.
 * @param {Array<(number|null)>} [options.limits] Not taken, for the same
 *   reason.
 * @param {number} [options.tick_count] Not taken: only a position, `x`, `y` or
 *   `z`, has an axis with ticks to count. gog refuses it when the plot is
 *   drawn.
 * @param {number} [options.speed] Not taken: only `play()` has a speed, because
 *   only it shows frames one after another. gog refuses it when the plot is
 *   drawn, and says to write it in `play()`.
 * @param {boolean} [options.free] Not taken: `free` fits an axis to each
 *   panel's own rows, and only a position, `x`, `y` or `z`, has an axis. gog
 *   refuses it when the plot is drawn, and says to write it on a position.
 * @param {boolean} [options.legend] `false` leaves this channel's legend out of
 *   the plot. The column is still mapped; only the legend that decodes it is
 *   not drawn, for a plot that says the same thing another way, such as names
 *   written at the ends of the lines.
 * @param {boolean} [options.axis] Not taken: only a position, `x`, `y` or `z`,
 *   draws an axis. gog refuses it when the plot is drawn, and names the three
 *   positions.
 * @param {number} [options.decimals] Not taken: this channel tells categories
 *   apart and prints no numbers. gog refuses it when the plot is drawn.
 * @param {boolean} [options.short] Not taken, for the same reason.
 * @param {boolean} [options.percent] Not taken, for the same reason.
 * @returns {Atom} The channel, to write into `plot()`. Written before any mark
 *   it is the whole plot's; written after a mark, that mark's alone.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // A different shape for each species.
 * const p = plot(data(flowers), point, x(col.sepal_length), y(col.petal_length),
 *   shape(col.species));
 */
export const shape = plainChannel("shape");
// Map paint texture to a categorical column — `shape`'s twin.
/**
 * Give each category its own texture: like `shape`, but for lines and filled
 * regions.
 *
 * On a filled region (`bar`, `box`, `area`, `ribbon`, `zone`) each category
 * draws as a hatch, and on a stroke (`line`, `step`, `interval`, `path`,
 * `rule`, `edge`) as a dash. It separates categories without color, so they
 * stay distinct in a gray printout or for a color-blind reader. Using
 * `pattern()` and `color()` on the same column shows each category two ways,
 * which is the usual advice for accessible plots. A scale is refused: like
 * `shape`, it answers "which one?", not "how much?".
 *
 * Not to be confused with the setting `style({ pattern })`, which fixes one
 * texture for the whole layer; `pattern()` maps a column to several.
 *
 * @param {Column} field The categorical column to map.
 * @param {object} [options] Settings, by name.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] Not taken: this
 *   channel tells categories apart rather than measuring them, so there is no
 *   scale along it. gog refuses it when the plot is drawn.
 * @param {number} [options.base] Not taken, for the same reason.
 * @param {Array<(number|null)>} [options.limits] Not taken, for the same
 *   reason.
 * @param {number} [options.tick_count] Not taken: only a position, `x`, `y` or
 *   `z`, has an axis with ticks to count. gog refuses it when the plot is
 *   drawn.
 * @param {number} [options.speed] Not taken: only `play()` has a speed, because
 *   only it shows frames one after another. gog refuses it when the plot is
 *   drawn, and says to write it in `play()`.
 * @param {boolean} [options.free] Not taken: `free` fits an axis to each
 *   panel's own rows, and only a position, `x`, `y` or `z`, has an axis. gog
 *   refuses it when the plot is drawn, and says to write it on a position.
 * @param {boolean} [options.legend] `false` leaves this channel's legend out of
 *   the plot. The column is still mapped; only the legend that decodes it is
 *   not drawn, for a plot that says the same thing another way, such as names
 *   written at the ends of the lines.
 * @param {boolean} [options.axis] Not taken: only a position, `x`, `y` or `z`,
 *   draws an axis. gog refuses it when the plot is drawn, and names the three
 *   positions.
 * @param {number} [options.decimals] Not taken: this channel tells categories
 *   apart and prints no numbers. gog refuses it when the plot is drawn.
 * @param {boolean} [options.short] Not taken, for the same reason.
 * @param {boolean} [options.percent] Not taken, for the same reason.
 * @returns {Atom} The channel, to write into `plot()`. Written before any mark
 *   it is the whole plot's; written after a mark, that mark's alone.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // A different hatch for each species, so the bars differ without color.
 * const p = plot(data(flowers), layer(bar, mean), x(col.species),
 *   y(col.petal_length), pattern(col.species));
 */
export const pattern = plainChannel("pattern");
// Draw a column's values as text — the `text` mark's content.
/**
 * Give the text a `text` mark writes: one value of a column at each position.
 *
 * `label()` supplies what a `text` mark draws, the way `x()` and `y()` supply
 * where. A column of text is written as it is; a column of numbers is
 * formatted, and the options below choose how. It is `text`'s required channel,
 * and no other mark takes it, so a labeled scatter plot is two marks, `point`
 * and `text`. A scale is refused: a label is content, like `shape`, not an
 * amount for a scale to run along.
 *
 * @param {Column} field The column whose values are written.
 * @param {object} [options] Settings, by name.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] Not taken: a label
 *   writes each value as text rather than measuring it, so there is no scale
 *   along it. gog refuses it when the plot is drawn.
 * @param {number} [options.base] Not taken, for the same reason.
 * @param {Array<(number|null)>} [options.limits] Not taken, for the same
 *   reason.
 * @param {number} [options.tick_count] Not taken: only a position, `x`, `y` or
 *   `z`, has an axis with ticks to count. gog refuses it when the plot is
 *   drawn.
 * @param {number} [options.speed] Not taken: only `play()` has a speed, because
 *   only it shows frames one after another. gog refuses it when the plot is
 *   drawn, and says to write it in `play()`.
 * @param {boolean} [options.free] Not taken: `free` fits an axis to each
 *   panel's own rows, and only a position, `x`, `y` or `z`, has an axis. gog
 *   refuses it when the plot is drawn, and says to write it on a position.
 * @param {boolean} [options.legend] Not taken: a label is read where it is
 *   written, so it draws no legend. gog refuses it when the plot is drawn, and
 *   names the five channels that do.
 * @param {boolean} [options.axis] Not taken: only a position, `x`, `y` or `z`,
 *   draws an axis. gog refuses it when the plot is drawn, and names the three
 *   positions.
 * @param {number} [options.decimals] How many decimal places each number is
 *   printed with, a whole number from 0 to 10:
 *   `label(col.life, { decimals: 1 })` prints 70.72848 as 70.7. The places are
 *   fixed, so 77 prints as 77.0, and a column of labels reads as one column of
 *   figures.
 * @param {boolean} [options.short] `true` writes a large number short, with K
 *   for thousands, M for millions, B for billions and T for trillions:
 *   3811953827 prints as 3.81B. It keeps three significant digits unless
 *   `decimals` gives the places after the point.
 * @param {boolean} [options.percent] `true` prints each number as a percent:
 *   0.25 prints as 25%. With `decimals` it gives the places after the point.
 *   Not together with `short`.
 * @returns {Atom} The channel, to write into `plot()`. Written before any mark
 *   it is the whole plot's; written after a mark, that mark's alone.
 *
 * @example
 * const cars = {
 *   car: ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
 *         "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // Each car's name, written at its weight and miles per gallon.
 * const p = plot(data(cars), text, x(col.wt), y(col.mpg), label(col.car));
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // The points, each with its miles per gallon written beside it.
 * const q = plot(data(cars), x(col.wt), y(col.mpg), point,
 *   text, label(col.mpg, { decimals: 1 }), style({ nudge: "right" }));
 */
export const label = plainChannel("label");
// Cut the plot into frames and play them — the time dimension.
//
// `play` is `facet` read in time. Both split the rows by a column's distinct
// values; `across(plot, facet(col.continent))` lays the pieces out over the page
// and `play(col.year)` lays them out in sequence. Every scale, the color map and
// every legend are fitted across the whole sequence rather than per frame, so the
// axes hold still and only the data moves; a layer that does not bind `play` is
// drawn in every frame. A static image made from the plot shows the first frame.
// `speed` stays second, where a positional one has always gone.
/**
 * Cut the plot into frames, one for each value of a column, and play them in
 * turn.
 *
 * `play` splits the rows as `across()` does, but shows the pieces one after
 * another instead of side by side. So `play(col.year)` shows, one at a time,
 * the panels that a facet by year shows side by side.
 *
 * Every scale, the color map and every legend are computed from all the frames
 * together, never frame by frame, so the axes hold still and only the data
 * moves. A layer that does not bind `play` is drawn in every frame, so a
 * reference line stays in place while the other marks move. The frames need an
 * order: numbers and dates have one, and a column of text needs one declared
 * with `ordered()`, or gog warns that the frames run in the order of the rows.
 *
 * On a web page the frames play by themselves. A still picture made from its
 * SVG by a converter shows the first frame. `save_png()` refuses a plot with
 * more than one frame, and `save_gif()` writes all of them.
 *
 * @param {Column} field The column whose values become the frames.
 * @param {object} [options] Settings, by name.
 * @param {number} [options.speed] How many times faster than usual the frames
 *   run, one positive number: 2 is twice as fast, 0.5 half. At the usual speed,
 *   1, each frame shows for 0.8 seconds.
 * @param {"linear"|"log"|"time"|"category"} [options.scale] Not taken: each
 *   value is a frame of its own rather than an amount, so there is no scale
 *   along it. gog refuses it when the plot is drawn.
 * @param {number} [options.base] Not taken, for the same reason.
 * @param {Array<(number|null)>} [options.limits] Not taken, for the same
 *   reason.
 * @param {number} [options.tick_count] Not taken: only a position, `x`, `y` or
 *   `z`, has an axis with ticks to count. gog refuses it when the plot is
 *   drawn.
 * @param {boolean} [options.free] Not taken: `free` fits an axis to each
 *   panel's own rows, and only a position, `x`, `y` or `z`, has an axis. gog
 *   refuses it when the plot is drawn, and says to write it on a position.
 * @param {boolean} [options.legend] Not taken: each frame is named in the strip
 *   above the panel while it is shown, so `play` draws no legend. gog refuses
 *   it when the plot is drawn, and names the five channels that do.
 * @param {boolean} [options.axis] Not taken: only a position, `x`, `y` or `z`,
 *   draws an axis. gog refuses it when the plot is drawn, and names the three
 *   positions.
 * @param {number} [options.decimals] Not taken yet: the strip above the panel
 *   writes each frame's name in its own way. gog refuses it when the plot is
 *   drawn.
 * @param {boolean} [options.short] Not taken yet, for the same reason.
 * @param {boolean} [options.percent] Not taken yet, for the same reason.
 * @returns {Atom} The channel, to write into `plot()`. Written before any mark
 *   it is the whole plot's; written after a mark, that mark's alone.
 *
 * @example
 * // `ordered()` gives the trees an order, which a sequence of frames needs.
 * const trees = {
 *   tree: ordered(["1", "1", "1", "1", "2", "2", "2", "2", "3", "3", "3", "3"],
 *                 ["1", "2", "3"]),
 *   age: [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
 *   circumference: [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108],
 * };
 *
 * // One frame for each tree. On a web page, the plot shows them in turn.
 * const p = plot(data(trees), point, x(col.age), y(col.circumference),
 *   play(col.tree));
 */
export const play = (...raw) => channelAtom("play", "play", raw,
  ["field", "speed", "scale", "base", "limits", "tick_count", "free", "legend", "axis",
   "decimals", "short", "percent"]);

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
/**
 * Let the reader select rows, and dim the rest.
 *
 * `brush` selects some of one column's values: a range of numbers, or a set of
 * categories. Rows inside it keep the plot's colors; rows outside it are
 * dimmed, so the reader sees the selected rows beside all the others. Where the
 * page can run the engine, as in a block from `html_block()`, dragging moves
 * the selection; in a saved file it stays where the sentence put it.
 *
 * A brush highlights. It never removes rows: removing rows before the
 * statistics run is what a position's `limits` does, and gog reports how many
 * rows it removed. One column for each `brush`; write two for a rectangle. A
 * layer can be brushed when it draws a shape for each row: `point`, `bar`,
 * `text`, `rule` and `zone`, an `interval` with `bounds()`, and a `ribbon` with
 * `flow()`. A layer that draws one shape through many rows, such as a `line`,
 * or that summarizes rows, such as `layer(bar, count)`, has no single row to
 * select. If another layer can be brushed, this layer is drawn in full, and gog
 * says so; if no layer can be, the brush is refused.
 *
 * @param {Column} [field] The column the selection is read on.
 * @param {object} [options] Settings, by name.
 * @param {Array<number>|Array<string>|string} [options.at] Where the selection
 *   opens: two numbers on a column that measures, `[low, high]`, or the names
 *   to select on a categorical column. Left out, nothing is selected, and the
 *   plot draws exactly as it would with no brush.
 * @returns {Atom} The selection, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // On a web page, a drag across the plot selects rows by weight.
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg), brush(col.wt));
 *
 * // A selection written in the sentence: the cars from 20 to 30 miles per
 * // gallon.
 * const q = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   brush(col.mpg, { at: [20, 30] }));
 */
export const brush = callableAtom(new Atom("brush", { field: "" }), (...raw) => {
  const { field, at } = readArgs(raw, "brush", ["field", "at"]);
  const name = field === undefined || field === null ? "" : columnName(field, "brush");
  return new Atom("brush", { field: name, ...checkBrushAt(at) });
});

// ---------------------------------------------------------------------------
// Settings — they fix a value, map nothing, and earn no legend (spec §7)
// ---------------------------------------------------------------------------

const STYLE_STRINGS = ["color", "shape", "border_color"];
const STYLE_NUMBERS = ["opacity", "size", "border_size", "angle", "gap"];
const STYLE_FLAGS = ["caps", "center"];
const STYLE_VALUES = {
  nudge: ["up", "down", "left", "right"],
  pattern: ["solid", "dashed", "dotted", "dotdash", "longdash",
            "hatch", "crosshatch", "stripes", "grid", "dots"],
  arrow: ["end", "start", "both"],
  reach: ["panel", "edge"],
};
const STYLE_PROPS = [
  ...STYLE_STRINGS,
  ...STYLE_NUMBERS,
  ...STYLE_FLAGS,
  ...Object.keys(STYLE_VALUES),
];
// An example the setting itself would take: `style({ shape: "tomato" })` was the
// example a refused shape was given. The same values in all four bindings.
const STYLE_EXAMPLES = {
  color: '"tomato"', shape: '"square"', border_color: '"black"',
  opacity: "0.3", size: "6", border_size: "1.5", angle: "90", gap: "0.2",
};
// What a setting does, said after the values it takes, in the same words as the
// other three bindings.
const STYLE_MEANINGS = {
  caps: " — `caps: false` draws a bare linerange, `caps: true` (the default) an " +
    "error bar.",
  center: " — `center: false` hides a confidence interval's center dot, " +
    "`center: true` (the default) draws it.",
  nudge: " — which way a text label sits from its point.",
  arrow: " — which end of a `path` carries the head.",
};

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
/**
 * Set a visual property of the mark written before it, to one value.
 *
 * A channel maps a column: `color(col.species)` gives each species its own
 * color and draws a legend. `style()` sets one value for the whole layer
 * instead. It maps no column and draws no legend. It applies to the mark
 * written directly before it. Two `style()`
 * calls on one mark are combined; where both set the same property, the later
 * one wins.
 *
 * Each setting is available on every mark that can carry it, and refused on a
 * mark that cannot, with a message saying what to write instead. A setting that
 * is not on the list is refused by name, and the British spellings `colour`,
 * `border_colour` and `centre` are refused with the American one.
 *
 * @param {object} props The settings, by name. At least one.
 * @param {string} [props.color] A color name (`"steelblue"`) or a hex code
 *   (`"#4e79a7"`).
 * @param {number} [props.opacity] A number from 0 to 1, set as given: unlike
 *   the `opacity()` channel, there is no range of data to rescale from.
 * @param {number} [props.size] Pixels: a point's radius (4.5 unless set), or a
 *   line's width (2 unless set).
 * @param {"circle"|"square"|"triangle"|"diamond"|"cross"|"star"|"wye"} [props.shape]
 *   The shape a point draws; `"wye"` is a Y shape.
 * @param {string} [props.border_color] The outline color of a filled mark. On
 *   `text`, setting `border_color` or `border_size` draws a halo around each
 *   label. Its color is the panel's background unless `border_color` names
 *   one, and its width is 3 unless `border_size` gives one.
 * @param {number} [props.border_size] The outline's width; on `text`, the
 *   halo's width (see `border_color`).
 * @param {boolean} [props.caps] `true` (the default) draws the end caps on an
 *   `interval`, an error bar; `false` leaves a bare line.
 * @param {boolean} [props.center] `true` (the default) draws the dot at the
 *   center of an `interval` whose transform computes one, such as `confidence`;
 *   `false` hides it.
 * @param {"up"|"down"|"left"|"right"} [props.nudge] Which side of its point a
 *   text label is drawn on, so that the point stays visible.
 * @param {"solid"|"dashed"|"dotted"|"dotdash"|"longdash"|"hatch"|"crosshatch"|"stripes"|"grid"|"dots"} [props.pattern]
 *   One texture for the whole layer. A stroke (`line`, `step`, `interval`,
 *   `path`, `rule`, `edge`) takes a dash: `"solid"`, `"dashed"`, `"dotted"`,
 *   `"dotdash"` or `"longdash"`. A fill (`bar`, `box`, `area`, `ribbon`,
 *   `zone`) takes a texture: `"hatch"`, `"crosshatch"`, `"stripes"`, `"grid"`
 *   or `"dots"`.
 * @param {"end"|"start"|"both"} [props.arrow] Which end of a `path` or an
 *   `edge` carries an arrowhead.
 * @param {"panel"|"edge"} [props.reach] How far a `rule` crosses the panel:
 *   `"panel"` (the default) all the way, a reference line; `"edge"` a short
 *   tick at the start of the axis, a rug.
 * @param {number} [props.angle] Degrees to turn a `text` label,
 *   counterclockwise from horizontal: at 90 the text reads from bottom to top.
 *   Each label turns around its point.
 * @param {number} [props.gap] The share of the space each category gives a
 *   `bar` or a `box` that is left empty beside it, from 0 (neighbors touch) to
 *   just under 1. On a dodged mark it is a share of the mark's part of that
 *   space. Unset, a bar on categories
 *   leaves 0.2, a binned bar 0, and a box keeps its usual width.
 * @returns {Atom} The settings, to write into `plot()` after their mark.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // One color and opacity for every point: settings, not mappings.
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   style({ color: "tomato", opacity: 0.5 }));
 *
 * @example
 * const trees = {
 *   tree: ["1", "1", "1", "1", "2", "2", "2", "2", "3", "3", "3", "3"],
 *   age: [118, 484, 664, 1004, 118, 484, 664, 1004, 118, 484, 664, 1004],
 *   circumference: [30, 58, 87, 115, 33, 69, 111, 156, 30, 51, 75, 108],
 * };
 *
 * // A thin gray line for each tree.
 * const q = plot(data(trees), line, x(col.age), y(col.circumference),
 *   group(col.tree), style({ color: "gray", size: 1 }));
 */
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
          `\`style({ ${name}: ${STYLE_EXAMPLES[name]} })\`.`
      );
    }
    if (STYLE_NUMBERS.includes(name) && (typeof value !== "number" || !Number.isFinite(value))) {
      throw new GogError(
        `gog: \`style({ ${name}: … })\` needs a single number, e.g. ` +
          `\`style({ ${name}: ${STYLE_EXAMPLES[name]} })\`.`
      );
    }
    if (STYLE_FLAGS.includes(name) && typeof value !== "boolean") {
      throw new GogError(
        `gog: \`style({ ${name}: … })\` needs true or false${STYLE_MEANINGS[name] ?? "."}`
      );
    }
    if (STYLE_VALUES[name] && !STYLE_VALUES[name].includes(value)) {
      // `pattern` is the one setting whose values split by geometry: five are a
      // stroke's dash and five a fill's texture, and the engine refuses each on
      // the other. A flat list of ten hides that.
      if (name === "pattern") {
        throw new GogError(
          'gog: `style({ pattern: … })` needs a stroke\'s dash ' +
            '("solid", "dashed", "dotted", "dotdash", "longdash") ' +
            'or a fill\'s texture ' +
            '("hatch", "crosshatch", "stripes", "grid", "dots").'
        );
      }
      // `reach` says what each of its two values draws, which is the whole of
      // what a reader needs to choose between them.
      if (name === "reach") {
        throw new GogError(
          'gog: `style({ reach: … })` needs "panel" (the default — a `rule` all the ' +
            'way across, a reference line) or "edge" (a short tick at the start of ' +
            "that axis, a rug)."
        );
      }
      const allowed = STYLE_VALUES[name].map((v) => `"${v}"`).join(", ");
      throw new GogError(
        `gog: \`style({ ${name}: … })\` needs one of ${allowed}${STYLE_MEANINGS[name] ?? "."}`
      );
    }
  }

  return new Atom("style", { props: { ...props } });
}

// ---------------------------------------------------------------------------
// Plot-level atoms
// ---------------------------------------------------------------------------

// Order the categorical axis by a column.
// The column is required. `order({ desc: true })` alone reads as a way to run a
// category axis backward, and it never was; naming the category column itself
// is, so the refusal says that.
/**
 * Order the categories on an axis by a column.
 *
 * Name a column of numbers to sort the categories by their values, smallest
 * first: `order(col.mpg)`. This overrides an order declared with `ordered()`.
 * Name the categorical column itself to sort the categories by their own order:
 * alphabetical for plain text, and the declared order for a column made with
 * `ordered()`. The column is required.
 *
 * @param {Column} field The column to order by.
 * @param {object} [options] Settings, by name.
 * @param {boolean} [options.desc] `true` reverses the order, so the largest
 *   comes first. `false` unless given.
 * @returns {Atom} The order, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   car: ["Mazda RX4", "Mazda RX4 Wag", "Datsun 710", "Hornet 4 Drive",
 *         "Hornet Sportabout", "Valiant", "Duster 360", "Merc 240D"],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // Bars in order of their height, lowest first.
 * const p = plot(data(cars), bar, x(col.car), y(col.mpg), order(col.mpg));
 *
 * // Highest first.
 * const q = plot(data(cars), bar, x(col.car), y(col.mpg),
 *   order(col.mpg, { desc: true }));
 */
export function order(...raw) {
  const { field, desc = false } = readArgs(raw, "order", ["field", "desc"]);
  // `Boolean(desc)` read any non-empty string as true, so `{ desc: "no" }`
  // sorted largest first without a word.
  if (typeof desc !== "boolean") {
    throw new GogError(
      "gog: `order({ desc })` is true or false — true reverses the order, so the " +
        "largest value comes first, and false keeps it, smallest first."
    );
  }
  if (field === undefined) {
    const written = desc ? "order({ desc: true })" : "order()";
    const tail = desc ? ", { desc: true }" : "";
    throw new GogError(
      `gog: \`${written}\` names no column to order by. Name the category column ` +
        `itself to sort the axis by its own categories, \`order(col.<category>${tail})\`, ` +
        `or a column of values to sort them by, \`order(col.<value>${tail})\`.`
    );
  }
  return new Atom("order", {
    field: columnName(field, "order"),
    descending: desc,
  });
}

// Set the categorical palette — a name, a list of hex colors, or an object that
// binds a color to each level by name: `palette({ Asia: "tomato", Europe:
// "steelblue" })` gives each level the color beside it, whatever order the rows
// arrive in. Which names are levels is the engine's question, since only the
// table can answer it; this checks the shape of what was written. A `Map` is
// read the same way, because `JSON.stringify` would otherwise send it as `{}`.
/**
 * Choose the colors `color()` uses.
 *
 * The colors of a palette name or a list go to the categories in order: the
 * declared order of a column made with `ordered()`, or, for plain text, the
 * order in which the values first appear. The first category gets the first
 * color. If there are more categories than
 * colors, more are made so that no two categories share one, and gog says so.
 * An object gives each category its color by name instead, so the colors do not
 * depend on the order of the rows. Every name must be a category of the
 * `color()` column, and every category drawn needs a name. A `Map` is read the
 * same way as an object. For a column of numbers, a ramp name such as
 * `"viridis"` chooses the ramp.
 *
 * @param {string|Array<string>|Object<string, string>|Map<string, string>} pal
 *   A palette name: `"gog"` (the default), `"okabe"` or `"soft"` for
 *   categories, or a ramp such as `"viridis"`, `"magma"`, `"blue"` or
 *   `"blue_red"` for numbers. Or a list of colors, `["#1b9e77", "#d95f02"]`. Or
 *   a color for each category, `{ setosa: "tomato", virginica: "steelblue" }`.
 * @returns {Atom} The palette, to write into `plot()`.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa", "versicolor",
 *             "versicolor", "versicolor", "versicolor", "virginica",
 *             "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // The Okabe-Ito colors, chosen to stay distinct for colorblind readers.
 * const p = plot(data(flowers), point, x(col.sepal_length), y(col.petal_length),
 *   color(col.species), palette("okabe"));
 *
 * // A color for each species, by name.
 * const q = plot(data(flowers), point, x(col.sepal_length), y(col.petal_length),
 *   color(col.species),
 *   palette({ setosa: "tomato", versicolor: "gray", virginica: "steelblue" }));
 */
export function palette(pal) {
  if (typeof pal === "string") {
    return new Atom("palette", { value: { named: pal } });
  }
  if (Array.isArray(pal) && pal.every((c) => typeof c === "string")) {
    return new Atom("palette", { value: { custom: [...pal] } });
  }
  const entries = pal instanceof Map ? [...pal.entries()]
    : pal !== null && typeof pal === "object" && !Array.isArray(pal)
      && [Object.prototype, null].includes(Object.getPrototypeOf(pal)) ? Object.entries(pal)
    : null;
  if (entries !== null) {
    if (!entries.every(([level, color]) => typeof level === "string" && typeof color === "string")) {
      throw new GogError(
        "gog: `palette()` with names binds a color to each level, both written as " +
          'text, e.g. `palette({ Asia: "tomato", Europe: "steelblue" })`.'
      );
    }
    return new Atom("palette", { value: { levels: Object.fromEntries(entries) } });
  }
  // The same sentence in all four bindings, and only the examples spelled in each.
  throw new GogError(
    "gog: `palette()` takes a palette name, a list of colors, or a color for each " +
      'level by name, e.g. `palette("okabe")`, `palette(["#1b9e77", "#d95f02"])` ' +
      'or `palette({ Asia: "tomato" })`.'
  );
}

export const THEME_PRESETS = ["gog", "minimal", "bw"];
const GRID_VALUES = ["both", "x", "y", "none"];
const FRAME_VALUES = ["full", "axes", "none"];
const LEGEND_SIDES = ["right", "left", "top", "bottom"];

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
// `legend` is which side of the plot its legends sit on: "right" (the default),
// "left", "top" or "bottom". Above or below the plot the keys are laid in a row,
// which leaves the width beside the panel to the panel. To leave one legend out,
// write `legend: false` on its channel instead, as in
// `color(col.continent, { legend: false })`.
// `width` and `height` are how many pixels the plot asks for. Alone that is the
// image; composed into an arrangement with `beside()` or `below()` it is the
// plot's *cell*, and the plots that ask for nothing split what is left — which
// is how a marginal histogram says it is thin. One meaning in both places
// (Law 6), and not to be confused with `ratio`, which shapes the panel inside
// whatever room the plot was given.
/**
 * Set how the plot looks apart from its marks: the panel, the gridlines, the
 * text and the legends.
 *
 * Nothing here maps a column, so each property is a setting; but none of it
 * belongs to a mark either, which is why it is not `style()`. `style()` sets
 * how a mark is drawn and `theme()` sets everything around the marks, so the
 * two never set the same property. A named preset comes first, and the options
 * object adjusts it: `theme("bw", { grid: "none" })`. Two `theme()` calls in
 * one plot are combined; where both set the same property, the later one wins.
 *
 * Among the plots of `beside()` or `below()`, a `theme()` is the whole
 * figure's, and it may set only `width`, `height` and `gap`.
 *
 * @param {"gog"|"minimal"|"bw"} [preset] A named look: `"gog"` (the default),
 *   `"minimal"` (no gridlines) or `"bw"` (a white panel inside a full
 *   rectangle, with white facet strips: the journal look. Only the parts around
 *   the data change; the data keep their colors).
 * @param {object} [options] Settings, by name.
 * @param {"both"|"x"|"y"|"none"} [options.grid] Which gridlines are drawn,
 *   named by the axis whose ticks they mark. So the setting also works in
 *   `polar()`, where the y axis's gridlines are rings.
 * @param {number} [options.ratio] The panel's width divided by its height, one
 *   positive number. `1` is a square.
 * @param {number} [options.tick_angle] Degrees to turn the x tick labels, from
 *   -90 to 90. gog already turns labels that do not fit side by side; set an
 *   angle only to choose a different one.
 * @param {number} [options.font_size] The size of a tick label in pixels, at
 *   least 4. The other text follows it: the axis names are 1.2 times as large
 *   and the title 1.2 times larger again, rounded, so `11` (the default) gives
 *   11, 13 and 16, and `16` gives 16, 19 and 23. A size, not a multiplier, and
 *   it names no typeface.
 * @param {string} [options.background] The panel's fill: any color, including
 *   `"transparent"`.
 * @param {string} [options.strip] The fill of the band above each panel of a
 *   split plot, which names the value the panel holds. `theme("bw")` sets it
 *   white.
 * @param {string} [options.strip_text] The color of the band's label. Left out,
 *   gog picks whichever of its two default colors is easier to read on the
 *   band, so `theme({ strip: "black" })` already gives white text.
 * @param {"full"|"axes"|"none"} [options.frame] How the panel is bounded:
 *   `"full"` (a rectangle, as `theme("bw")` sets), `"axes"` (the default,
 *   bottom and left only) or `"none"`.
 * @param {"beside"|"end"} [options.axis_label] Where each axis's name is drawn:
 *   `"beside"` (the default, centered along its axis, which turns the y name
 *   upright) or `"end"` (at the axis's far end, level, so nothing is read
 *   sideways).
 * @param {"right"|"left"|"top"|"bottom"} [options.legend] Which side of the
 *   plot the legends are drawn on; `"right"` unless given. Above or below the
 *   plot the legend entries are written in a row, so the panel can use the
 *   full width. To leave one legend out, write `{ legend: false }` on its
 *   channel instead, as in `color(col.species, { legend: false })`.
 * @param {number} [options.width] How many pixels wide the plot is, at least
 *   40; 800 unless given. Composed with `beside()` or `below()`, it is the
 *   width of the plot's cell, and the plots that ask for nothing share what is
 *   left. Not to be confused with `ratio`, which shapes the panel inside
 *   whatever room the plot was given.
 * @param {number} [options.height] How many pixels high the plot is, at least
 *   40; 600 unless given. Composed, it is the height of the plot's cell.
 * @param {number} [options.gap] How many pixels apart an arrangement's plots
 *   are, 0 or more; 20 unless given. An arrangement's own setting: write it
 *   among the plots of `beside()` or `below()`. A single plot refuses it.
 * @returns {Atom} The settings, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // A square panel, with no gridlines.
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   theme({ ratio: 1, grid: "none" }));
 *
 * // A smaller picture.
 * const q = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   theme({ width: 400, height: 300 }));
 *
 * // A preset, adjusted.
 * const r = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   theme("bw", { frame: "axes" }));
 */
export function theme(...raw) {
  const {
    preset, grid, ratio, tick_angle, font_size, background, strip, strip_text,
    axis_label, legend,
    frame, width, height, gap,
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
    "axis_label",
    "legend",
    "width",
    "height",
    "gap",
  ]);

  if (preset === undefined && grid === undefined && ratio === undefined &&
      tick_angle === undefined && font_size === undefined &&
      background === undefined && strip === undefined &&
      strip_text === undefined && frame === undefined && axis_label === undefined &&
      legend === undefined && width === undefined && height === undefined &&
      gap === undefined) {
    throw new GogError(
      "gog: `theme()` sets nothing. Name a preset or a property, e.g. " +
        '`theme("minimal")` or `theme({ grid: "none", ratio: 1 })`.'
    );
  }
  if (preset !== undefined && typeof preset !== "string") {
    throw new GogError(
      'gog: `theme()` takes a preset name first — `theme("minimal")` — and ' +
        'everything else by name: `theme({ grid: "none" })`.'
    );
  }
  // Checked in the engine too (`check_theme`), which is what makes the rule the
  // grammar's rather than this binding's. Checking here as well is what puts the
  // error on the line that wrote it.
  if (grid !== undefined && !GRID_VALUES.includes(grid)) {
    throw new GogError(
      `gog: \`theme({ grid: … })\` is one of ` +
        `${GRID_VALUES.slice(0, -1).map((v) => `"${v}"`).join(", ")} or "${GRID_VALUES.at(-1)}".`
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
        "degrees. Names that do not fit side by side are turned without it, so " +
        "state an angle only to choose one, such as `tick_angle: 45`."
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
      'gog: `theme({ frame: … })` is one of "full" (a rectangle round the panel), ' +
        '"axes" (bottom and left only) or "none".'
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
  if (axis_label !== undefined && !["end", "beside"].includes(axis_label)) {
    throw new GogError(
      'gog: `theme({ axis_label: … })` is "end" or "beside" \u2014 where each ' +
      "axis's name sits: at the axis's far end, or centered along it.");
  }
  // `legend: false` is how a channel leaves its own key out, so it is the likeliest
  // thing to be written here too; the refusal points at the channel.
  if (typeof legend === "boolean") {
    throw new GogError(
      "gog: `theme({ legend: … })` says which side of the plot the legends sit on: " +
        '"right", "left", "top" or "bottom". To leave a legend out, write ' +
        "`legend: false` on the channel it decodes, as in " +
        "`color(col.continent, { legend: false })`."
    );
  }
  if (legend !== undefined && !LEGEND_SIDES.includes(legend)) {
    throw new GogError(
      'gog: `theme({ legend: … })` is one of "right", "left", "top" or "bottom": ' +
        "the side of the plot its legends sit on. To leave a legend out, write " +
        "`legend: false` on its channel."
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
          "with `beside()` or `below()` it sizes the plot's cell in the arrangement."
      );
    }
  }
  // How far apart an arrangement's plots are: the arrangement's own setting, as
  // its size is.
  if (gap !== undefined && (typeof gap !== "number" || !Number.isFinite(gap) || gap < 0)) {
    throw new GogError(
      "gog: `theme({ gap: … })` is how many pixels apart an arrangement's plots stand, so it " +
        "needs one number of 0 or more, e.g. `theme({ gap: 8 })`."
    );
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
    axis_label: axis_label ?? null,
    legend: legend ?? null,
    width: width ?? null,
    height: height ?? null,
    gap: gap ?? null,
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
/**
 * Set the plot's title.
 *
 * @param {string} value The title.
 * @returns {Atom} The text, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   title("Heavier cars go fewer miles on a gallon"));
 */
export function title(value) {
  return new Atom("title", { value: textValue(value, "title") });
}

// Set a subtitle, a second line under the title: smaller and lighter, for what
// the plot shows or where its numbers stop.
/**
 * Set a subtitle, a second line under the title.
 *
 * Smaller and lighter than the title: what the plot shows, or the period its
 * numbers cover, so that the title can stay short.
 *
 * @param {string} value The subtitle.
 * @returns {Atom} The text, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   title("Heavier cars go fewer miles on a gallon"),
 *   subtitle("Eight cars from 1973 and 1974"));
 */
export function subtitle(value) {
  return new Atom("subtitle", { value: textValue(value, "subtitle") });
}

// Set a caption, a small line at the bottom of the plot, lined up with the left
// side of the panel: the source of the numbers, as a printed figure carries one.
/**
 * Set a caption, a small line at the bottom of the plot.
 *
 * It is lined up with the left side of the panel: the source of the numbers, as
 * a printed figure carries one.
 *
 * @param {string} value The caption.
 * @returns {Atom} The text, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   title("Heavier cars go fewer miles on a gallon"),
 *   caption("Source: Motor Trend, 1974"));
 */
export function caption(value) {
  return new Atom("caption", { value: textValue(value, "caption") });
}

// Override the x-axis label.
/**
 * Set the name of the x axis.
 *
 * The axis is named after its column unless this says otherwise.
 *
 * @param {string} value The axis's name.
 * @returns {Atom} The text, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   x_label("Weight (1000 lb)"));
 */
export function x_label(value) {
  return new Atom("x_label", { value: textValue(value, "x_label") });
}

// Override the y-axis label.
/**
 * Set the name of the y axis.
 *
 * The axis is named after its column unless this says otherwise.
 *
 * @param {string} value The axis's name.
 * @returns {Atom} The text, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg),
 *   y_label("Miles per gallon"));
 */
export function y_label(value) {
  return new Atom("y_label", { value: textValue(value, "y_label") });
}

// Override the z-axis label.
/**
 * Set the name of the z axis.
 *
 * It names the z axis, as `x_label()` and `y_label()` do for the other two. It
 * is used only in a three-dimensional plot, where the axis names are written
 * along the cube's edges. Without `z()`, gog warns that the name is ignored.
 *
 * @param {string} value The axis's name.
 * @returns {Atom} The text, to write into `plot()`.
 *
 * @example
 * const hill = {
 *   x: [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4],
 *   y: [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4],
 *   height: [0.223, 0.435, 0.435, 0.223, 0.435, 0.846, 0.846, 0.435, 0.435,
 *            0.846, 0.846, 0.435, 0.223, 0.435, 0.435, 0.223],
 * };
 *
 * const p = plot(data(hill), surface, x(col.x), y(col.y), z(col.height),
 *   z_label("Height"));
 */
export function z_label(value) {
  return new Atom("z_label", { value: textValue(value, "z_label") });
}
