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

  const data = {};
  for (const [name, table] of Object.entries(plot.frames)) {
    data[name] = to_wire(table, name);
  }
  const payload = JSON.stringify({ spec: plot.spec, data });

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
export function svg_block(svg) {
  const sized = svg.replace(
    'width="800" height="600"',
    'width="800" height="600" style="max-width:100%;height:auto;"'
  );
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
      `${render_svg(plot)}\n</body>\n</html>`,
    "utf8"
  );
  return file;
}
