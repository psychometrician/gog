// spec.js — the plot under construction, and the four operators that build it
//
// The mirror of `py-pkg/gog/gog/spec.py` and `r-pkg/gog/R/spec.R`, with one
// difference that is the whole reason this binding exists: **JavaScript cannot
// overload `+`, `*`, `|` or `/`** (spec §8), so it is
// the one target that writes a different *sentence* rather than a
// differently-captured one. Four words carry the four operators:
//
//     data(gm) + bar * bin + x(life) | facet(era)                     R
//     plot(data(gm), layer(bar, bin), x(col.life), across(col.era))   here
//
//     `+`             the comma inside plot(…)
//     `*`             layer(…)
//     `|` + facet()   across(…)
//     `/` + facet()   down(…)
//
// They are the operators, not new grammar. `plot()` is what gives the comma its
// meaning, and `layer()` shows `*`'s tighter binding as **nesting** — a stronger
// signal than a precedence rule the reader has to already know. `facet` is not a
// separate word because in R it never was one: `|` and `/` are meaningless
// without a `facet()` on their right (`facet_join` refuses everything else,
// `+ facet(g)` included), so the pair was always one phrase.
//
// **What this binding does not need, and why.** Python's `Plot` copies itself on
// every `+`, because `+` has a left-hand side that a caller may still be holding
// — `base + color(…)` must not change what `base + size(…)` means. A variadic
// call has no left-hand side to share: `plot(…)` builds from its arguments and
// returns, so the copying that Law 6 forces on the other two bindings is free
// here. What must still be copied is an *atom's* fields, since the same
// `layer(bar, count)` value may appear in two sentences.

import { Column, columnName, describe } from "./columns.js";
import { GogError } from "./errors.js";
import { Query, rowsToColumns } from "./render.js";

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

/**
 * One word of a sentence, as a value.
 *
 * Every word you write in `plot()` is an `Atom`: a channel such as
 * `x(col.wt)`, a setting such as `style({ color: "tomato" })`, a table from
 * `data()`, and a layer from `layer()`. A bare mark or transform, such as
 * `point` or `count`, is a function that carries its atom, which is what lets
 * you write it without parentheses. You do not make an `Atom` yourself; the
 * words make them. The class is exported so that code can check for one with
 * `instanceof`.
 *
 * An atom is a value: give it a name, use it in more than one plot, and it
 * means the same thing in each.
 *
 * @param {string} kind What the atom is, such as `"coord_x"` for `x()` or
 *   `"layer"` for `layer()`.
 * @param {object} [fields] What the atom holds. An empty object unless given.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 *   cylinders: ["6", "6", "4", "6", "8", "6", "8", "4"],
 * };
 *
 * const counts = layer(bar, count);
 * console.log(counts instanceof Atom); // true
 * console.log(x(col.wt) instanceof Atom); // true
 *
 * // The same atom in two plots means the same thing in both.
 * const p = plot(data(cars), counts, x(col.cylinders));
 * const q = plot(data(cars), counts, x(col.cylinders), polar());
 */
export class Atom {
  constructor(kind, fields = {}) {
    this.kind = kind;
    this.fields = fields;
  }

  toString() {
    const name = this.fields.mark || this.fields.transform || this.kind;
    return this.fields.field ? `<gog ${name}(${this.fields.field})>` : `<gog ${name}>`;
  }

  [Symbol.for("nodejs.util.inspect.custom")]() {
    return this.toString();
  }
}

// An atom that is usable bare and also takes parameters.
//
// `layer(bar, bin)` and `layer(bar, bin(30))` have to reach the same code path,
// the way they do in R — where a transform used bare arrives as the function
// itself and `*` calls it with its defaults. JavaScript has a hook the other two
// bindings had to build: a function *is* an object, so the bare atom rides on
// the function that configures it.
export function callableAtom(bare, configure) {
  const atom = (...args) => configure(...args);
  atom.gogAtom = bare;
  atom.toString = () => bare.toString();
  atom[Symbol.for("nodejs.util.inspect.custom")] = () => bare.toString();
  return atom;
}

// An atom that takes no parameters at all — `point`, `mean`, `stack`.
//
// It is a function only so that calling it can say so. `layer(bar, mean())`
// would otherwise be "mean is not a function", which names neither the cause
// nor the fix; the atom knows its own name and can give both.
export function bareAtom(kind, fields) {
  const bare = new Atom(kind, fields);
  const name = fields.mark || fields.transform || kind;
  return callableAtom(bare, () => {
    throw new GogError(
      `gog: \`${name}\` takes no parameters — use it bare, e.g. \`layer(bar, ${name})\`.`
    );
  });
}

// The bare atom behind a value, whichever of the two forms it arrived in.
export function asAtom(value) {
  if (value instanceof Atom) return value;
  if (typeof value === "function" && value.gogAtom instanceof Atom) return value.gogAtom;
  return null;
}

function clone(value) {
  return value === undefined ? undefined : structuredClone(value);
}

// ---------------------------------------------------------------------------
// layer() — `*`, a mark with its transforms
// ---------------------------------------------------------------------------

// Move a transform's parameters onto the layer, where the engine reads them.
//
// `bin`'s count, `density`'s bandwidth, `confidence`'s level, `jitter`'s amount,
// `stack`'s share flag and baseline, `cumulative`'s share flag,
// and `bounds`' column names ride the *layer* on the wire (`layer.bin`, …), not
// the transform list — the transform list is names only. Absent parameters
// attach nothing, so a bare `layer(bar, bin)` stays on Sturges' rule.
const CARRIED = new Set(["bin", "density", "range", "confidence", "smooth_band", "straight_band", "deviation", "quantile", "jitter", "stack",
  "cumulative", "bounds", "partition", "flow", "layout", "cluster"]);

function carry(layer, transform) {
  const name = transform.fields.transform;
  if (!CARRIED.has(name)) return;
  const params = {};
  for (const [key, value] of Object.entries(transform.fields)) {
    if (key !== "transform" && value !== null && value !== undefined) params[key] = value;
  }
  if (Object.keys(params).length) layer.fields[name] = params;
}

/**
 * Join a mark to its transforms: one layer, such as bars of counts.
 *
 * `layer()` is how JavaScript writes the `*` of R, Python and Julia:
 * `layer(bar, count)` is their `bar * count`, bars whose heights are counts.
 * The mark comes first, then one or more transforms, which apply from left to
 * right. So `layer(step, bin, cumulative)` cuts the column into bins, counts
 * each bin, and then adds up the counts.
 *
 * Channels and settings do not go inside a layer. They follow it in `plot()`,
 * and a channel written right after a layer belongs to that layer alone.
 *
 * A layer is a value: name it once, use it in more than one plot, and it means
 * the same thing in each.
 *
 * @param {...(Atom|Function)} parts A mark, then its transforms. A bare one,
 *   such as `bar` or `count`, is written without parentheses; one given
 *   arguments is called, as in `bin(20)`.
 * @returns {Atom} The layer, to write into `plot()`.
 *
 * @example
 * const cars = {
 *   cylinders: ["6", "6", "4", "6", "8", "6", "8", "4"],
 * };
 *
 * // A mark joined to a transform is one layer: bars of counts.
 * const counts = layer(bar, count);
 * const p = plot(data(cars), counts, x(col.cylinders));
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883,
 *               4.7, 3.6, 1.95, 4.35, 1.833, 3.917],
 * };
 *
 * // Transforms apply from left to right: a cumulative histogram.
 * const h = plot(data(geyser), layer(step, bin, cumulative), x(col.eruptions));
 */
export function layer(...parts) {
  if (!parts.length) {
    throw new GogError(
      "gog: `layer()` combines a mark with a transform — `layer(bar, bin)`, " +
        "`layer(line, smooth)`. It is how JavaScript spells `*`."
    );
  }

  const atoms = parts.map((part, index) => {
    const atom = asAtom(part);
    if (!atom) {
      throw new GogError(
        `gog: \`layer()\` takes gog atoms — a mark, then its transforms. ` +
          `Got ${describe(part)} in position ${index + 1}.`
      );
    }
    return atom;
  });

  const [mark, ...rest] = atoms;
  if (mark.kind !== "mark") {
    throw new GogError(
      `gog: \`layer()\` starts with a mark — \`layer(bar, bin)\`, not ` +
        `\`layer(${mark.fields.transform || mark.kind}, …)\`. It is how JavaScript ` +
        `spells \`*\`, and \`*\` combines a mark with a transform.`
    );
  }

  const built = new Atom("layer", {
    mark: mark.fields.mark,
    transforms: [],
    encodings: {},
  });
  if (mark.fields.box !== undefined) built.fields.box = clone(mark.fields.box);

  for (const atom of rest) {
    if (atom.kind !== "transform") {
      // A channel inside `layer()` is the commonest way to mis-read the nesting,
      // and the fix is one comma out: a channel joins the sentence, not the mark.
      const label = atom.fields.field
        ? `${atom.kind}(col.${atom.fields.field})`
        : atom.kind;
      throw new GogError(
        `gog: \`layer()\` holds a mark and its transforms — \`${label}\` is a ` +
          `${atom.kind === "style" ? "setting" : "channel"}, and it joins the sentence ` +
          `rather than the mark: \`plot(data(df), layer(${built.fields.mark}, …), ${label})\`.`
      );
    }
    built.fields.transforms.push(atom.fields.transform);
    carry(built, atom);
  }

  return built;
}

// ---------------------------------------------------------------------------
// across() and down() — `|` and `/`, which always took a facet()
// ---------------------------------------------------------------------------

// `wrap` folds a long line of panels into a rectangle — the number is how many
// panels before the line turns. Which *way* the line runs is the word's to say,
// not the number's, exactly as it is the operator's in the other three bindings:
// `across(col.g, { wrap: 4 })` puts four to a row, `down(col.g, { wrap: 4 })`
// four to a column.
function facetAtom(kind, field, options, word) {
  // A second column is a crossing, not options: one word for each direction.
  if (options instanceof Column) {
    const other = word === "across" ? "down" : "across";
    throw new GogError(
      `gog: \`${word}()\` splits the plot by one column, and \`${options}\` is a second ` +
        `one. To cross two columns, split each way: \`${word}(${field}), ${other}(${options})\`.`
    );
  }
  // A bare count destructured to nothing and was dropped in silence.
  if (options !== undefined && options !== null && typeof options !== "object") {
    throw new GogError(
      `gog: \`${word}()\` takes the number of panels before the line turns in an ` +
        `options object: \`${word}(${field}, { wrap: ${JSON.stringify(options)} })\`.`
    );
  }
  const { wrap = null, ...rest } = options ?? {};
  const unknown = Object.keys(rest);
  if (unknown.length) {
    throw new GogError(
      `gog: \`${word}()\` takes a column and \`{ wrap }\`, not ` +
        `\`${unknown[0]}\`. Write \`${word}(col.g, { wrap: 4 })\`.`
    );
  }
  if (wrap !== null && (!Number.isInteger(wrap) || typeof wrap === "boolean")) {
    throw new GogError(
      `gog: \`${word}(col.g, { wrap })\` takes the number of panels to draw ` +
        `before the line of them turns — one whole number, e.g. \`{ wrap: 4 }\`.`
    );
  }
  return new Atom(kind, { field: columnName(field, word), wrap });
}

/**
 * Split the plot into panels side by side, one for each value of a column.
 *
 * `across()` is how JavaScript writes `| facet()` from R, Python and Julia.
 * The panels share one scale, which is what makes them comparable, and any
 * transform runs within each panel's rows. The column must hold categories: a
 * number is a position along an axis, not a name for a panel.
 *
 * Write `down()` as well to cross two columns into a grid: one panel column
 * for each value of the `across()` column, one panel row for each value of the
 * `down()` column. A plot is split once each way, so a second `across()` is
 * refused.
 *
 * @param {Column} field The column whose values name the panels.
 * @param {object} [options] Settings, by name.
 * @param {number} [options.wrap] How many panels to put in a row before
 *   starting a new row, one whole number: twelve panels with `{ wrap: 4 }` make
 *   three rows of four. Left out, the panels stay in one row. Wrapping is
 *   refused when both `across()` and `down()` are written, because that grid
 *   is already a rectangle.
 * @returns {Atom} The split, to write into `plot()`.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa",
 *             "versicolor", "versicolor", "versicolor", "versicolor",
 *             "virginica", "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // One panel for each species, side by side.
 * const p = plot(data(flowers), point, x(col.sepal_length), y(col.petal_length),
 *   across(col.species));
 *
 * // Two panels in each row.
 * const w = plot(data(flowers), point, x(col.sepal_length), y(col.petal_length),
 *   across(col.species, { wrap: 2 }));
 */
export function across(field, options) {
  return facetAtom("facet_col", field, options, "across");
}

/**
 * Split the plot into panels one above another, one for each value of a column.
 *
 * `down()` is how JavaScript writes `/ facet()` from R, Python and Julia. It
 * works like `across()`, with the panels one above another: the panels share
 * one scale, any transform runs within each panel's rows, and the column must
 * hold categories. Write
 * `across()` as well to cross two columns into a grid.
 *
 * @param {Column} field The column whose values name the panels.
 * @param {object} [options] Settings, by name.
 * @param {number} [options.wrap] How many panels to put in a column before
 *   starting a new column, one whole number: twelve panels with `{ wrap: 4 }`
 *   make three columns of four. Left out, the panels stay in one column.
 *   Wrapping is refused when both `across()` and `down()` are written, because
 *   that grid is already a rectangle.
 * @returns {Atom} The split, to write into `plot()`.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa",
 *             "versicolor", "versicolor", "versicolor", "versicolor",
 *             "virginica", "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * // One panel for each species, one above another.
 * const p = plot(data(flowers), point, x(col.sepal_length), y(col.petal_length),
 *   down(col.species));
 */
export function down(field, options) {
  return facetAtom("facet_row", field, options, "down");
}

// `facet` exists in JavaScript only to say where it went. A reader arriving from
// the manual will type it, and §12 says a refusal names the fix.
/**
 * Not a word in JavaScript: write `across()` or `down()` instead.
 *
 * In R, Python and Julia, `facet()` follows `|` or `/` and means nothing
 * without one. JavaScript has neither operator, so the pair is one word here:
 * `across(col.species)` for panels side by side, `down(col.species)` for panels
 * one above another. `facet()` is exported only so that writing it gets a
 * message naming those two words, rather than "facet is not defined".
 *
 * @param {Column} [field] The column you meant to split by. The message
 *   writes the two sentences with it.
 * @returns {never} Nothing: it always throws.
 * @throws {GogError} Always, naming `across()` and `down()`.
 *
 * @example
 * const flowers = {
 *   species: ["setosa", "setosa", "setosa", "setosa",
 *             "versicolor", "versicolor", "versicolor", "versicolor",
 *             "virginica", "virginica", "virginica", "virginica"],
 *   sepal_length: [5.1, 4.9, 4.7, 4.6, 7, 6.4, 6.9, 5.5, 6.3, 5.8, 7.1, 6.3],
 *   petal_length: [1.4, 1.4, 1.3, 1.5, 4.7, 4.5, 4.9, 4, 6, 5.1, 5.9, 5.6],
 * };
 *
 * try {
 *   facet(col.species);
 * } catch (error) {
 *   console.log(error.message); // names across(col.species) and down(col.species)
 * }
 *
 * // What to write instead.
 * const p = plot(data(flowers), point, x(col.sepal_length), y(col.petal_length),
 *   across(col.species));
 */
export function facet(field) {
  const name = field instanceof Column ? String(field) : "col.<name>";
  throw new GogError(
    `gog: JavaScript has no \`facet()\`, because it has no \`|\` or \`/\` to join it ` +
      `with — the pair is one word here. Panels side by side: \`across(${name})\`. ` +
      `Panels stacked: \`down(${name})\`.`
  );
}

// ---------------------------------------------------------------------------
// beside() and below() — `|` and `/` between two *plots*
//
//     beside(a, b)              side by side          R: a | b
//     below(a, b)               one above the other   R: a / b
//     below(top, beside(m, r))  nested: the marginal plot
//
// The same two operators the facet words spell, told apart by what they are
// given: `across(col.g)` splits one plot by a column, `beside(p, q)` arranges
// two plots. R and Python distinguish those by the operand's type on `|`; here
// they are two words, for the reason every operator is a word in this binding.
//
// Faceting is one plot split by a variable and sharing everything; composition
// is several plots in one picture, each keeping its own coordinate space (spec
// §11). What relates the composed plots is one rule, and the engine owns it:
// the same column on the same axis in two of them, read the same way, is one
// axis — one scale, one panel extent, drawn once (`render::page`).
// ---------------------------------------------------------------------------

// An arrangement of plots. It carries `spec` and `frames` exactly as a `Plot` does,
// because every host — `render_svg`, `save`, `show` — asks a figure for those
// two and nothing else.
/**
 * Two or more plots placed together in one figure.
 *
 * `beside()` and `below()` return one. It is drawn and saved exactly as a
 * `Plot` is: `render_svg()`, `save_svg()`, `save_png()` and the rest take
 * either. It holds what the engine reads, `spec`, and the tables its plots
 * read, `frames`. You do not make an `Arrangement` yourself. The class is
 * exported so that code can check for one with `instanceof`.
 *
 * @param {object} spec The arrangement as the engine reads it: how its cells
 *   are arranged, and the cells.
 * @param {Object<string, object>} frames Each table its plots read, by name.
 * @param {Set<string>} [anonymous] The table names gog made up, rather than
 *   names given with `data(table, { name })`. If two plots bring different
 *   tables under one such name, gog renames one of them.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg));
 * const q = plot(data(cars), layer(bar, bin), x(col.mpg));
 * const both = beside(p, q);
 * console.log(both instanceof Arrangement); // true
 */
export class Arrangement {
  constructor(spec, frames, anonymous = new Set()) {
    this.spec = spec;
    this.frames = frames;
    // Which of these names the binding invented rather than being handed. Only
    // a name the *author* wrote can clash: a generated one means nothing to
    // them, so it can be renamed to make room.
    this.anonymous = anonymous;
  }

  toString() {
    return `<gog arrangement: ${this.spec.cells.length} cells, ${this.spec.arrange}>`;
  }

  [Symbol.for("nodejs.util.inspect.custom")]() {
    return this.toString();
  }
}

// The theme properties that describe a *panel*, and so cannot be said about a
// arrangement. The engine holds the same list in `check_page_theme`; this copy
// is what puts the refusal on the line that wrote it.
const PANEL_THEME = ["preset", "grid", "ratio", "tick_angle", "font_size",
                     "background", "strip", "strip_text", "frame", "axis_label",
                     "legend"];

// A `theme()` among the figures is the arrangement's own, and the only atom an
// arrangement takes. Its subject is the figure rather than a panel: `theme({
// height: 310 })` says how big this arrangement is, which is the same sentence
// a plot writes about itself. When the plots on an arrangement ask for no size,
// the arrangement is the one place the figure's size can be written.
//
// Written in the argument list because that is how JavaScript spells `+`
// (spec §8): `beside(a, b, theme({ height: 310 }))` is R's
// `(a | b) + theme(height = 310)`, word for word.
function pageTheme(atom, word) {
  const named = PANEL_THEME.filter(
    (k) => atom.fields[k] !== null && atom.fields[k] !== undefined
  );
  if (named.length) {
    const written = named.includes("preset")
      ? `theme("${atom.fields.preset}")`
      : `theme({ ${named[0]}: … })`;
    throw new GogError(
      `gog: \`${written}\` describes a panel, and an arrangement is plots placed ` +
        `together rather than a panel of its own. On an arrangement, \`theme()\` ` +
        `states how big the figure ` +
        `is and how far apart its plots are — \`width\`, \`height\` and \`gap\` — ` +
        `and nothing else. Write this into the plot ` +
        `it describes, before composing: \`${word}(plot(data(df), ${written}, …), …)\`.`
    );
  }
  const theme = {};
  for (const key of ["width", "height", "gap"]) {
    if (atom.fields[key] !== null && atom.fields[key] !== undefined) {
      theme[key] = atom.fields[key];
    }
  }
  return theme;
}

// The next generated table name nothing is using: `data`, `data2`, …
function nextFreeName(taken) {
  if (!taken.has("data")) return "data";
  let n = 2;
  while (taken.has(`data${n}`)) n += 1;
  return `data${n}`;
}

// Rewrite every reference to a table, through nested arrangements. A name
// reaches the wire in exactly two places — the plot's own table, and a layer
// that reads a different one — so this is the whole rewrite. It copies rather
// than edits: the cells belong to the plots the caller still holds.
function renameTable(cells, oldName, newName) {
  return cells.map((cell) => {
    const next = { ...cell };
    if (next.data === oldName) next.data = newName;
    if (Array.isArray(next.layers)) {
      next.layers = next.layers.map((layer) =>
        layer && layer.data === oldName ? { ...layer, data: newName } : layer
      );
    }
    if (Array.isArray(next.cells)) next.cells = renameTable(next.cells, oldName, newName);
    return next;
  });
}

// Why an atom handed to `beside()` or `below()` is not a plot, and where it goes:
// `data()` begins a plot, and every other atom joins one. A mark and a transform
// are named as bare words (`point`, `mean`), every other atom as a call.
//
// The same sentence R, Python and Julia say for `arrangement + atom`, in the
// words JavaScript has: there the arrangement is "the left side" of an
// operator, here it is the arranging call, and the direction is the same in all
// four — write the atom into the plot it describes, before composing.
function atomOnPage(atom, word) {
  if (atom.kind === "data") {
    return `gog: \`data()\` belongs to a plot, and \`${word}()\` arranges plots. ` +
      "Write it into the plot it holds the table for, before composing: " +
      `\`${word}(plot(data(df), …), other_plot)\`.`;
  }
  // A position or a space is held as `coord_x` or `coord_polar`, and written `x`.
  // A mark with its transforms is named with its parts, `layer(bar, count)`, as
  // the other three bindings name it `bar * count`.
  const kind = atom.kind.replace(/^coord_/, "");
  const compound = atom.kind === "layer"
    ? `layer(${[atom.fields.mark, ...atom.fields.transforms].join(", ")})`
    : null;
  const shown = atom.kind === "mark" ? atom.fields.mark
    : atom.kind === "transform" ? atom.fields.transform
    : compound ?? `${kind}()`;
  const example = atom.kind === "mark" ? atom.fields.mark
    : compound ? compound
    : atom.kind === "transform" ? `layer(<mark>, ${atom.fields.transform})`
    : ["title", "subtitle", "caption", "x_label", "y_label", "z_label"].includes(atom.kind) ? `${atom.kind}("…")`
    : `${kind}(…)`;
  return `gog: \`${shown}\` belongs to a plot, and \`${word}()\` arranges plots. ` +
    "Write it into the plot it describes, before composing: " +
    `\`${word}(plot(data(df), …, ${example}), other_plot)\`.`;
}

function compose(arrange, figures, word) {
  // One group of cells per figure, flattened at the end. Keeping them apart is
  // what lets a rename reach exactly the cells that refer to the table.
  const groups = [];
  const frames = {};
  const anonymous = new Set();
  let theme = null;
  for (const figure of figures) {
    const atom = asAtom(figure);
    if (atom && atom.kind === "theme") {
      theme = { ...(theme ?? {}), ...pageTheme(atom, word) };
      continue;
    }
    if (!(figure instanceof Plot) && !(figure instanceof Arrangement)) {
      // The atom is named as it is written, and so is the example: every atom was
      // shown as `typeof`, so `color(col.a)` read `plot(data(df), object, …)`.
      throw new GogError(atom ? atomOnPage(atom, word)
        : `gog: \`${word}()\` arranges plots — got ${describe(figure)}.`);
    }
    // An arrangement already running this way is *flattened* into it, so
    // `beside(a, b, c)` is one row of three rather than a row of a row. An
    // arrangement running the other way stays a cell of its own, which is what
    // makes `below(top, beside(main, right))` two rows, the second holding two
    // plots.
    //
    // An arrangement that has stated its own size does not flatten either:
    // flattening keeps the cells and drops the node, and the node is where the
    // size was written.
    const mine =
      figure instanceof Arrangement && figure.spec.arrange === arrange && !figure.spec.theme
        ? [...figure.spec.cells]
        : [figure.spec];
    groups.push(mine);
    const here = groups.length - 1;

    for (const [name, table] of Object.entries(figure.frames)) {
      if (frames[name] !== undefined && frames[name] !== table) {
        const taken = new Set([...Object.keys(frames), ...Object.keys(figure.frames)]);
        // A name the author wrote is theirs and cannot be moved. A generated
        // one is the binding's own and means nothing to them, so it gives way —
        // which is what keeps an arrangement of two anonymous tables legal, the
        // way a plot of two already is.
        if (figure.anonymous.has(name)) {
          const fresh = nextFreeName(taken);
          groups[here] = renameTable(groups[here], name, fresh);
          frames[fresh] = table;
          anonymous.add(fresh);
          continue;
        }
        if (anonymous.has(name)) {
          const fresh = nextFreeName(taken);
          for (let k = 0; k < here; k += 1) groups[k] = renameTable(groups[k], name, fresh);
          frames[fresh] = frames[name];
          anonymous.delete(name);
          anonymous.add(fresh);
          frames[name] = table;
          continue;
        }
        throw new GogError(
          `gog: two different tables in one arrangement are both called \`${name}\` — a layer ` +
            `resolves its columns against the nearest table by name, so one of these ` +
            `can never be reached. Give them distinct names: ` +
            `\`data(df, { name: "…" })\`.`
        );
      }
      frames[name] = table;
      if (figure.anonymous.has(name)) anonymous.add(name);
    }
  }
  const cells = groups.flat();

  // Counted after the theme has been set aside, so the arity a reader is asked
  // for is the one they wrote: `beside(a, theme(…))` is one plot, not two.
  if (cells.length < 2) {
    throw new GogError(
      `gog: \`${word}()\` arranges two or more plots into one arrangement — ` +
        `\`${word}(plot(…), plot(…))\`.`
    );
  }
  const spec = { arrange, cells };
  if (theme && Object.keys(theme).length) spec.theme = theme;
  return new Arrangement(spec, frames, anonymous);
}

/**
 * Place plots side by side in one figure.
 *
 * `beside()` is how JavaScript writes `|` between two plots in R, Python and
 * Julia. Each plot keeps its own coordinate space. Plots side by side line up
 * their `y` axes, and when two of them put the same column on `y` with the
 * same scale settings, they share one axis: one scale, drawn once.
 *
 * It takes arrangements as well as plots, so one arrangement can hold another:
 * `below(top, beside(main, right))` is a histogram above a scatter plot, with
 * another histogram to its right.
 *
 * A `theme()` written among the plots is the whole figure's. It may set only
 * `width`, `height` and `gap`, the space between the plots; anything that
 * describes a panel belongs in the plot it describes. A plot's own
 * `theme({ width })` or `theme({ height })` sizes its cell, and the plots that
 * ask for no size share the rest.
 *
 * @param {...(Plot|Arrangement|Atom)} figures Two or more plots or
 *   arrangements, in order from left to right, and optionally a `theme()` for
 *   the whole figure.
 * @returns {Arrangement} The figure, to draw or save as a plot is.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 *   cylinders: ["6", "6", "4", "6", "8", "6", "8", "4"],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg));
 * const q = plot(data(cars), layer(bar, mean), x(col.cylinders), y(col.mpg));
 *
 * // Two plots side by side. Both put mpg on y, so they share that axis.
 * const side = beside(p, q);
 *
 * // The same, with the size of the whole figure.
 * const sized = beside(p, q, theme({ width: 700, height: 300 }));
 */
export function beside(...figures) {
  return compose("beside", figures, "beside");
}

/**
 * Place plots one above another in one figure.
 *
 * `below()` is how JavaScript writes `/` between two plots in R, Python and
 * Julia: `below(a, b)` draws `a` on top and `b` under it. Each plot keeps its
 * own coordinate space. Plots one above another line up their `x` axes, and
 * when two of them put the same column on `x` with the same scale settings,
 * they share one axis: one scale, drawn once.
 *
 * It takes arrangements as well as plots, so `below(top, beside(main, right))`
 * is the marginal plot: a histogram above a scatter plot, with another to its
 * right. A `theme()` among the plots sets the whole figure's `width`, `height`
 * and `gap`, and nothing else; a plot's own `theme({ height })` sizes its row.
 *
 * @param {...(Plot|Arrangement|Atom)} figures Two or more plots or
 *   arrangements, in order from top to bottom, and optionally a `theme()` for
 *   the whole figure.
 * @returns {Arrangement} The figure, to draw or save as a plot is.
 *
 * @example
 * const geyser = {
 *   eruptions: [3.6, 1.8, 3.333, 2.283, 4.533, 2.883,
 *               4.7, 3.6, 1.95, 4.35, 1.833, 3.917],
 *   waiting: [79, 54, 74, 62, 85, 55, 88, 85, 51, 85, 54, 84],
 * };
 *
 * // A short histogram above the scatter plot. Both put eruptions on x.
 * const top = plot(data(geyser), layer(bar, bin), x(col.eruptions),
 *   theme({ height: 150 }));
 * const main = plot(data(geyser), point, x(col.eruptions), y(col.waiting));
 * const stacked = below(top, main);
 *
 * // A histogram of waiting to the right of the scatter plot as well.
 * const right = plot(data(geyser), layer(bar, bin), y(col.waiting),
 *   theme({ width: 150 }));
 * const marginal = below(top, beside(main, right));
 */
export function below(...figures) {
  return compose("below", figures, "below");
}

// ---------------------------------------------------------------------------
// data() — the table, and its name
// ---------------------------------------------------------------------------

/**
 * Start a plot with a table.
 *
 * Every plot begins by naming its table, and then the channels name its
 * columns: `x(col.wt)`. A table is an object of columns,
 * `{ wt: [2.62, 2.875], mpg: [21, 21] }`, or an array of rows, one object per
 * row, `[{ wt: 2.62, mpg: 21 }, { wt: 2.875, mpg: 21 }]`. A column is an array,
 * or a typed array such as a `Float64Array`. A column of numbers is measured,
 * a column of text holds categories, and a column of `Date` values is drawn on
 * a calendar axis. A missing value is `null`, `undefined` or `NaN`. To give a
 * column's categories an order, use `ordered()`.
 *
 * JavaScript cannot read a table's name off its variable, so a table you do
 * not name gets a generated one: `data`, then `data2`, and so on. That is
 * enough to tell two tables apart. Name a table when you want a message about
 * it to use your name.
 *
 * A plot can read more than one table. A `data()` written after the first
 * gives its table to the mark written directly after it, and to no other.
 *
 * @param {Object<string, Array>|Array<object>} table An object of columns, or
 *   an array of rows.
 * @param {object|string} [options] Settings, by name, or the table's name
 *   alone: `data(cars, "cars")` is `data(cars, { name: "cars" })`.
 * @param {string} [options.name] What to call the table. Messages about the
 *   table use it, and two different tables in one plot need two names.
 * @returns {Atom} The table, to write first in `plot()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg));
 *
 * // The table named, so that a message about it says "cars".
 * const q = plot(data(cars, { name: "cars" }), point, x(col.wt), y(col.mpg));
 *
 * @example
 * // The same table as an array of rows, one object per row.
 * const rows = [
 *   { wt: 2.62, mpg: 21 }, { wt: 2.875, mpg: 21 }, { wt: 2.32, mpg: 22.8 },
 *   { wt: 3.215, mpg: 21.4 }, { wt: 3.44, mpg: 18.7 }, { wt: 3.46, mpg: 18.1 },
 *   { wt: 3.57, mpg: 14.3 }, { wt: 3.19, mpg: 24.4 },
 * ];
 * const p = plot(data(rows), point, x(col.wt), y(col.mpg));
 */
export function data(table, options = {}) {
  if (asAtom(table)) {
    throw new GogError(
      "gog: `data()` takes a table, not an atom — " +
        "`plot(data(df), point, x(col.a), y(col.b))`."
    );
  }
  if (table instanceof Column) {
    throw new GogError(
      "gog: `data()` takes the table itself, not a column — `data(df)`, " +
        "then the columns are named inside the plot: `x(col.gdp)`."
    );
  }
  // `gog_table()` is asynchronous in JavaScript alone, so the commonest way to
  // hand `data()` a Promise is to forget its `await`. A Promise is an object,
  // so it passed the check below and the plot was refused later, for the first
  // column it could not find, with advice to check a spelling that was right.
  if (typeof table?.then === "function") {
    throw new GogError(
      "gog: `data()` got a Promise rather than a table. `gog_table()` is " +
        "asynchronous in JavaScript, so await it first: " +
        '`const df = await gog_table("medals")`, then `data(df)`.'
    );
  }
  if (table === null || typeof table !== "object") {
    throw new GogError(
      "gog: `data()` takes a table — an object of columns, " +
        "`{ x: [1, 2], y: [3, 4] }`, or an array of rows, `[{ x: 1, y: 3 }, …]`. " +
        `Got ${describe(table)}.`
    );
  }
  // An array of rows is turned into columns here, once, so every check after this
  // one reads the same columns the wire will carry.
  if (Array.isArray(table)) {
    if (!table.length) {
      throw new GogError(
        "gog: `data()` got an array with no rows, so there are no columns to name."
      );
    }
    table = rowsToColumns(table);
  }

  const settings = typeof options === "string" ? { name: options } : options;
  const name = settings && settings.name;
  if (name !== undefined && typeof name !== "string") {
    throw new GogError(
      'gog: `data(df, { name: … })` takes a string — `data(df, { name: "notes" })`.'
    );
  }

  return new Atom("data", { table, name: name ?? null });
}

// A table that lives in a database.
//
// `query()` stands exactly where `data()` stands, and **nothing after it
// changes** — the same words, channels and transforms:
//
//     plot(data(orders),                          bar, count, x(col.status))
//     plot(query(con, "SELECT * FROM orders"),    bar, count, x(col.status))
//
// The SQL is confined to this one argument and never enters the grammar:
// `x(col.status)` is still a column resolved by the same mask.
//
// It returns the **same `data` atom** a table does, carrying a `Query` instead
// of an object of arrays, so `plot()` and the arrangement builder need no
// branch: a query is a table whose rows have not been fetched yet. They are
// fetched once, at render, which is what leaves room for the pushdown planner.
//
// JavaScript is the one binding with no database standard to lean on, so the
// connection is duck-typed and, because `render_svg()` is synchronous, an async
// driver is refused by name with its own direction (`render.js`).
/**
 * Start a plot with a table that lives in a database.
 *
 * Write `query()` where you would write `data()`, and nothing after it
 * changes: the same marks, the same channels, and columns named with `col`
 * as before. The SQL stays inside this one argument. It is not run when you write it; it runs
 * once, when the plot is drawn. The connection is your own, and gog opens
 * none.
 *
 * Node.js has no standard database interface, so gog accepts any connection
 * that has the right methods. It must be synchronous: anything with `.prepare(sql).all()`, such as
 * `DatabaseSync` from `node:sqlite` or a `better-sqlite3` database, or anything
 * with `.all(sql)`. A driver whose query returns a Promise, such as `pg` or
 * `mysql2`, is refused, because `render_svg()` is synchronous. With one of
 * those, await the rows yourself and pass them to `data(rows)`.
 *
 * @param {object} connection An open, synchronous database connection.
 * @param {string} sql One SELECT, as text.
 * @param {object|string} [options] Settings, by name, or the table's name
 *   alone.
 * @param {string} [options.name] What to call the table; `"query"` unless
 *   given. A second query in one plot needs a name of its own, because a layer
 *   finds its columns in the nearest table by name.
 * @returns {Atom} The table, to write first in `plot()`.
 *
 * @example
 * // `node:sqlite` is built into Node.js 22.13 and later.
 * const { DatabaseSync } = await import("node:sqlite");
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const con = new DatabaseSync(":memory:");
 * con.exec("CREATE TABLE cars (wt REAL, mpg REAL)");
 * const insert = con.prepare("INSERT INTO cars VALUES (?, ?)");
 * cars.wt.forEach((wt, i) => insert.run(wt, cars.mpg[i]));
 *
 * const p = plot(query(con, "SELECT wt, mpg FROM cars"),
 *   point, x(col.wt), y(col.mpg));
 * const svg = render_svg(p);
 * con.close();
 */
export function query(connection, sql, options = {}) {
  // `sql` is checked for `undefined` first so that `query("SELECT ...")` — the
  // mistake `data()` invites, that atom taking one argument — is told the fix
  // rather than failing later with an undefined query. The same guard is in the
  // other three bindings.
  if (sql === undefined) {
    if (typeof connection === "string") {
      throw new GogError(
        "gog: `query()` takes the connection first, then the SELECT — " +
          "`query(con, 'SELECT ...')`. A query on its own cannot say which " +
          "database it runs against, which is why the connection is written out " +
          "loud. If the rows are already in hand, that is `data(rows)`."
      );
    }
    throw new GogError(
      "gog: `query()` takes a connection and a SELECT — " +
        `\`query(con, 'SELECT ...')\`. Got ${describe(connection)} and no query.`
    );
  }
  if (typeof connection === "string") {
    throw new GogError(
      "gog: `query()` takes the connection first, then the SELECT — " +
        "`query(con, 'SELECT ...')`. A query on its own cannot say which " +
        "database it runs against, which is why the connection is written out " +
        "loud. If the rows are already in hand, that is `data(rows)`."
    );
  }
  if (typeof sql !== "string") {
    throw new GogError(
      "gog: `query()` takes a SELECT as text — `query(con, 'SELECT ...')`. " +
        `Got ${describe(sql)} for the query.`
    );
  }

  const settings = typeof options === "string" ? { name: options } : options;
  const name = settings && settings.name;
  if (name !== undefined && typeof name !== "string") {
    throw new GogError(
      'gog: `query(con, sql, { name: … })` takes a string — ' +
        '`query(con, sql, { name: "orders" })`.'
    );
  }

  return new Atom("data", { table: new Query(connection, sql), name: name ?? "query" });
}

// ---------------------------------------------------------------------------
// Plot — a finished sentence
// ---------------------------------------------------------------------------

/**
 * A finished sentence: a plot, ready to draw.
 *
 * `plot()` returns one. Draw it with `render_svg()`, save it with
 * `save_svg()`, `save_png()` or `save_json()`, put it in a page with
 * `html_block()`, or compose it with others through `beside()` and `below()`.
 * It holds what the engine reads, `spec`, and the tables it names, `frames`.
 * Printed, it gives one line naming its marks and its table, not the picture.
 * You do not make a `Plot` yourself. The class is exported so that code can
 * check for one with `instanceof`.
 *
 * @param {object} spec The specification, as the engine reads it.
 * @param {Object<string, object>} frames Each table the plot reads, by name.
 * @param {Set<string>} [anonymous] The table names gog made up, rather than
 *   names given with `data(table, { name })`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg));
 * console.log(p instanceof Plot); // true
 * console.log(String(p)); // <gog plot: point on data>
 */
export class Plot {
  constructor(spec, frames, anonymous = new Set()) {
    this.spec = spec;
    this.frames = frames;
    this.anonymous = anonymous;
  }

  toString() {
    const marks = this.spec.layers.map((l) => l.mark).join(" + ");
    return `<gog plot: ${marks || "no mark"} on ${this.spec.data}>`;
  }

  [Symbol.for("nodejs.util.inspect.custom")]() {
    return this.toString();
  }
}

function channelDef(atom) {
  // Every key is present and `null` where R and Python write `null`. JSON.stringify
  // *omits* an `undefined`, so a missing key here would reach the engine as a
  // different document than the other two bindings send — and parity compares the
  // picture, which would then differ for a reason no one could see.
  return {
    field: atom.fields.field,
    scale: atom.fields.scale ?? null,
    base: atom.fields.base ?? null,
    limits: atom.fields.limits ?? null,
    tick_count: atom.fields.tick_count ?? null,
    speed: atom.fields.speed ?? null,
    free: atom.fields.free ?? false,
    // Whether the channel's legend is drawn; `null` is the default, which draws it.
    legend: atom.fields.legend ?? null,
    // Whether a position's axis is drawn; `null` is the default, which draws it.
    axis: atom.fields.axis ?? null,
    // How many decimal places the channel's numbers are printed with.
    decimals: atom.fields.decimals ?? null,
    // Whether the channel's large numbers are written short (3.81B).
    short: atom.fields.short ?? null,
    // Whether the numbers are written as percents (25%).
    percent: atom.fields.percent ?? null,
  };
}

class Builder {
  constructor() {
    this.spec = {
      data: null,
      layers: [],
      coord: "flat",
      title: null,
      // `AxisSpec` is the axis's furniture, which is only its name: `tick_count`
      // moved to the channel binding 2026-07-26, beside `scale` and `limits`,
      // because how many ticks an axis gets is a property of the scale (§10).
      x_axis: { label: null },
      y_axis: { label: null },
      z_axis: { label: null },
      x: null,
      y: null,
      z: null,
      channels: {},
    };
    this.frames = {};
    this.names = new Map(); // table object → the name it was given
    this.currentLayer = null;
    this.pendingData = null;
    this.anonymous = 0;
    this.generated = new Set(); // the names this builder invented
  }

  // The table's name is the one thing JavaScript does worse than the other three
  // bindings, and it is answered by generating rather than by guessing. Law 4
  // resolves nearest-table-wins *by name*: R reads the name with `substitute()`,
  // Python off the caller's frame, and JavaScript can do neither. What the name
  // has to *do* is distinguish, and a counter distinguishes — so an unnamed table
  // gets a unique one, the same table passed twice keeps the one it has (R's
  // "a restatement, not a clash"), and `{ name: … }` is there when a diagnostic
  // should say `notes` instead of `data2`.
  nameFor(atom) {
    const { table, name } = atom.fields;

    if (name) {
      const existing = this.frames[name];
      if (existing !== undefined && existing !== table) {
        // The author wrote this name, and the builder invented the one already
        // here, so that is the one that moves, as it does on an arrangement.
        // Refused instead, an unnamed table followed by `data(df, { name:
        // "data" })` was two tables with one name, while the other order drew.
        if (!this.generated.has(name)) {
          throw new GogError(
            `gog: two different tables are both called \`${name}\` — a layer resolves ` +
              `its columns against the nearest table by name, so one of these can never ` +
              `be reached. Give them distinct names: \`data(df, { name: "…" })\`.`
          );
        }
        this.moveGenerated(name);
      }
      this.names.set(table, name);
      return name;
    }

    const already = this.names.get(table);
    if (already) return already;

    this.anonymous += 1;
    const generated = this.anonymous === 1 ? "data" : `data${this.anonymous}`;
    this.names.set(table, generated);
    this.generated.add(generated);
    return generated;
  }

  // Move a table the builder named, and every reference to it: the plot's own
  // table, each layer's, the open layer's and a pending one.
  moveGenerated(old) {
    let fresh;
    do {
      this.anonymous += 1;
      fresh = `data${this.anonymous}`;
    } while (this.frames[fresh] !== undefined);
    const table = this.frames[old];
    delete this.frames[old];
    this.frames[fresh] = table;
    this.names.set(table, fresh);
    this.generated.delete(old);
    this.generated.add(fresh);
    const swap = (n) => (n === old ? fresh : n);
    this.spec.data = swap(this.spec.data);
    for (const layer of this.spec.layers) layer.data = swap(layer.data);
    if (this.currentLayer) this.currentLayer.data = swap(this.currentLayer.data);
    this.pendingData = swap(this.pendingData);
  }

  add(item, position) {
    const atom = asAtom(item);

    if (!atom) {
      // A whole plot handed in as an argument, which is how the grouping the
      // other three bindings spell with parentheses would be written here. It
      // cannot work and must not look as though it did: a nested plot carries
      // marks, positions and a title that this list has nowhere to put, and
      // dropping them would be the silent loss §12 forbids. The direction is the
      // one every binding gives — repeat `data()` per mark. Checked before the
      // bare-table branch below, which would otherwise claim a `Plot` is a table.
      if (item instanceof Plot || item instanceof Arrangement) {
        throw new GogError(
          "gog: a plot cannot be an argument to another plot, so everything inside " +
            "it would be dropped. Write the atoms in sequence instead, and repeat " +
            "`data()` before each mark that reads that table: " +
            "`plot(data(a), line, data(b), point, data(b), area)`. " +
            "To arrange two plots, use `beside()` or `below()`."
        );
      }
      // A table handed over bare, which is the sentence the manual documents
      // being refused: `gapminder_2007 + point + …`. The direction is `data()`,
      // not "that is not an atom" — the reader has the right table and the wrong
      // wrapper, and the wrapper is what gives the table a name for Law 4 to
      // resolve against.
      if (item !== null && typeof item === "object" && !Array.isArray(item)) {
        throw new GogError(
          "gog: a plot starts with `data()`, which names the table — a channel names " +
            "one of its columns, and the nearest named table wins, so the name matters. " +
            "Write `plot(data(df), point, x(col.a), y(col.b))`."
        );
      }
      if (typeof item === "function") {
        throw new GogError(
          "gog: an atom that takes an argument was written bare, which adds the " +
            "function itself. Write it with its argument, e.g. `x(col.gdp)`."
        );
      }
      throw new GogError(
        "gog: `plot()` takes gog atoms — a mark, a channel, a setting. " +
          `Got ${describe(item)} in position ${position + 1}. A table joins through ` +
          "`data()`: `plot(data(df), point, x(col.a), y(col.b))`."
      );
    }

    if (position === 0 && atom.kind !== "data") {
      throw new GogError(
        "gog: a plot starts with its table, which is what makes a column name mean " +
          "something: `plot(data(df), point, x(col.a), y(col.b))`."
      );
    }

    switch (atom.kind) {
      case "data": {
        const name = this.nameFor(atom);
        this.frames[name] = atom.fields.table;
        if (position === 0) this.spec.data = name;
        else this.pendingData = name;
        return;
      }

      case "mark": {
        const layer = {
          mark: atom.fields.mark,
          encodings: {},
          transforms: [],
          data: this.pendingData,
        };
        if (atom.fields.box !== undefined) layer.box = clone(atom.fields.box);
        this.openLayer(layer);
        return;
      }

      case "layer": {
        const layer = {
          mark: atom.fields.mark,
          encodings: clone(atom.fields.encodings) ?? {},
          transforms: [...atom.fields.transforms],
          data: this.pendingData,
        };
        for (const param of ["bin", "density", "range", "confidence", "smooth_band", "straight_band", "deviation", "quantile", "jitter", "stack",
          "cumulative", "bounds", "partition", "flow", "layout", "cluster", "box"]) {
          if (atom.fields[param] !== undefined) layer[param] = clone(atom.fields[param]);
        }
        this.openLayer(layer);
        return;
      }

      case "coord_x":
      case "coord_y":
      case "coord_z":
        this.setPosition(atom.kind.slice(-1), atom);
        return;

      case "coord_space":
        this.spaceFree("space");
        this.spec.coord = {
          space: { turn: atom.fields.turn, tilt: atom.fields.tilt },
        };
        return;

      case "coord_polar":
        this.spaceFree("polar");
        this.spec.coord = { polar: { start: atom.fields.start } };
        return;

      // Nest carries no view parameter, so it crosses as the bare string
      // "nest" — the one unit variant left in `CoordSpace`.
      case "coord_nest":
        this.spaceFree("nest");
        this.spec.coord = "nest";
        return;

      // A globe carries the place its view faces, in space's own two words:
      // {"globe":{"turn":0,"tilt":0}} matches `CoordSpace::Globe(GlobeView)`,
      // and a bare "globe" is not a legal form.
      case "coord_network": {
        this.spaceFree("network");
        const view = {};
        if (atom.fields.turn !== undefined) view.turn = atom.fields.turn;
        if (atom.fields.tilt !== undefined) view.tilt = atom.fields.tilt;
        this.spec.coord = { network: view };
        return;
      }
      case "coord_globe":
        this.spaceFree("globe");
        this.spec.coord = {
          globe: { turn: atom.fields.turn, tilt: atom.fields.tilt },
        };
        return;

      // A map carries what the flattening must preserve, the same way space and
      // polar carry theirs: {"map":{"preserve":"area"}} matches
      // `CoordSpace::Map(MapView)`, and a bare "map" is not a legal form.
      case "coord_map":
        this.spaceFree("map");
        this.spec.coord = { map: { preserve: atom.fields.preserve } };
        return;

      case "color":
      case "group":
      case "size":
      case "shape":
      case "opacity":
      case "label":
      case "pattern":
      case "play":
        this.setChannel(atom.kind, atom);
        return;

      // Plot-scoped, like `palette`: a predicate over rows is a fact about the
      // data, so every layer reading that column answers to it.
      case "brush": {
        const entry = { field: atom.fields.field };
        if (atom.fields.at !== undefined) entry.at = atom.fields.at;
        if (atom.fields.levels !== undefined) entry.levels = atom.fields.levels;
        (this.spec.brush ??= []).push(entry);
        return;
      }

      case "style":
        this.setStyle(atom.fields.props);
        return;

      case "palette":
        this.spec.palette = atom.fields.value;
        return;

      case "theme": {
        // Merged rather than replaced, so two `theme()` calls accumulate the way
        // two `style()` calls on one mark do. Only the properties actually named
        // are written, keeping "said nothing" apart from "asked for the default"
        // (spec §7).
        if (!this.spec.theme) this.spec.theme = {};
        for (const key of ["preset", "grid", "ratio", "tick_angle", "font_size", "background", "strip", "strip_text", "axis_label",
                           "legend", "frame", "width", "height", "gap"]) {
          if (atom.fields[key] !== null && atom.fields[key] !== undefined) {
            this.spec.theme[key] = atom.fields[key];
          }
        }
        return;
      }

      case "title":
        this.spec.title = atom.fields.value;
        return;

      case "subtitle":
        this.spec.subtitle = atom.fields.value;
        return;

      case "caption":
        this.spec.caption = atom.fields.value;
        return;

      case "x_label":
      case "y_label":
      case "z_label":
        this.spec[`${atom.kind[0]}_axis`].label = atom.fields.value;
        return;

      case "order":
        this.spec.order = {
          field: atom.fields.field,
          descending: atom.fields.descending,
        };
        return;

      case "facet_col":
      case "facet_row": {
        if (!this.spec.facet) this.spec.facet = { col: null, row: null };
        const slot = atom.kind === "facet_col" ? "col" : "row";
        // A plot splits once across and once down, so a second split in one
        // direction would replace the first. It did, in silence:
        // `across(col.continent), across(col.era)` kept only `era`.
        const taken = this.spec.facet[slot];
        if (taken !== null && taken !== undefined) {
          const [way, word] = slot === "col" ? ["panel columns", "across"] : ["panel rows", "down"];
          throw new GogError(
            `gog: this plot is already split into ${way} by \`${taken}\`, and a second ` +
              `\`${word}()\` would replace it. A plot splits once across and once down: ` +
              `cross two columns with \`across(col.a), down(col.b)\`, or split by one.`
          );
        }
        this.spec.facet[slot] = atom.fields.field;
        // The count rides with the column it was written on; which way the line
        // runs is the word's, already settled. Carried even onto a crossing,
        // where the engine refuses it with the reason — dropping a binding in
        // silence is what spec §12 forbids.
        if (atom.fields.wrap !== null && atom.fields.wrap !== undefined) {
          this.spec.facet.wrap = atom.fields.wrap;
        }
        return;
      }

      default:
        throw new GogError(`gog: unknown atom \`${atom.kind}\`.`);
    }
  }

  // A plot is drawn in one coordinate space. Each space word wrote its own and
  // replaced whatever was there, so the engine only ever saw the last one:
  // `space(), polar()` drew polar and dropped the cube, in silence. Restating the
  // same space replaces its view, which is how an angle is changed.
  spaceFree(kind) {
    const coord = this.spec.coord;
    const was = typeof coord === "string" ? coord : coord ? Object.keys(coord)[0] : "flat";
    if (was === "flat" || was === kind) return;
    throw new GogError(
      `gog: this plot is already drawn in \`${was}()\`, and \`${kind}()\` asks for a ` +
        `second space. A plot is drawn in one coordinate space: keep one, or draw the ` +
        `plot twice side by side, \`beside(plot(..., ${was}()), plot(..., ${kind}()))\`.`
    );
  }

  openLayer(layer) {
    if (this.currentLayer !== null) this.spec.layers.push(this.currentLayer);
    this.currentLayer = layer;
    this.pendingData = null;
  }

  // A position is scoped by position, like every other channel. Written before
  // any mark it is the plot's; written after one it is that layer's, which is
  // what lets a second table say where its own rows go. One axis with two column
  // names, never two axes — the scale, the ticks and the space stay the plot's.
  setPosition(channel, atom) {
    const definition = channelDef(atom);
    if (this.currentLayer === null) this.spec[channel] = definition;
    else this.currentLayer.encodings[channel] = definition;
  }

  setChannel(channel, atom) {
    const definition = channelDef(atom);
    if (this.currentLayer === null) this.spec.channels[channel] = definition;
    else this.currentLayer.encodings[channel] = definition;
  }

  setStyle(props) {
    if (this.currentLayer === null) {
      throw new GogError(
        "gog: `style()` has no mark to style. Put it after a mark, e.g. " +
          '`plot(data(df), point, style({ color: "tomato" }))`.'
      );
    }
    if (!this.currentLayer.style) this.currentLayer.style = {};
    Object.assign(this.currentLayer.style, props);
  }

  // A table still waiting for its mark is refused rather than dropped: a later
  // `data()` gives its table to the mark written after it, and at the end of the
  // plot there is none, so the plot drew as though the table had never been
  // written, and said nothing.
  finish() {
    if (this.pendingData !== null) {
      // Named as the author wrote it, or not at all: a name this builder invented
      // for an unnamed table is one they never wrote and would not recognize.
      const name = this.generated.has(this.pendingData) ? null : this.pendingData;
      const said = name === null ? "the last `data()`" : `\`data(${name})\``;
      throw new GogError(
        `gog: ${said} ends the plot, so no mark comes after it to read its table. ` +
          `A later \`data()\` gives its table to the mark written directly after it: ` +
          `write that mark next, as in \`plot(..., data(${name ?? "..."}), point)\`, or ` +
          `remove ${name === null ? "that `data()`" : said}.`
      );
    }
    if (this.currentLayer !== null) {
      this.spec.layers.push(this.currentLayer);
      this.currentLayer = null;
    }
    return new Plot(this.spec, this.frames, this.generated);
  }
}

/**
 * Build a plot from its words: a table, a mark, and the channels that place it.
 *
 * `plot()` is how JavaScript writes the `+` of R, Python and Julia: there the
 * words of a sentence are joined by `+`, and here they are the arguments,
 * joined by commas. The first word is the table, from `data()` or `query()`.
 * Then come marks and layers, channels, settings, and the words about the
 * whole plot, such as `title()` and `theme()`.
 *
 * Where a word is written decides what it belongs to. A channel written before
 * any mark belongs to the whole plot; one written after a mark belongs to that
 * mark alone. A position written after a mark still reaches every layer that
 * names no column of its own for that axis. A `style()` sets the mark written
 * before it. A second `data()`
 * gives its table to the mark written directly after it.
 *
 * A word gog has no meaning for is refused with a `GogError` that says what to
 * write instead. Nothing you write is accepted and then ignored.
 *
 * @param {...(Atom|Function)} items The words of the sentence, starting with
 *   `data()`. A bare mark or transform, such as `point`, is written without
 *   parentheses.
 * @returns {Plot} The finished sentence. Draw it with `render_svg()`, or save
 *   it with `save_svg()` or `save_png()`.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 *   cylinders: ["6", "6", "4", "6", "8", "6", "8", "4"],
 * };
 *
 * // Read aloud: given cars, points, x is wt, y is mpg.
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg));
 *
 * // The positions are the plot's, so both marks use them. The color is
 * // written after `point`, so it colors the points and not the line.
 * const q = plot(data(cars), x(col.wt), y(col.mpg),
 *   line, point, color(col.cylinders));
 */
export function plot(...items) {
  if (!items.length) {
    throw new GogError(
      "gog: `plot()` has nothing to draw. A sentence starts with its table and " +
        "names a mark: `plot(data(df), point, x(col.a), y(col.b))`."
    );
  }
  const builder = new Builder();
  items.forEach((item, position) => builder.add(item, position));
  return builder.finish();
}
