// render.js — the bridge: a table to the wire, a spec to the CLI, an SVG back
//
// The mirror of `r-pkg/gog/R/render.R` and `py-pkg/gog/gog/render.py`, and
// deliberately the same shape: find the binary, turn each table into the
// column-oriented wire form, hand `{spec, data}` to `gog-cli` on stdin, read the
// SVG off stdout and the diagnostics off stderr. No policy lives here. Which
// plots are legal, what a missing value does to a row, what `GOG_STRICT` means —
// all of that is `gog-core`'s, because a rule implemented in a binding is a rule
// the other bindings will get wrong (spec §14).
//
// This is the thinnest of the three bridges, and the reason is the architecture
// rather than the effort: the IR *is* JavaScript's native tongue, so where R has
// `df_to_wire` and Python has `to_wire` marshaling a foreign object, here a
// table is already an object of arrays and the spec is already JSON.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { describe } from "./columns.js";
import { GogError } from "./errors.js";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const EXE = process.platform === "win32" ? "gog-cli.exe" : "gog-cli";
const REQUIRE = createRequire(import.meta.url);

// ---------------------------------------------------------------------------
// Find the gog-cli binary
// ---------------------------------------------------------------------------

// The platforms the engine is built for, as `${process.platform}-${process.arch}`.
//
// This list is the authority, and three things have to agree with it: the
// release workflow builds one package per entry, `package.json` pins the same
// five under `optionalDependencies`, and a test asserts all three match. The
// reason that test exists is the failure mode below — a platform added in one
// place and forgotten in another is not a build error anywhere, it is a machine
// that installs cleanly and then cannot draw.
export const ENGINE_PLATFORMS = [
  "darwin-arm64",
  "darwin-x64",
  "linux-arm64",
  "linux-x64",
  "win32-x64",
];

// The package carrying the engine for the machine this is running on, or `null`
// where the engine is not built for it.
//
// The engine ships as one small package per platform rather than five binaries
// in this one, listed under `optionalDependencies` and filtered by npm's
// `os`/`cpu` fields, so an install downloads a single 2 MB engine. It is the
// same decision the Python wheels make — one artifact per platform, each
// carrying its own engine — written the way npm already understands, and it
// needs no install script, so `--ignore-scripts` is not a way to end up without
// an engine.
export function platform_package() {
  const target = `${process.platform}-${process.arch}`;
  return ENGINE_PLATFORMS.includes(target) ? `grammar-of-graphics-${target}` : null;
}

// Make `binary` runnable, or report that it cannot be. npm preserves the
// executable bit in a tarball, so this is a repair for the cases that do not go
// through npm at all — a hand-copied file, a zip round trip.
function runnable(binary) {
  if (!fs.existsSync(binary)) return null;
  if (process.platform === "win32") return binary;
  try {
    fs.accessSync(binary, fs.constants.X_OK);
  } catch {
    try {
      fs.chmodSync(binary, fs.statSync(binary).mode | 0o111);
    } catch {
      return null;
    }
  }
  return binary;
}

function platformCli() {
  const name = platform_package();
  if (!name) return null;
  try {
    return runnable(REQUIRE.resolve(`${name}/bin/${EXE}`));
  } catch {
    // **npm omits an optional dependency it cannot install without failing the
    // install**, so this miss is the normal shape of three different situations:
    // no engine published for this platform, an install run with
    // `--no-optional`, and a development checkout that never installed at all.
    // None of them is an error *here*; the caller decides, and names the missing
    // package so the diagnosis is not left to the user (§12).
    return null;
  }
}

function bundledCli() {
  // The engine sitting inside this package directory. Nothing published puts it
  // here — a release ships it in the platform package above — so this is the
  // route for a hand-packed tarball and for the release job's own check, which
  // stages the binary before the platform packages exist.
  return runnable(path.join(HERE, "..", "bin", EXE));
}

function onPath() {
  const dirs = (process.env.PATH || "").split(path.delimiter).filter(Boolean);
  for (const dir of dirs) {
    const candidate = path.join(dir, EXE);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      if (fs.statSync(candidate).isFile()) return candidate;
    } catch {
      // not here; keep looking
    }
  }
  return null;
}

// Locate the engine: an override, the shipped one, PATH, then a local build.
// The same order as R's and Python's, for the same reason at step two — the
// binary that shipped with a package is the one whose wire format matches it, so
// an unrelated `gog-cli` earlier on PATH must not silently answer for it.
export function find_gog_cli() {
  const override = process.env.GOG_CLI_PATH;
  if (override && fs.existsSync(override)) return override;

  const shipped = platformCli() ?? bundledCli();
  if (shipped) return shipped;

  const found = onPath();
  if (found) return found;

  // Walk up from this file as well as from the working directory, so a plot
  // drawn from anywhere inside the repo finds the build.
  const roots = [process.cwd()];
  let here = HERE;
  for (let i = 0; i < 6; i += 1) {
    roots.push(here);
    const parent = path.dirname(here);
    if (parent === here) break;
    here = parent;
  }
  let cwd = process.cwd();
  for (let i = 0; i < 6; i += 1) {
    const parent = path.dirname(cwd);
    if (parent === cwd) break;
    cwd = parent;
    roots.push(cwd);
  }
  for (const root of roots) {
    for (const build of ["release", "debug"]) {
      const candidate = path.join(root, "target", build, EXE);
      if (fs.existsSync(candidate)) return candidate;
    }
  }

  // Which advice is right depends on whether an engine exists for this machine
  // at all, so the two cases are separated rather than merged into one message
  // that half of its readers cannot act on.
  const name = platform_package();
  const cause = name
    ? `The engine for this machine ships in \`${name}\`, and it is not here.\n` +
      "npm installs that package as an optional dependency, and it omits an\n" +
      "optional dependency without failing, so an install that skipped it looks\n" +
      "like it worked.\n" +
      `  Install it:  npm install ${name}\n`
    : `No engine is published for ${process.platform}-${process.arch}. The built\n` +
      `platforms are: ${ENGINE_PLATFORMS.join(", ")}.\n` +
      "Building from source is the route on any other machine.\n";

  throw new GogError(
    "gog: cannot find the `gog-cli` binary — the engine that draws the plot.\n" +
      cause +
      "  Build it:  cargo build --release -p gog-cli\n" +
      "  Or point at one:  GOG_CLI_PATH=/path/to/gog-cli"
  );
}

// ---------------------------------------------------------------------------
// A JavaScript table → the wire
//
// The wire form is column-oriented and typed by which map a column lands in:
//
//   floats   {"gdp": [1.0, 2.0, null]}      numbers, and temporal values
//   strings  {"continent": ["Asia", null]}  text
//   levels   {"size": ["Low", "High"]}      a declared category order
//   dates    {"day": "day"}                 a column of floats read as time
// ---------------------------------------------------------------------------

// A column that remembers its category order — JavaScript's answer to R's
// `factor()` and pandas' `Categorical`, neither of which the language has. It is
// a host-language word like `col`, not a word of the grammar: dropping the
// declared order would make an ordered-category plot fall back to the order of
// the rows and say nothing, which is the silent drop §12 forbids.
export function ordered(values, levels) {
  if (!Array.isArray(values) || !Array.isArray(levels)) {
    throw new GogError(
      "gog: `ordered()` takes the column's values and its category order — " +
        '`ordered(["Low", "High"], ["Low", "Mid", "High"])`.'
    );
  }
  const column = [...values];
  Object.defineProperty(column, "levels", {
    value: levels.map(String),
    enumerable: false,
  });
  return column;
}

function isMissing(value) {
  return value === null || value === undefined || (typeof value === "number" && Number.isNaN(value));
}

// Exported since scale limits shipped: a domain on a temporal axis is written
// in dates, and it has to be converted by the same function the *column* is, or
// the two disagree silently (spec §10). Representation is the binding's job.
export function epochSeconds(value) {
  // A `Date` already carries an epoch, which is the engine's one temporal unit,
  // so this is exact rather than reconstructed from a calendar the way Python's
  // naive `datetime` has to be.
  return value.getTime() / 1000;
}

function columnValues(table, name) {
  const series = table[name];
  if (!Array.isArray(series)) {
    throw new GogError(
      `gog: column \`${name}\` is not a column — a column is an array of values, one ` +
        `per row. A single value is a length-1 array: \`{ ${name}: [${JSON.stringify(series)}] }\`.`
    );
  }
  return series;
}

export function to_wire(table, name) {
  if (table === null || typeof table !== "object" || Array.isArray(table)) {
    throw new GogError(
      "gog: `data()` takes a table — an object of columns, " +
        "`{ x: [1, 2], y: [3, 4] }`."
    );
  }

  const floats = {};
  const strings = {};
  const levels = {};
  const dates = {};

  for (const column of Object.keys(table)) {
    const values = columnValues(table, column);
    const present = values.filter((value) => !isMissing(value));

    // A column is one type — the engine's table has a `Float` column and a `Str`
    // column and nothing that is both. Deciding by majority, or by the first
    // row, would be the silent drop §12 forbids one level down, so a mixed
    // column is refused here where the caller can still see which column it was.
    if (present.length && present.every((value) => value instanceof Date)) {
      for (const value of present) {
        if (Number.isNaN(value.getTime())) {
          throw new GogError(
            `gog: column \`${column}\` of \`${name}\` holds an Invalid Date. A date ` +
              "that could not be parsed is not a missing value; fix it, or write `null`."
          );
        }
      }
      floats[column] = values.map((value) => (isMissing(value) ? null : epochSeconds(value)));
      // JavaScript has one `Date` where R has `Date` and `POSIXct`, so the unit
      // is read off the values rather than off a class: a column that lands
      // exactly on midnight everywhere is a run of days, and anything else keeps
      // its clock. That is the right answer rather than a guess at one — a
      // reader of all-midnight timestamps wants day ticks.
      dates[column] = present.every((value) => value.getTime() % 86400000 === 0)
        ? "day"
        : "second";
    } else if (values.every((value) => isMissing(value) || typeof value === "number")) {
      for (const value of values) {
        if (!isMissing(value) && !Number.isFinite(value)) {
          throw new GogError(
            `gog: column \`${column}\` of \`${name}\` holds ${value}, which is not a ` +
              "number a scale can place. Use `null` for a missing value."
          );
        }
      }
      floats[column] = values.map((value) => (isMissing(value) ? null : value));
    } else if (values.every((value) => isMissing(value) || typeof value !== "number")) {
      strings[column] = values.map((value) => (isMissing(value) ? null : String(value)));
      if (Array.isArray(values.levels)) levels[column] = [...values.levels];
    } else {
      const kinds = [
        ...new Set(present.map((value) => (value instanceof Date ? "Date" : typeof value))),
      ].sort();
      throw new GogError(
        `gog: column \`${column}\` of \`${name}\` mixes ${kinds.join(" and ")} — a column ` +
          "is one type, because a scale reads it as one kind of thing. Make it numbers " +
          "(a position, a magnitude) or text (a category)."
      );
    }
  }

  return { floats, strings, levels, dates };
}

// ---------------------------------------------------------------------------
// Render to an SVG string
// ---------------------------------------------------------------------------

// A table named by a SQL query instead of held in memory. Deliberately not
// executed when it is written: a query that ran at that moment would foreclose
// pushing the transform down to the database, because the planner has to see
// the whole sentence first. `resolveQuery` runs it once, at render.
export class Query {
  constructor(connection, sql) {
    this.connection = connection;
    this.sql = sql;
  }
}

// Run the query, as an object of columns.
//
// JavaScript is the one binding with **no database standard to lean on** — R has
// DBI, Python has PEP 249, Julia has DBInterface, and node has nothing. So this
// duck-types on the two shapes that actually appear, and refuses the rest by
// name rather than guessing.
//
// It also has a constraint the other three do not: `render_svg` is
// **synchronous**, so a driver whose query returns a Promise cannot be awaited
// here. `pg` and `mysql2` are async and are refused *with their own direction* —
// await the rows yourself and pass them to `data()`, which is a table like any
// other. That is a real limit, named out loud rather than left to fail as
// `[object Promise]` reaching the wire.
export function resolveQuery(query, table) {
  const con = query.connection;
  let rows;

  if (con && typeof con.prepare === "function") {
    // better-sqlite3, and node:sqlite (Node 22+) — both synchronous.
    const statement = con.prepare(query.sql);
    if (typeof statement.all !== "function") {
      throw new GogError(
        `gog: the connection for \`${table}\` prepared the query but the result ` +
          "has no `.all()`, so the rows cannot be read."
      );
    }
    rows = statement.all();
  } else if (con && typeof con.all === "function") {
    rows = con.all(query.sql);
  } else if (con && typeof con.query === "function") {
    throw new GogError(
      `gog: the connection for \`${table}\` looks asynchronous (\`pg\`, ` +
        "`mysql2`), and `render_svg()` is synchronous, so its rows cannot be " +
        "awaited here. Await them yourself and hand them over as a table: " +
        "`const { rows } = await con.query(sql)`, then `data(rows)`."
    );
  } else {
    throw new GogError(
      "gog: `query()` takes a database connection and a SELECT — " +
        "`query(con, 'SELECT ...')`. The connection must be a synchronous one " +
        "(`better-sqlite3`, `node:sqlite` — anything with `.prepare(sql).all()`). " +
        `Got ${describe(con)}. If the rows are already in hand, that is ` +
        "`data(rows)`."
    );
  }

  if (rows instanceof Promise) {
    throw new GogError(
      `gog: the query for \`${table}\` returned a Promise, and \`render_svg()\` ` +
        "is synchronous. Await the rows and pass them as a table: `data(rows)`."
    );
  }
  if (!Array.isArray(rows) || rows.length === 0) {
    throw new GogError(
      `gog: the query for \`${table}\` returned no rows, so there is nothing to ` +
        "draw and no columns to name."
    );
  }

  // Rows arrive as objects, one per row; the wire wants columns.
  const columns = {};
  for (const key of Object.keys(rows[0])) {
    columns[key] = rows.map((row) => row[key]);
  }
  return columns;
}

// The tables, resolved and turned into the wire's column-oriented form. Shared
// by `render_svg` and `html_block` so the SVG on the page and the spec the
// browser re-renders from can never describe different data.
function wireData(plot) {
  const data = {};
  for (const [name, table] of Object.entries(plot.frames)) {
    // A `query()` table is resolved here and nowhere else — one place, at
    // render, which is what leaves room for the planner to rewrite the sentence
    // before the database is ever asked (the pushdown design).
    data[name] =
      table instanceof Query ? to_wire(resolveQuery(table, name), name) : to_wire(table, name);
  }
  return data;
}

// The whole request, exactly as `gog-cli` reads it on stdin.
function wireRequest(plot) {
  return { spec: plot.spec, data: wireData(plot) };
}

// The engine's input for a plot or a page, as JSON — the one place either is
// turned into what `gog-cli` reads.
//
// Split out of `render_svg` when `save_gif` became a second caller. Two
// functions serializing the same object is two chances to disagree about a
// number's precision or about what a missing value crosses as, and that
// disagreement would surface as a GIF that does not match the plot beside it.
function wirePayload(plot) {
  return JSON.stringify({ spec: plot.spec, data: wireData(plot) });
}

export function render_svg(plot) {
  if (!plot || typeof plot !== "object" || !plot.spec || !plot.frames) {
    throw new GogError(
      "gog: `render_svg()` draws a plot — `render_svg(plot(data(df), point, " +
        "x(col.a), y(col.b)))`."
    );
  }
  // A page arrives here too, and asks nothing more of this function: it carries
  // a `spec` and its `frames` exactly as a plot does, and the engine tells the
  // two shapes apart itself (`ir::Figure`).

  const payload = wirePayload(plot);

  const result = spawnSync(find_gog_cli(), {
    input: payload,
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
  });

  if (result.error) {
    throw new GogError(`gog: could not run the engine — ${result.error.message}`);
  }

  const messages = (result.stderr || "").trim();
  if (result.status !== 0) {
    // The diagnostics *are* the error. Surfacing them as-is rather than wrapping
    // them in an exit-code message keeps the direction the engine wrote (§12).
    throw new GogError(messages || `gog-cli exited with status ${result.status}`);
  }

  // Non-fatal diagnostics — an Assumption, a dropped row — belong beside the
  // plot, not inside it: stderr, exactly where the engine put them.
  if (messages) process.stderr.write(`${messages}\n`);

  return result.stdout;
}

// ---------------------------------------------------------------------------
// Showing the plot — a file, an HTML host
// ---------------------------------------------------------------------------

// The SVG wrapped for an HTML host, sized to fit its column.
//
// **Whatever size the canvas is.** This matched the literal 800x600 for as long
// as that was the only canvas, so `size()` on a plot quietly opted it out of
// fitting. Anchored inside the opening `<svg` tag, because `[^>]` cannot cross
// the tag's own `>` — which keeps it off the background `<rect>` carrying the
// same two numbers a few characters later.
const FIT = [/(<svg[^>]*) width="(\d+)" height="(\d+)"/,
             '$1 width="$2" height="$3" style="max-width:100%;height:auto;"'];

export function svg_block(svg) {
  const sized = svg.replace(...FIT);
  return `<div class="gog-plot" style="text-align:center;">\n${sized}\n</div>`;
}

// Draw the plot and write the SVG to `file`. Returns the path.
export function save(plot, file) {
  if (typeof file !== "string" || !file) {
    throw new GogError('gog: `save()` needs a path — `save(plot, "plot.svg")`.');
  }
  fs.writeFileSync(file, render_svg(plot), "utf8");
  return file;
}

// Write a played plot to an animated GIF. Returns the path.
//
// A plot that binds `play()` moves in a browser, because the SVG carries its own
// timing. Most other places do not read that: a message to a friend, a slide, a
// post. This writes the same sequence as a GIF, which they do read.
//
// The frames come out of the one renderer, so the file cannot disagree with the
// plot. Every scale, the color map and each legend are fitted across the whole
// sequence at once, and the moments are cut from that single drawing rather than
// drawn again one at a time. Nothing needs to be installed.
//
// `scale` multiplies the plot's canvas, which is 800 by 600 unless its theme
// says otherwise — small for a post, so `scale: 2` doubles it.
export function save_gif(plot, file, options = {}) {
  if (!plot || typeof plot !== "object" || !plot.spec || !plot.frames) {
    throw new GogError(
      'gog: `save_gif()` writes a plot — `save_gif(plot(...), "wave.gif")`.'
    );
  }
  if (typeof file !== "string" || !file) {
    throw new GogError('gog: `save_gif()` needs one path — `save_gif(p, "wave.gif")`.');
  }
  // The name says what the file is, so a path that says otherwise is refused
  // rather than quietly corrected. Writing GIF bytes into `wave.png` is the kind
  // of small lie that is discovered much later, by someone else.
  if (!file.toLowerCase().endsWith(".gif")) {
    const stem = file.replace(/\.[^.]*$/, "") || file;
    throw new GogError(
      "gog: `save_gif()` writes a GIF, so the path ends in `.gif` — " +
        `\`save_gif(p, "${stem}.gif")\`.`
    );
  }
  const scale = options.scale ?? 1;
  if (typeof scale !== "number" || !Number.isFinite(scale) || scale <= 0) {
    throw new GogError(
      "gog: `save_gif({ scale })` needs one positive number, e.g. " +
        '`save_gif(p, "wave.gif", { scale: 2 })`.'
    );
  }

  const result = spawnSync(
    find_gog_cli(),
    ["--gif", file, "--scale", String(scale)],
    { input: wirePayload(plot), encoding: "utf8", maxBuffer: 256 * 1024 * 1024 }
  );

  if (result.error) {
    throw new GogError(`gog: could not run the engine — ${result.error.message}`);
  }
  const messages = (result.stderr || "").trim();
  if (result.status !== 0) {
    throw new GogError(messages || `gog-cli exited with status ${result.status}`);
  }
  if (messages) process.stderr.write(`${messages}\n`);
  return file;
}

// Where the browser assets live, when a host wants them beside the page rather
// than carried inside it. A book sets these; a script leaves them empty and gets
// the bytes inline, which is the form that works from a `file://` temp path.
export const assetUrls = {
  wasm: process.env.GOG_WASM_URL || null,
  js: process.env.GOG_JS_URL || null,
};

// The engine and its runtime, or null — in which case plots stay static.
// Walked up to rather than counted, since the distance to the repository root
// differs between a script, a test and an installed package.
function findWasmAssets() {
  const bundled = [
    path.join(HERE, "..", "www", "gog.wasm"),
    path.join(HERE, "interactive.js"),
  ];
  if (bundled.every((p) => fs.existsSync(p))) return bundled;

  for (const start of [process.cwd(), HERE]) {
    let root = path.resolve(start);
    for (let i = 0; i < 7; i++) {
      const pair = [
        path.join(root, "gog-wasm/target/wasm32-unknown-unknown/release/gog_wasm.wasm"),
        path.join(root, "js-pkg/gog/src/interactive.js"),
      ];
      if (pair.every((p) => fs.existsSync(p))) return pair;
      const parent = path.dirname(root);
      if (parent === root) break;
      root = parent;
    }
  }
  return null;
}

// The modules' own source, ready to sit inside `<script type="module">`.
//
// **A `data:` URL cannot be imported where a page has a content-security
// policy**, and every host that shows a plot outside a plain browser sets one:
// JupyterLab, VS Code notebooks, and the Positron and RStudio viewer panes.
// `script-src` there does not list `data:`, so the import is refused *silently* —
// a blocked module import throws nothing the page can catch. The static SVG
// still drew and every control was missing.
//
// Inlining survives that policy: an inline module runs under
// `script-src 'unsafe-inline'` and needs no URL of any scheme.
const inlineModules = (files) =>
  files
    .map((f) => fs.readFileSync(f, "utf8"))
    .join("\n")
    // `interactive.js` takes its view helpers from the sibling file. Inlined,
    // that specifier has nothing to resolve against, and both files are already
    // in this one scope, so the two statements naming it go.
    .replace(/(?:import|export)\s*\{[^}]*\}\s*from\s*"\.\/view\.js";?/g, "");

// The engine as an expression evaluating to its bytes. `loadEngine` takes a URL
// *or* a BufferSource, so this is the second of the two: no fetch, no scheme,
// nothing the policy can refuse.
const wasmExpression = (file) =>
  assetUrls.wasm
    ? `"${assetUrls.wasm}"`
    : `Uint8Array.from(atob("${fs.readFileSync(file).toString("base64")}"), c => c.charCodeAt(0))`;

// An `import` needs a module specifier, which is stricter than a URL a `fetch`
// would take. A bare word like `"gog.js"` is reserved for import maps, so a
// browser refuses it outright: the script never runs, nothing is fetched, and
// the page shows the static plot with nothing in the console to say why.
const moduleSpecifier = (url) =>
  /^(data:|https?:|file:|\/|\.\/|\.\.\/)/.test(url) ? url : "./" + url;

// Does this spec draw in the cube? The twin of `isSpatial` in the browser
// module and of `space_of` in the engine — a bound `z` projects a plot even
// when the coordinate still reads "flat".
// Two reasons to carry the engine, not one. A plot in the cube has an angle worth dragging; a plot that names a brush has a bound worth moving. A flat plot with neither stays a still image and pays nothing.
function specNeedsEngine(spec) {
  if ((spec?.brush?.length ?? 0) > 0) return true;
  if ((spec?.cells ?? spec?.plots ?? []).some(specNeedsEngine)) return true;
  return specIsSpatial(spec);
}

function specIsSpatial(spec) {
  // The cube's view, or the globe's: both carry an angle worth dragging.
  if (spec?.coord && typeof spec.coord === "object" && (spec.coord.space || spec.coord.globe)) return true;
  // A network with a *stated* angle is the cube form and turns; bare
  // `network()` is flat and has nothing to drag.
  {
    const net = spec?.coord && typeof spec.coord === "object" ? spec.coord.network : null;
    if (net && (net.turn !== undefined || net.tilt !== undefined)) return true;
  }
  if (spec?.z != null) return true;
  if ((spec?.layers ?? []).some((l) => (l?.encodings ?? {}).z != null)) return true;
  return (spec?.cells ?? spec?.plots ?? []).some(specIsSpatial);
}

/**
 * The SVG wrapped for an HTML host, plus — for a plot in the cube — the script
 * that makes it turnable.
 *
 * The static SVG is still what is written, and it is what a reader sees with no
 * JavaScript and before the engine loads. The script only upgrades a picture
 * that is already there, so when the assets are missing the plot simply stays
 * still.
 *
 * **Public, and named the way the other exports are.** The other three bindings
 * never need this by hand: R registers `repr_html`, Python defines
 * `_repr_html_`, Julia writes a `text/html` show method, and each one hands a
 * notebook the turnable block on its own. JavaScript has no such protocol to
 * register with, so the block has to be reachable as a function or it is not
 * reachable at all — which it was not, until a test round measured that
 * `svg_block(render_svg(p))` was the only thing a reader could put in a page,
 * and that it draws a picture nobody can turn.
 */
export function html_block(plot) {
  const svg = render_svg(plot).replace(...FIT);
  const spec = plot.spec ?? plot;
  // Two questions, not one. The *engine* has two reasons — an angle worth
  // dragging, a bound worth moving — and both redraw. The *module* has a third,
  // and it is every plot: looking closer. A zoom scales the viewBox and
  // recomputes nothing, so it needs this file and not the WebAssembly beside it.
  const needsEngine = specNeedsEngine(spec);
  const assets = findWasmAssets();
  if (!assets) return `<div class="gog-plot" style="text-align:center;">\n${svg}\n</div>`;

  // A flat plot names the smaller module and sends no data.
  //
  // **The script goes inside the container**, which is a layout rule rather than
  // a style choice. Quarto's `layout-ncol` divides a chunk's output into cells by
  // counting top-level blocks, so a `<div>` with a sibling `<script>` is two
  // cells and two plots become four — wrapping into two rows, each plot alone at
  // full width beside an empty cell holding only its script. One element is one
  // cell. Nothing else cares where it sits: the container is resolved by id, the
  // SVG is still its first element, and a redraw can only remove a module script
  // that has already run.
  if (!needsEngine) {
    const viewPath = path.join(path.dirname(assets[1]), "view.js");
    const head = assetUrls.js
      ? `import { mountView } from "${moduleSpecifier(assetUrls.js.replace("interactive.js", "view.js"))}";\n`
      : inlineModules([viewPath]) + "\n";
    const vid = "gog-" + Math.abs(hashOf(svg)).toString(36).padStart(10, "0").slice(0, 10);
    return (
      `<div class="gog-plot" id="${vid}" style="text-align:center;">\n${svg}\n` +
      `<script type="module">\n${head}` +
      `mountView("${vid}");\n</script>\n</div>`
    );
  }

  // The module arrives one of two ways, and the engine likewise. A book names
  // files it serves; everything else carries them, because a notebook cell has
  // no server behind it and a temp page in a viewer pane has no directory.
  const head = assetUrls.js
    ? `import { mount } from "${moduleSpecifier(assetUrls.js)}";\n`
    : inlineModules([path.join(path.dirname(assets[1]), "view.js"), assets[1]]) + "\n";
  const id = "gog-" + Math.abs(hashOf(svg)).toString(36).padStart(10, "0").slice(0, 10);
  const request = JSON.stringify(wireRequest(plot));

  return (
    `<div class="gog-plot" id="${id}" style="text-align:center;">\n${svg}\n` +
    `<script type="module">\n${head}` +
    `mount("${id}", ${request}, { wasm: ${wasmExpression(assets[0])} });\n</script>\n</div>`
  );
}

// A stable id from the drawing itself. `Math.random` is avoided deliberately:
// the same plot rendered twice should produce the same file, so a book build is
// reproducible and a diff of two renders shows what actually changed.
function hashOf(s) {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h | 0;
}

// Draw the plot and write it to a standalone HTML file, for a script that wants
// to look at one. Returns the path.
export function show(plot) {
  // A page has cells where a plot has layers; either way the count only has to
  // keep two files in one process from colliding.
  const parts = plot.spec.layers?.length ?? plot.spec.cells?.length ?? 0;
  const file = path.join(os.tmpdir(), `gog-${process.pid}-${parts}.html`);
  fs.writeFileSync(
    file,
    "<!DOCTYPE html>\n<html>\n<head><meta charset='utf-8'>" +
      "<style>body{margin:0;background:#fff;display:flex;" +
      "justify-content:center;padding:16px;}</style></head>\n<body>\n" +
      `${html_block(plot)}\n</body>\n</html>`,
    "utf8"
  );
  return file;
}
