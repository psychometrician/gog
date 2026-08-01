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
import { Query } from "./render.js";

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

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
// `stack`'s share flag and baseline,
// and `bounds`' column names ride the *layer* on the wire (`layer.bin`, …), not
// the transform list — the transform list is names only. Absent parameters
// attach nothing, so a bare `layer(bar, bin)` stays on Sturges' rule.
const CARRIED = new Set(["bin", "density", "confidence", "jitter", "stack", "bounds",
  "partition"]);

function carry(layer, transform) {
  const name = transform.fields.transform;
  if (!CARRIED.has(name)) return;
  const params = {};
  for (const [key, value] of Object.entries(transform.fields)) {
    if (key !== "transform" && value !== null && value !== undefined) params[key] = value;
  }
  if (Object.keys(params).length) layer.fields[name] = params;
}

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

export function across(field, options) {
  return facetAtom("facet_col", field, options, "across");
}

export function down(field, options) {
  return facetAtom("facet_row", field, options, "down");
}

// `facet` exists in JavaScript only to say where it went. A reader arriving from
// the manual will type it, and §12 says a refusal names the fix.
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
// is several plots on one page, each keeping its own coordinate space (spec
// §11). What relates the composed plots is one rule, and the engine owns it: the
// same column on the same axis in two of them is one axis — one scale, one panel
// extent, drawn once (`render::page`).
// ---------------------------------------------------------------------------

// A page of plots. It carries `spec` and `frames` exactly as a `Plot` does,
// because every host — `render_svg`, `save`, `show` — asks a figure for those
// two and nothing else.
export class Page {
  constructor(spec, frames) {
    this.spec = spec;
    this.frames = frames;
  }

  toString() {
    return `<gog page: ${this.spec.cells.length} cells, ${this.spec.arrange}>`;
  }

  [Symbol.for("nodejs.util.inspect.custom")]() {
    return this.toString();
  }
}

function compose(arrange, figures, word) {
  if (figures.length < 2) {
    throw new GogError(
      `gog: \`${word}()\` arranges two or more plots on one page — ` +
        `\`${word}(plot(…), plot(…))\`.`
    );
  }

  const cells = [];
  const frames = {};
  for (const figure of figures) {
    if (!(figure instanceof Plot) && !(figure instanceof Page)) {
      const hint = asAtom(figure)
        ? `an atom joins a plot rather than a page: \`plot(data(df), ${describe(figure)}, …)\``
        : `got ${describe(figure)}`;
      throw new GogError(`gog: \`${word}()\` arranges plots — ${hint}.`);
    }
    // A page already running this way is *flattened* into it, so
    // `beside(a, b, c)` is one row of three rather than a row of a row. A page
    // running the other way stays a cell of its own, which is what makes
    // `below(top, beside(main, right))` two rows, the second holding two plots.
    if (figure instanceof Page && figure.spec.arrange === arrange) {
      cells.push(...figure.spec.cells);
    } else {
      cells.push(figure.spec);
    }
    for (const [name, table] of Object.entries(figure.frames)) {
      if (frames[name] !== undefined && frames[name] !== table) {
        throw new GogError(
          `gog: two different tables on one page are both called \`${name}\` — a layer ` +
            `resolves its columns against the nearest table by name, so one of these ` +
            `can never be reached. Give them distinct names: ` +
            `\`data(df, { name: "…" })\`.`
        );
      }
      frames[name] = table;
    }
  }
  return new Page({ arrange, cells }, frames);
}

export function beside(...figures) {
  return compose("beside", figures, "beside");
}

export function below(...figures) {
  return compose("below", figures, "below");
}

// ---------------------------------------------------------------------------
// data() — the table, and its name
// ---------------------------------------------------------------------------

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
  if (table === null || typeof table !== "object") {
    throw new GogError(
      "gog: `data()` takes a table — an object of columns, " +
        `\`{ x: [1, 2], y: [3, 4] }\`. Got ${describe(table)}.`
    );
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
// of an object of arrays, so `plot()` and the page builder need no branch: a
// query is a table whose rows have not been fetched yet. They are fetched once,
// at render, which is what leaves room for the pushdown planner.
//
// JavaScript is the one binding with no database standard to lean on, so the
// connection is duck-typed and, because `render_svg()` is synchronous, an async
// driver is refused by name with its own direction (`render.js`).
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
        "`query(con, 'SELECT ...')`."
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

export class Plot {
  constructor(spec, frames) {
    this.spec = spec;
    this.frames = frames;
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
        throw new GogError(
          `gog: two different tables are both called \`${name}\` — a layer resolves ` +
            `its columns against the nearest table by name, so one of these can never ` +
            `be reached. Give them distinct names: \`data(df, { name: "…" })\`.`
        );
      }
      this.names.set(table, name);
      return name;
    }

    const already = this.names.get(table);
    if (already) return already;

    this.anonymous += 1;
    const generated = this.anonymous === 1 ? "data" : `data${this.anonymous}`;
    this.names.set(table, generated);
    return generated;
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
      if (item instanceof Plot || item instanceof Page) {
        throw new GogError(
          "gog: a plot cannot be an argument to another plot, so everything inside " +
            "it would be dropped. Write the atoms in sequence instead, and repeat " +
            "`data()` before each mark that reads that table: " +
            "`plot(data(a), line, data(b), point, data(b), area)`. " +
            "To put two plots on a page, use `beside()` or `below()`."
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
        for (const param of ["bin", "density", "confidence", "jitter", "stack", "bounds",
          "partition", "box"]) {
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
        this.spec.coord = {
          space: { turn: atom.fields.turn, tilt: atom.fields.tilt },
        };
        return;

      case "coord_polar":
        this.spec.coord = { polar: { start: atom.fields.start } };
        return;

      // Nest carries no view parameter, so it crosses as the bare string
      // "nest" — `CoordSpace::Nest` is a unit variant, like globe.
      case "coord_nest":
        this.spec.coord = "nest";
        return;

      // A map carries what the flattening must preserve, the same way space and
      // polar carry theirs: {"map":{"preserve":"area"}} matches
      // `CoordSpace::Map(MapView)`, and a bare "map" is not a legal form.
      case "coord_map":
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
        for (const key of ["preset", "grid", "ratio", "tick_angle", "font_size", "background", "strip", "strip_text",
                           "frame", "width", "height"]) {
          if (atom.fields[key] !== null && atom.fields[key] !== undefined) {
            this.spec.theme[key] = atom.fields[key];
          }
        }
        return;
      }

      case "title":
        this.spec.title = atom.fields.value;
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

  finish() {
    if (this.currentLayer !== null) {
      this.spec.layers.push(this.currentLayer);
      this.currentLayer = null;
    }
    return new Plot(this.spec, this.frames);
  }
}

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
