// columns.js — how a JavaScript expression names a column
//
// The mirror of `py-pkg/gog/gog/columns.py`, and deliberately not a variation
// on it. Spec §8 ("the cross-language wrinkles"): every binding hands the
// engine a column *name*, and each language captures a bare name its own way.
//
//     R       data(gapminder) + point + x(gdp)      + y(life)
//     Python  data(gapminder) + point + x(col.gdp)  + y(col.life)
//     Julia   data(gapminder) + point + x(:gdp)     + y(:life)
//     JS      plot(data(gapminder), point, x(col.gdp), y(col.life))
//
// JavaScript's capture answer *is* Python's, decided 2026-07-25: the same `col`
// accessor, mandatory for the same reason, a `Proxy` where Python's is a
// `__getattr__`. That is Law 2 rather than a coincidence — JavaScript has no
// bare names and neither does Python, and in this grammar a string is how a
// *value* is spelled (`style({ color: "tomato" })`, `title("…")`,
// `palette("okabe")`). Spec §18 refuses `color("red")` as a channel argument
// precisely because **a channel takes a column, never a value**; R and Julia get
// that refusal free from their syntax. Restoring the distinction here is what
// lets the refusal be given with direction — "map with `color(col.species)`, set
// with `style({ color: 'red' })`" — instead of the engine reporting a missing
// column called `red`, which blames the reader for what the binding lost.
//
// So the rule this module enforces is one line: **`col.name` is a column,
// everything else is a value.** It is the same rule in every atom, which is what
// Law 1 asks of it.

import { GogError } from "./errors.js";

const IDENTIFIER = /^[A-Za-z_$][A-Za-z0-9_$]*$/;

/**
 * A column's name, as `col` writes it.
 *
 * `col.wt` is a `Column` whose `name` is `"wt"`. A channel or a transform takes
 * one wherever a column belongs. Printed, it reads the way you typed it:
 * `col.wt`. You do not make a `Column` yourself; `col` makes them. The class is
 * exported so that code can check for one with `instanceof`.
 *
 * A column cannot be used in arithmetic: gog has no computed channels, so
 * `col.wt / 2` is refused. Compute the new column in the table first, then name
 * it.
 *
 * @param {string} name The column's name in the table.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const weight = col.wt;
 * console.log(weight instanceof Column); // true
 * console.log(weight.name); // "wt"
 *
 * const p = plot(data(cars), point, x(weight), y(col.mpg));
 */
export class Column {
  constructor(name) {
    this.name = name;
    Object.freeze(this);
  }

  toString() {
    return IDENTIFIER.test(this.name)
      ? `col.${this.name}`
      : `col[${JSON.stringify(this.name)}]`;
  }

  // What `${col.gdp}` and an error message both print. Node's inspector asks
  // for this symbol, so a Column in a thrown message reads as the thing the
  // caller typed rather than as `Column { name: 'gdp' }`.
  [Symbol.for("nodejs.util.inspect.custom")]() {
    return this.toString();
  }

  // Arithmetic on a column would be a computed channel, which gog does not
  // have. `col.gdp / 1000` asks for a number, and got `NaN`, which reached the
  // atom as "Got number". A string is still given for the text a message
  // builds, since `+` asks the same way for a sum and for a joined string.
  [Symbol.toPrimitive](hint) {
    if (hint === "number") {
      throw new GogError(
        "gog: a channel takes a column name, not an expression — gog has no " +
          "computed channels. Compute the column in JavaScript first and bind the " +
          "result: `df.ratio = df.a.map((a, i) => a / df.b[i])`, then `col.ratio` " +
          "where the expression was."
      );
    }
    return this.toString();
  }
}

// `col` — the bare-name capture layer. `col.gdp`, or `col["life exp"]`.
//
// The target is a function so the proxy can intercept a call and refuse it with
// direction; a plain object target would give JavaScript's "col is not a
// function", which names neither the cause nor the fix.
/**
 * Name a column: `col.wt` is the column called `wt`.
 *
 * A channel takes a column, and in this grammar a plain string is a value, as
 * in `style({ color: "tomato" })` or `title("Fuel use")`. So a column is
 * written through `col`. A string where a column belongs is refused, and the
 * message names the `col` spelling to write instead.
 *
 * `col["wt"]` names the same column. Use that bracket form for a name that
 * is not a JavaScript identifier, such as one with a space or a dot in it:
 * `col["life exp"]`. A name in another writing system needs no brackets.
 *
 * `col` holds no values and is not a function, so `col()` and `col.wt = …`
 * are both refused.
 *
 * @type {Readonly<Object<string, Column>>}
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg));
 *
 * // The bracket form names the same column.
 * const q = plot(data(cars), point, x(col["wt"]), y(col["mpg"]));
 */
export const col = new Proxy(function col() {}, {
  get(_target, key) {
    // Symbols are the machinery's, never a column's: `util.inspect`,
    // `Symbol.toPrimitive`, iteration protocols. Handing each of them a Column
    // would misbehave far from here, which is the same trap Python's dunder
    // guard exists for.
    if (typeof key === "symbol") {
      return key === Symbol.for("nodejs.util.inspect.custom")
        ? () => "col"
        : undefined;
    }
    return new Column(key);
  },

  apply() {
    throw new GogError(
      "gog: `col` is not a function — a column is `col.gdp`, or " +
        '`col["life exp"]` when the name is not a JavaScript identifier.'
    );
  },

  // Without this, `col.gdp = …` would silently succeed against the function
  // target and the next read would return the assigned value rather than a
  // Column. Nothing about `col` is settable; say so.
  set(_target, key) {
    throw new GogError(
      `gog: \`col\` names columns, it does not hold them — \`col.${String(key)} = …\` ` +
        "sets nothing. Put the values in the table, then name the column: " +
        "`x(col.<name>)`."
    );
  },
});

// Channels that also exist as a `style()` setting. Spec §7 is the distinction
// these two spellings sit either side of: a channel *maps* a column and earns a
// legend; a setting *fixes* one value and earns none. It is exactly the mistake
// a string in a channel is usually reaching for, so the refusal names both.
const SETTABLE = new Set(["color", "size", "opacity", "shape", "pattern"]);

// Take the column name out of `col.x`, refusing a value with direction.
export function columnName(value, atom) {
  if (value instanceof Column) return value.name;

  // No column at all: `y({ limits: [0, 60] })`. The old refusal said only "Got
  // undefined", and R's missing argument reached the engine as an empty name,
  // which drew under `bar * count`. All four now refuse it with one direction: a
  // setting describes the axis of the column it sits beside, so the column is
  // named first.
  if (value === undefined || value === null) {
    if (atom === "x" || atom === "y" || atom === "z") {
      throw new GogError(
        `gog: \`${atom}()\` names no column. A position maps a column, and its ` +
          `settings describe that column's axis, so name the column first: ` +
          `\`${atom}(col.life, { limits: [0, 90] })\`. Where a transform makes the ` +
          `measure, the name is the transform's own, such as \`count\` or ` +
          `\`density\`: \`${atom}(col.count, { limits: [0, 60] })\`. With nothing to ` +
          `set, leave \`${atom}()\` out.`
      );
    }
    throw new GogError(
      `gog: \`${atom}()\` names no column. It takes a column of the table: ` +
        `\`${atom}(col.<column>)\`.`
    );
  }

  if (value === col) {
    throw new GogError(
      `gog: \`${atom}()\` needs a column — \`col\` on its own is the accessor, ` +
        `not a column. Write \`${atom}(col.<name>)\`.`
    );
  }

  if (typeof value === "string") {
    const direction = IDENTIFIER.test(value)
      ? `\`${atom}(col.${value})\` maps the column called \`${value}\``
      : `\`${atom}(col[${JSON.stringify(value)}])\` maps the column of that name`;
    const setting = SETTABLE.has(atom)
      ? `\n  To fix one value for the whole layer instead — no legend, nothing ` +
        `to decode — that is a setting: \`style({ ${atom}: ${JSON.stringify(value)} })\`.`
      : "";
    throw new GogError(
      `gog: \`${atom}(${JSON.stringify(value)})\` binds a *value*, and a channel takes a ` +
        `*column*. In JavaScript, a bare name is a variable, so a column is ` +
        `written with the accessor: ${direction}.${setting}`
    );
  }

  // An array — the *values* rather than the name. This is spec §18's refused
  // sentence arriving in JavaScript dress: a plot is a mapping from a table
  // (Law 4 — the table is the context that makes a bare name mean something),
  // so a channel takes a column and never values, and vector-direct plotting is
  // a decided refusal. Give it the direction §18 records: put the values in a
  // table first.
  if (Array.isArray(value)) {
    throw new GogError(
      `gog: \`${atom}()\` takes a column *name*, and this is a column's *values*. ` +
        `gog plots a table: a channel names one of its columns, so that a legend, ` +
        `an axis and a second layer all know what they are talking about. Put the ` +
        `values in a table first — \`plot(data({ value: values }), point, ` +
        `${atom}(col.value))\` — or, if the table already exists, name the column: ` +
        `\`${atom}(col.<name>)\`.`
    );
  }

  throw new GogError(
    `gog: \`${atom}()\` takes a column — \`${atom}(col.<name>)\`. ` +
      `Got ${describe(value)}.`
  );
}

export function describe(value) {
  if (value === null) return "null";
  if (Array.isArray(value)) return "an array";
  if (value instanceof Column) return String(value);
  return typeof value;
}
