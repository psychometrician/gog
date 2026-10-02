// Basic sanity test for the JavaScript binding.
//
//     node --test js-pkg/gog/test/
//
// The mirror of `py-pkg/gog/tests/test_basic.py` and `r-pkg/gog/tests/test_basic.R`:
// does a sentence reach the engine, does the engine draw it, and do the refusals
// refuse. It loads the package from source and finds `gog-cli` the way a user's
// first plot would.
//
// The checks that are *this binding's own* are the ones spec §8 decided: the four
// words that spell the four operators, the mandatory accessor, the options
// object, and the table name JavaScript cannot read off a variable.

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import { spawnSync } from "node:child_process";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";

import { ENGINE_PLATFORMS, Query, find_gog_cli, platform_package } from "../src/render.js";

import {
  GogError,
  query,
  across,
  area,
  bar,
  bin,
  bounds,
  partition,
  flow,
  layout,
  cluster,
  label,
  box,
  col,
  color,
  colour,
  confidence,
  count,
  data,
  dodge,
  median,
  quantile,
  deviation,
  beside,
  below,
  down,
  facet,
  interval,
  layer,
  line,
  map,
  mean,
  sum,
  ordered,
  order,
  palette,
  pattern,
  jitter,
  path,
  play,
  brush,
  plot,
  point,
  density,
  polar,
  nest,
  network,
  edge,
  opacity,
  size,
  globe,
  proportion,
  group,
  render_svg,
  save,
  ribbon,
  rule,
  save_gif,
  save_svg,
  shape,
  range,
  smooth,
  space,
  stack,
  repel,
  step,
  style,
  theme,
  text,
  title,
  x,
  x_label,
  y,
  y_label,
  z,
  zone,
  surface,
  html_block,
  show,
  to_wire,
} from "../src/index.js";

const df = {
  x: [1, 2, 3, 4, 5],
  y: [2.5, 3.1, 1.8, 4.0, 3.5],
  group: ["A", "B", "A", "B", "A"],
};
const bars = { category: ["A", "B", "C"], value: [10, 25, 15] };
const gaps = { a: [1, null, 3, 4], b: [2, 2.5, null, 4.5] };

function refuses(thunk, fragment) {
  assert.throws(thunk, (error) => {
    assert.ok(error instanceof GogError, `not a GogError: ${error}`);
    assert.match(error.message, /gog:/, "a refusal says `gog:`");
    if (fragment) assert.match(error.message, fragment);
    return true;
  });
}

// ---------------------------------------------------------------------------
// The four words — `+`, `*`, `|`, `/` in a language that cannot spell them
// ---------------------------------------------------------------------------

test("the comma is `+`: atoms accumulate left to right", () => {
  const p = plot(data(df), point, x(col.x), y(col.y));
  assert.equal(p.spec.layers.length, 1);
  assert.equal(p.spec.layers[0].mark, "point");
  // Written *after* the mark, so they are that layer's — position decides
  // scope, and the comma changes none of that.
  assert.equal(p.spec.layers[0].encodings.x.field, "x");
  assert.equal(p.spec.layers[0].encodings.y.field, "y");
  assert.equal(p.spec.x, null);
});

test("position decides scope, and the comma reads the same way `+` does", () => {
  const before = plot(data(df), x(col.x), y(col.y), point);
  assert.equal(before.spec.x.field, "x");
  assert.deepEqual(before.spec.layers[0].encodings, {});
});

test("layer() is `*`: a mark with its transforms", () => {
  const p = plot(data(bars), layer(bar, mean), x(col.category), y(col.value));
  assert.deepEqual(p.spec.layers[0].transforms, ["mean"]);
});

test("layer() nests, so `*` binding tighter than `+` is visible", () => {
  // `bar * bin + color(g)` in R: the transform is inside the mark, the channel
  // beside it. Nesting says that where a precedence rule had to be known.
  const p = plot(data(df), layer(bar, bin), x(col.x), color(col.group));
  assert.deepEqual(p.spec.layers[0].transforms, ["bin"]);
  assert.equal(p.spec.layers[0].encodings.color.field, "group");
  assert.deepEqual(p.spec.channels, {});
});

test("`text * repel` separates a label crowd and keeps every label", () => {
  // The fourth offset, and the one that moves ink (spec §5). `dodge`, `stack`
  // and `jitter` resolve marks that share a *position*; a label is as wide as
  // the word it draws, so two labels overlap where their points never did.
  const crowd = {
    px: [5, 5, 5, 5, 5, 5],
    py: [5, 5, 5, 5, 5, 5],
    who: ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"],
  };
  const labelAt = (svg) =>
    [...svg.matchAll(/<text x="([0-9.-]+)" y="([0-9.-]+)" fill="[^"]*" fill-opacity=/g)]
      .map((m) => [Number(m[1]), Number(m[2])]);

  const plain = labelAt(render_svg(plot(data(crowd), text, x(col.px), y(col.py), label(col.who))));
  assert.equal(new Set(plain.map(String)).size, 1, "six coincident rows, one place");

  const spec = plot(data(crowd), layer(text, repel), x(col.px), y(col.py), label(col.who));
  const svg = render_svg(spec);
  const moved = labelAt(svg);
  assert.equal(moved.length, 6, "repel must draw every label, never leave one out");
  for (let i = 0; i < 6; i += 1) {
    for (let j = i + 1; j < 6; j += 1) {
      const apart = Math.max(Math.abs(moved[i][0] - moved[j][0]), Math.abs(moved[i][1] - moved[j][1]));
      assert.ok(apart > 7, `repel left labels ${i} and ${j} on top of each other`);
    }
  }
  // One specification is one picture, however the placement anneals.
  assert.equal(svg, render_svg(spec), "repel must render identically every run");
  // A label pushed clear of its dot keeps a line back to it. Six names ring
  // their shared point at resting distance, so none travels; it takes a deeper
  // crowd, whose outer ranks are held off by the inner ones, to earn the
  // connector.
  const deep = {
    px: Array(14).fill(5),
    py: Array(14).fill(5),
    who: [..."ABCDEFGHIJKLMN"].map((c) => `crew ${c}`),
  };
  const deepSvg = render_svg(plot(data(deep), layer(text, repel), x(col.px), y(col.py), label(col.who)));
  assert.match(deepSvg, /stroke-width="0.7"/, "a travelled label should keep its leader");
  // It is `text`-only, and each refusal names the offset that fits.
  refuses(() => render_svg(plot(data(crowd), layer(point, repel), x(col.px), y(col.py))), /jitter/);
  refuses(() => render_svg(plot(data(crowd), layer(bar, repel), x(col.who), y(col.py))), /dodge/);
});

test("across() and down() are `|` and `/`", () => {
  const p = plot(data(df), point, x(col.x), y(col.y), across(col.group));
  assert.deepEqual(p.spec.facet, { col: "group", row: null });

  const grid = plot(data(df), point, x(col.x), y(col.y), across(col.group), down(col.group));
  assert.deepEqual(grid.spec.facet, { col: "group", row: "group" });
});

// The cube takes a facet too, one projected box per panel. Refused as "not drawn
// yet" until 2026-07-28, when it turned out the renderer had always built its
// scene from the panel's own rectangle and only the check said otherwise.
test("a faceted cube projects one scene per panel", () => {
  const cubes = { ...df, z: [1, 5, 2, 6, 3] };
  const svg = render_svg(
    plot(data(cubes), point, x(col.x), y(col.y), z(col.z), across(col.group)),
  );
  const count = (needle) => svg.split(needle).length - 1;
  assert.equal(count('fill="#f5f5f8"'), 2, "one panel per level of `group`");
  assert.equal(count('stroke="#d8d8de"'), 2, "and each panel projects its own cube");
});

// The darker edges are the three the numbers are written along. They were the
// three from the cube's back corner, which all start at one point.
test("the cube darkens the three edges its numbers are written along", () => {
  const svg = render_svg(plot(data({ ...df, z: [1, 5, 2, 6, 3] }), point, x(col.x), y(col.y), z(col.z)));
  const dark = svg.match(/<g stroke="#9a9aa4" stroke-width="1.4"[\s\S]*?<\/g>/)[0];
  const starts = [...dark.matchAll(/x1="[0-9.]+" y1="[0-9.]+"/g)].map((m) => m[0]);
  assert.equal(starts.length, 3);
  assert.ok(new Set(starts).size > 1, `${starts}`);
});

test("wrap folds the line of panels, and the word says which way it runs", () => {
  const wrapped = plot(data(df), point, x(col.x), y(col.y), across(col.group, { wrap: 4 }));
  assert.deepEqual(wrapped.spec.facet, { col: "group", row: null, wrap: 4 });

  const down4 = plot(data(df), point, x(col.x), y(col.y), down(col.group, { wrap: 4 }));
  assert.deepEqual(down4.spec.facet, { col: null, row: "group", wrap: 4 });

  // No `wrap` written, nothing on the wire — an unwrapped facet is unmoved.
  assert.deepEqual(
    plot(data(df), point, x(col.x), y(col.y), across(col.group)).spec.facet,
    { col: "group", row: null }
  );

  refuses(() => across(col.group, { wrap: 2.5 }), /whole number/);
  refuses(() => across(col.group, { wrap: true }), /whole number/);
  refuses(() => across(col.group, { ncol: 4 }), /\{ wrap \}/);
});

test("a free scale is fitted per panel, and only the axis that asked", () => {
  const free = { x: [1, 2, 1, 2, 1, 2], y: [1, 2, 100, 200, 10, 20],
                 g: ["a", "a", "b", "b", "c", "c"] };
  const shared = render_svg(plot(data(free), point, x(col.x), y(col.y), across(col.g)));
  const freed = render_svg(
    plot(data(free), point, x(col.x), y(col.y, { free: true }), across(col.g))
  );
  // Shared, the axis spans 1..200 and never ticks 20; freed, each panel does.
  assert.ok(!shared.includes(">20</text>"));
  assert.ok(freed.includes(">200</text>") && freed.includes(">20</text>"));

  refuses(
    () => render_svg(plot(data(free), point, x(col.x), y(col.y, { free: true }))),
    /one panel/
  );
  refuses(
    () => render_svg(plot(data(free), point, x(col.x),
                          y(col.y, { limits: [0, 300], free: true }), across(col.g))),
    /one scale per panel/
  );
  refuses(() => y(col.y, { free: "yes" }), /true or false/);
  // Every channel takes `free` and forwards it, so the engine's refusal is the one
  // a reader meets rather than "has no `free`", and it names the flag without any
  // one binding's spelling of it.
  refuses(
    () => render_svg(plot(data(free), point, x(col.x), y(col.y),
                          color(col.g, { free: true }), across(col.g))),
    /`color\(g\)` cannot be freed[\s\S]*Write `free` on a position/
  );
  refuses(() => color(col.g, { free: "yes" }), /`y\(col\.<name>, \{ free: true \}\)` frees y/);
});

test("wrap draws one panel per level and names every one", () => {
  const levels = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"];
  const wide = {
    x: levels.flatMap(() => [0, 1]),
    y: levels.flatMap((_, i) => [i * 2, i * 2 + 1]),
    g: levels.flatMap((level) => [level, level]),
    h: levels.flatMap(() => ["u", "v"]),
  };
  const svg = render_svg(
    plot(data(wide), point, x(col.x), y(col.y), across(col.g, { wrap: 4 }))
  );
  // Ten levels are ten panels, not the 4 x 3 rectangle's twelve cells: the slack
  // the fold left over is not a combination, so nothing is drawn there.
  assert.equal(svg.match(/fill="#f5f5f8"/g).length, 10);
  for (const level of levels) {
    assert.ok(svg.includes(`>${level}</text>`), `wrapped panel ${level} needs its own name`);
  }
  assert.notEqual(
    svg,
    render_svg(plot(data(wide), point, x(col.x), y(col.y), down(col.g, { wrap: 4 })))
  );
  refuses(
    () =>
      render_svg(
        plot(data(wide), point, x(col.x), y(col.y), across(col.g, { wrap: 2 }), down(col.h))
      ),
    /Drop `wrap`/
  );
});

test("facet() exists only to say where it went", () => {
  refuses(() => facet(col.group), /across\(col\.group\)/);
  refuses(() => facet(col.group), /down\(col\.group\)/);
});

test("Law 6: a sub-expression means the same thing in every sentence", () => {
  const piled = layer(point, bin, stack);
  const one = plot(data(df), piled, x(col.x));
  const two = plot(data(df), piled, x(col.y));
  assert.deepEqual(one.spec.layers[0].transforms, ["bin", "stack"]);
  assert.deepEqual(two.spec.layers[0].transforms, ["bin", "stack"]);
  // The second sentence must not have grown the first one's layer.
  assert.equal(one.spec.layers.length, 1);
  assert.notEqual(one.spec.layers[0], two.spec.layers[0]);
});

test("a layer holds a mark and its transforms, and says so otherwise", () => {
  refuses(() => layer(bin, bar), /starts with a mark/);
  refuses(() => layer(bar, color(col.group)), /joins the sentence/);
  refuses(() => layer(), /how JavaScript spells/);
});

// ---------------------------------------------------------------------------
// Capture — `col.name` is a column, everything else is a value
// ---------------------------------------------------------------------------

test("a channel takes a column, never a value", () => {
  refuses(() => x("gdp"), /x\(col\.gdp\)/);
  refuses(() => color("red"), /style\(\{ color: "red" \}\)/);
  refuses(() => color([1, 2, 3]), /values/);
  refuses(() => x(col), /accessor/);
  refuses(() => x(42), /takes a column/);
});

test("a setting takes a value, never a column — the same rule from the other side", () => {
  refuses(() => style({ color: col.group }), /that is a channel: `color\(col\.group\)`/);
  refuses(() => style({ nonsense: 1 }), /is not a setting/);
  refuses(() => style({}), /sets nothing/);
});

test("one spelling of English, and the refusal names which", () => {
  // A reader arriving from ggplot2 types `colour` because there it works, so
  // the refusal names the word to write rather than listing every setting.
  for (const [british, american] of [
    ["colour", "color"],
    ["border_colour", "border_color"],
    ["centre", "center"],
  ]) {
    refuses(
      () => style({ [british]: "red" }),
      new RegExp(`gog spells it \`${american}\`[\\s\\S]*ggplot2`)
    );
  }
  refuses(() => colour(col.species), /gog spells it `color\(col\.<name>\)`/);
});

test("col spells a name JavaScript cannot write bare", () => {
  assert.equal(x(col["life exp"]).fields.field, "life exp");
  refuses(() => col(), /not a function/);
});

// ---------------------------------------------------------------------------
// A named argument is one trailing options object
// ---------------------------------------------------------------------------

test("positional stays positional, named joins one object", () => {
  assert.equal(x(col.x, { scale: "log" }).fields.scale, "log");
  assert.equal(x(col.x, "log").fields.scale, "log");
  assert.equal(x(col.x, { scale: "log", base: 2 }).fields.base, 2);
  assert.equal(bin(30).fields.bins, 30);
  assert.equal(bin({ width: 5 }).fields.width, 5);
  assert.equal(space(45, 20).fields.tilt, 20);
  assert.equal(polar(90).fields.start, 90);
  assert.equal(box("range").fields.box.whiskers, "range");
  // The whisker rule is one of two words. The refusal naming them was untested
  // in all four suites until 0.0.4, which is how R's copy of this message
  // shipped for several releases writing `--` where the other three wrote a
  // dash. A message nothing triggers is a message nothing checks.
  assert.throws(() => box("middle"), /is either/);

  // `GOG_STRICT=0` does not reach a refusal raised while the atom is built, and
  // the manual says so, so the claim is pinned rather than left to reasoning. The
  // switch trades a refusal for a picture; there is no picture on offer when the
  // atom was never built, and downgrading could only invent a value gog was not
  // given. What would break this is moving such a check into the engine, where
  // the switch does reach it — which is exactly the refactor the ruling declines.
  const before = process.env.GOG_STRICT;
  process.env.GOG_STRICT = "0";
  try {
    assert.throws(() => box("middle"), /gog:/);
    assert.throws(() => deviation(-1), /gog:/);
  } finally {
    if (before === undefined) delete process.env.GOG_STRICT;
    else process.env.GOG_STRICT = before;
  }
});

test("an unknown key is refused, not ignored", () => {
  refuses(() => x(col.x, { scal: "log" }), /has no `scal`/);
  refuses(() => bin({ bins: 10, width: 5 }), /either `bins` or `width`/);
  refuses(() => x(col.x, { scale: "logarithmic" }), /is not a scale/);
  refuses(() => x(col.x, { scale: "log", base: 0.5 }), /greater than 1/);
});

// `category` is the third scale chosen from the column's *type*, and since
// 2026-07-28 the third that may be said out loud for nothing — the allowance
// `linear` has on a number and `time` has on a date (spec §10). It may not
// contradict the column, though: a scale says how a measured column is placed,
// and whether an axis measures at all is the column's type (§18).
test("`category` may be said on a text column, and refused on a number", () => {
  const t = { place: ["a", "b", "c"], life: [4, 5, 6], gdp: [1, 2, 3] };
  const plain = render_svg(plot(data(t), layer(bar, mean), x(col.place), y(col.life)));
  const said = render_svg(
    plot(data(t), layer(bar, mean), x(col.place, { scale: "category" }), y(col.life))
  );
  assert.equal(said, plain, "saying it out loud must draw the same plot");

  refuses(
    () => render_svg(plot(data(t), point, x(col.gdp, { scale: "category" }), y(col.life))),
    /Store `gdp` as text or categories/
  );
});

test("an atom that takes nothing says so when called", () => {
  refuses(() => mean(), /takes no parameters/);
  refuses(() => point(), /takes no parameters/);
});

test("bounds names columns in every argument, and reads like the rest", () => {
  const atom = bounds(col.lo, col.hi, { start: col.a, end: col.b });
  assert.equal(atom.fields.lower, "lo");
  assert.equal(atom.fields.end, "b");
  refuses(() => bounds(), /needs column names/);
});

// ---------------------------------------------------------------------------
// The table, and the name JavaScript cannot read off a variable
// ---------------------------------------------------------------------------

test("one table needs no name", () => {
  const p = plot(data(df), point, x(col.x), y(col.y));
  assert.equal(p.spec.data, "data");
  assert.deepEqual(Object.keys(p.frames), ["data"]);
});

test("two tables get distinct names, because a name's job is to distinguish", () => {
  const notes = { at: [2], value: [3], note: ["here"] };
  const p = plot(data(df), point, x(col.x), y(col.y), data(notes), text, x(col.at), y(col.value));
  assert.deepEqual(Object.keys(p.frames), ["data", "data2"]);
  assert.equal(p.spec.layers[1].data, "data2");
});

test("the same table twice is a restatement, not a clash", () => {
  const p = plot(data(df), point, x(col.x), y(col.y), data(df), line);
  assert.deepEqual(Object.keys(p.frames), ["data"]);
});

test("a name can be given, and two different tables cannot share one", () => {
  const notes = { at: [2], value: [3] };
  const p = plot(data(df, { name: "series" }), point, x(col.x), y(col.y));
  assert.equal(p.spec.data, "series");
  assert.equal(plot(data(df, "series"), point, x(col.x), y(col.y)).spec.data, "series");
  refuses(
    () => plot(data(df, { name: "s" }), point, x(col.x), y(col.y), data(notes, { name: "s" }), text),
    /two different tables are both called/
  );
});

test("a page of two anonymous tables draws what naming them would draw", () => {
  const other = { x: [3, 4], y: [5, 6] };
  // Neither plot can read a name, so the binding invents `data` for both. That
  // is its own name and means nothing to the author, so the second gives way
  // rather than colliding — the same rule a plot of two tables already follows.
  const bare = beside(
    plot(data(df), point, x(col.x), y(col.y)),
    plot(data(other), point, x(col.x), y(col.y))
  );
  const named = beside(
    plot(data(df, { name: "one" }), point, x(col.x), y(col.y)),
    plot(data(other, { name: "two" }), point, x(col.x), y(col.y))
  );
  assert.equal(Object.keys(bare.frames).length, 2);
  // The picture is the test: a rename that pointed both cells at one table
  // would draw too, and only this catches that.
  assert.equal(render_svg(bare), render_svg(named));

  // A name the author wrote still cannot be moved.
  refuses(
    () => beside(
      plot(data(df, { name: "s" }), point, x(col.x), y(col.y)),
      plot(data(other, { name: "s" }), point, x(col.x), y(col.y))
    ),
    /two different tables on one page are both called/
  );
});

test("a refused save() leaves an existing file alone", () => {
  // Julia's `save()` opened the destination before it knew the render had
  // succeeded, and opening for writing truncates, so a refused plot emptied
  // whatever was there. `writeFileSync` evaluates the render first and so
  // cannot, and this holds it to that: the ordering is easy to reverse.
  const dir = fs.mkdtempSync(`${os.tmpdir()}/gog-save-`);
  const file = `${dir}/plot.svg`;
  const good = plot(data(df), point, x(col.x), y(col.y));
  const bad = plot(data(df), point, x(col.x), y(col.y), palette("okabe"));

  save_svg(good, file);
  const before = fs.readFileSync(file, "utf8");
  assert.ok(before.length > 0);

  assert.throws(() => save_svg(bad, file), GogError);
  assert.equal(fs.readFileSync(file, "utf8"), before);
  // `save()` is retired for `save_svg()`. It refuses, naming the call that
  // replaces it with the reader's own path, and it writes nothing either.
  assert.throws(() => save(good, file),
    (e) => e.message.startsWith(`gog: \`save()\` is retired: \`save_svg(p, "${file}")\``));
  assert.equal(fs.readFileSync(file, "utf8"), before);
});

test("a plot starts with its table", () => {
  refuses(() => plot(point, x(col.x)), /starts with its table/);
  refuses(() => plot(), /nothing to draw/);
  refuses(() => plot(data(df), 42), /takes gog atoms/);
  refuses(() => data(point), /not an atom/);
  refuses(() => data("gm"), /takes a table/);
});

// `gog_table()` is asynchronous in JavaScript alone, so a forgotten `await` is the
// commonest way a Promise reaches `data()`. It is an object, so it passed as a table
// and the plot was refused later for a column it could not find, with advice to check
// a spelling that was right. Refused at the door, naming the `await`. (The other three
// bindings' `gog_table()` returns the table itself, so they have nothing to refuse.)
test("a Promise handed to data() is refused with its await", () => {
  const pending = Promise.resolve({ country: ["a"], gold: [1] });
  refuses(() => data(pending), /got a Promise rather than a table[\s\S]*await gog_table/);
  refuses(() => plot(data(pending), bar, x(col.country), y(col.gold)),
    /await gog_table/);
  return pending;
});

// ---------------------------------------------------------------------------
// The wire, and the engine behind it
// ---------------------------------------------------------------------------

test("a sentence reaches the engine and comes back an SVG", () => {
  const svg = render_svg(plot(data(df), point, x(col.x), y(col.y), color(col.group)));
  assert.match(svg, /^<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg"/);
  assert.match(svg, /<circle/);
});

// `play` is `across()` read in time — the same split, laid out in sequence
// rather than over the page. The options object is this binding's own idiom, so
// `speed` is checked through it rather than as a keyword.
test("play cuts one frame per value, names each, and speeds up through the options object", () => {
  const played = {
    x: [1, 2, 3, 10, 20, 30],
    y: [1, 2, 3, 10, 20, 30],
    year: [1957, 1957, 1957, 1962, 1962, 1962],
  };
  const frames = (svg) => svg.match(/<animate attributeName="display"/g)?.length ?? 0;

  const svg = render_svg(plot(data(played), point, x(col.x), y(col.y), play(col.year)));
  // Two moments, once for the marks and once for the strip that names them.
  assert.equal(frames(svg), 4);
  assert.match(svg, />1957<\/text>/);
  assert.match(svg, />1962<\/text>/);
  assert.doesNotMatch(svg, />1957\.0</, "a year is named, not measured");

  // The invariant the feature rests on: no play, no timing, no bytes.
  const still = render_svg(plot(data(played), point, x(col.x), y(col.y)));
  assert.doesNotMatch(still, /<animate/);

  const fast = render_svg(
    plot(data(played), point, x(col.x), y(col.y), play(col.year, { speed: 2 })),
  );
  assert.equal(frames(fast), 4, "speed changes the pace, not how many frames there are");
  assert.match(fast, /dur="0\.800s"/);

  refuses(() => play(col.year, { speed: 0 }), /above zero/);
});

// The same sequence written where SVG animation is not read. Checked as a file,
// because everything this adds happens after the SVG above: the header proves it
// is a GIF, the trailer proves it was finished rather than left half-written, and
// NETSCAPE2.0 is what makes it loop instead of freezing on the last moment.
test("save_gif writes a played plot where SVG motion is not read", () => {
  const played = {
    x: [1, 2, 3, 10, 20, 30],
    y: [1, 2, 3, 10, 20, 30],
    year: [1957, 1957, 1957, 1962, 1962, 1962],
  };
  const moving = plot(data(played), point, x(col.x), y(col.y), play(col.year));
  const folder = fs.mkdtempSync(`${os.tmpdir()}/gog-gif-`);
  try {
    const written = save_gif(moving, `${folder}/wave.gif`);
    const raw = fs.readFileSync(written);
    assert.equal(raw.subarray(0, 6).toString("latin1"), "GIF89a");
    assert.equal(raw[raw.length - 1], 0x3b, "the GIF should end with its trailer");
    assert.ok(raw.includes(Buffer.from("NETSCAPE2.0")), "the GIF should loop");

    // A plot with no moments cannot become a sequence, and the refusal says what
    // to write instead rather than leaving a file nobody asked for.
    refuses(
      () => save_gif(plot(data(played), point, x(col.x), y(col.y)), `${folder}/still.gif`),
      /does not play, so it has no frames to write[\s\S]*play\(year\)/,
    );
    // The name says what the file is, so a path that says otherwise is refused
    // rather than quietly corrected.
    refuses(() => save_gif(moving, `${folder}/wave.png`), /ends in `\.gif`/);

    // The correction keeps the directory that was asked for. R dropped it here
    // and the other three did not, so a reader was told to write into the
    // working directory while looking for the file somewhere else. The refusal
    // is worth nothing if the path it hands back is a different path.
    assert.throws(() => save_gif(moving, `${folder}/wave.png`), (error) => {
      assert.ok(error.message.includes(`${folder}/wave.gif`), error.message);
      return true;
    });
  } finally {
    fs.rmSync(folder, { recursive: true, force: true });
  }
});

test("every mark the kernel has draws", () => {
  const sentences = [
    plot(data(bars), bar, x(col.category), y(col.value)),
    plot(data(df), line, x(col.x), y(col.y)),
    plot(data(df), path, x(col.x), y(col.y)),
    plot(data(df), area, x(col.x), y(col.y)),
    plot(data(df), box, x(col.group), y(col.y)),
    plot(data(df), box("range"), x(col.group), y(col.y)),
    plot(data(df), layer(bar, count), x(col.group)),
    plot(data(df), point, x(col.x), y(col.y), title("A title"), x_label("An axis")),
    // `color` bound because a palette with nothing to color is now its own
    // refusal, and this list is asking whether a palette *draws*.
    plot(data(df), point, x(col.x), y(col.y), color(col.group), palette("okabe")),
    plot(data(bars), bar, x(col.category), y(col.value), polar()),
    plot(data(df), point, x(col.x), y(col.y), z(col.y), space()),
  ];
  for (const sentence of sentences) {
    assert.match(render_svg(sentence), /^<svg /);
  }
});

test("a viewing angle is a number and a label is a string, refused where typed", () => {
  refuses(() => space("left"), /number of degrees/);
  refuses(() => polar("top"), /number of degrees/);
  refuses(() => x_label(42), /needs a string/);
  refuses(() => title(42), /needs a string/);
});

// An elevation has ends and a bearing does not, and the pair is the test. The drag
// has clamped tilt to ±90 since 2026-08-06, so a reader turning the cube could not
// reach a turned-over view while a reader *writing* one could: `space(45, -400)`
// drew upside-down nonsense without a word. `turn` must stay silent, because a
// bearing genuinely wraps — refusing both alike teaches a cap that does not exist.
test("tilt has ends, turn wraps, and equal bearings draw alike", () => {
  const cube = { x: [1, 2, 3], y: [2, 1, 3], z: [3, 2, 1] };
  const turned = (opts) =>
    plot(data(cube), point, x(col.x), y(col.y), z(col.z), space(opts));

  for (const tilt of [95, 180, -400]) {
    refuses(() => render_svg(turned({ tilt })), /-90 to 90/);
    // The refusal must offer the bearing, or it reads as a cap on both angles.
    refuses(() => render_svg(turned({ tilt })), /space\(turn = \)/);
  }
  for (const tilt of [90, -90, 0, 25]) {
    assert.match(render_svg(turned({ tilt })), /^<svg /, `tilt ${tilt} is in range`);
  }
  // Equal bearings draw the same bytes, not merely a similar picture. `turn: -360`
  // used to lose two of eighteen tick labels, with every mark in place.
  const canonical = render_svg(turned({ turn: 30 }));
  for (const turn of [390, 750, -330, -690]) {
    assert.equal(render_svg(turned({ turn })), canonical, `turn ${turn} is turn 30`);
  }
});

// A fit needs rows, and it needs them in the cell the fit runs in. The engine had
// the minimum already — below three rows the transform returns the frame unchanged
// — but no gate, so the raw rows reached the page *as* the fitted curve. The split
// is the half that hid: six rows pass any whole-frame count while all three of
// their groups fail, so the picture drew three two-point polylines beside
// hundred-point ones with nothing to say which was a fit.
test("smooth needs three rows in every cell it fits", () => {
  // `layer(line, smooth)` is JavaScript's `line * smooth`: the mark and its
  // transform are one block, and this binding spells the operator as a word.
  const fit = (d, ...rest) =>
    render_svg(plot(data(d), layer(line, smooth), x(col.x), y(col.y), ...rest));

  for (const n of [1, 2]) {
    const rows = [1, 2].slice(0, n);
    refuses(() => fit({ x: rows, y: rows }), /at least 3/);
    refuses(() => fit({ x: rows, y: rows }), new RegExp(`has ${n}\\.`));
  }
  assert.match(fit({ x: [1, 2, 3], y: [1, 2, 3] }), /^<svg /, "three rows is a fit");

  const split = { g: ["a", "a", "b", "b", "c", "c"],
                  x: [1, 2, 1, 2, 1, 2], y: [1, 2, 2, 1, 1, 3] };
  refuses(() => fit(split, group(col.g)), /at least 3/);
  refuses(() => fit(split, group(col.g)), /Drop the split/);
  refuses(() => fit(split, group(col.g)), /`g`/);
});

test("a missing value is dropped and reported, never dropped in silence", () => {
  assert.match(render_svg(plot(data(gaps), point, x(col.a), y(col.b))), /^<svg /);
});

test("a mixed column is refused where the caller can still see which one", () => {
  refuses(
    () => render_svg(plot(data({ a: [1, "two", 3] }), point, x(col.a), y(col.a))),
    /mixes .* — a column is one type/
  );
});

test("a declared category order survives the trip", () => {
  const table = {
    size: ordered(["Low", "High", "Mid"], ["Low", "Mid", "High"]),
    value: [1, 3, 2],
  };
  const p = plot(data(table), bar, x(col.size), y(col.value));
  assert.match(render_svg(p), /^<svg /);
});

test("dates cross as time, and the unit is read off the values", () => {
  const days = {
    when: [new Date("2020-01-01"), new Date("2020-02-01"), new Date("2020-03-01")],
    value: [1, 2, 3],
  };
  const seconds = {
    when: [new Date("2020-01-01T09:30:00Z"), new Date("2020-01-01T10:45:00Z")],
    value: [1, 2],
  };
  assert.match(render_svg(plot(data(days), line, x(col.when), y(col.value))), /^<svg /);
  assert.match(render_svg(plot(data(seconds), line, x(col.when), y(col.value))), /^<svg /);
});

test("the engine's refusals arrive with the engine's own words", () => {
  // A legality refusal belongs to `legality.rs`, not to this binding: every
  // binding must get the same one, which is what makes the rule the engine's.
  refuses(() => render_svg(plot(data(df), point, x(col.x))), /gog:/);
  refuses(() => render_svg(plot(data(df), point, x(col.nope), y(col.y))), /nope/);
});

test("style() needs a mark to style", () => {
  refuses(() => plot(data(df), style({ color: "tomato" })), /has no mark to style/);
});

test("theme() is the page, style() is the ink", () => {
  const bars2 = { g: ["Alpha", "Beta", "Gamma"], v: [3, 7, 5],
                  side: ["Left", "Right", "Left"] };
  const lines = (t) =>
    (render_svg(plot(data(bars2), bar, x(col.g), y(col.v), t)).match(/<line/g) || []).length;

  assert.ok(lines(theme({ grid: "none" })) < lines(style({ opacity: 1 })),
    "grid: none drops the gridlines");
  assert.equal(lines(theme("minimal")), lines(theme({ grid: "none" })),
    "the preset resolves in the engine, not here");

  // A preset a caller cannot adjust sends them back to knobs.
  const square = render_svg(plot(data(bars2), bar, x(col.g), y(col.v), theme("minimal", { ratio: 1 })));
  const wide = render_svg(plot(data(bars2), bar, x(col.g), y(col.v), theme("minimal")));
  assert.notEqual(square, wide, "a preset can be adjusted");

  assert.match(
    render_svg(plot(data(bars2), bar, x(col.g), y(col.v), theme({ tick_angle: 45 }))),
    /rotate/
  );

  // One number, three sizes: the ticks take the number and the axis names and
  // the title are a fixed step above it, so a plot's text is one decision.
  const fontSizes = (svg) =>
    [...new Set([...svg.matchAll(/font-size="([0-9.]+)"/g)].map((m) => Number(m[1])))]
      .sort((a, b) => b - a);
  const typed = (...atoms) =>
    render_svg(plot(data(bars2), bar, x(col.g), y(col.v), title("T"), ...atoms));

  assert.deepEqual(fontSizes(typed()), [16, 13, 11]);
  assert.deepEqual(fontSizes(typed(theme({ font_size: 16 }))), [23, 19, 16],
    "font_size must carry the axis names and the title with it");
  // Asking for the size you already have must draw the plot you already had, or
  // the default is an approximation of the scale rather than a point on it.
  assert.equal(typed(theme({ font_size: 11 })), typed(),
    "font_size: 11 must be the untouched default");

  refuses(() => render_svg(plot(data(bars2), bar, x(col.g), theme("dark"))), /is not a theme/);
  refuses(() => theme({ grid: "diag" }), /is one of/);
  refuses(() => theme({ ratio: -1 }), /positive number/);
  refuses(() => theme({ tick_angle: 120 }), /-90 and 90/);
  refuses(() => theme(), /sets nothing/);
  refuses(() => theme({ grd: "none" }), /has no `grd`/);
  refuses(() => theme({ frame: "box" }), /is one of/);

  // The preset rule, faceted on purpose: it passed for the whole life of
  // `theme("bw")` while the preset left gray strips over its white panels,
  // because an unfaceted plot draws no strip to miss.
  const bwNamed = render_svg(
    plot(data(bars2), bar, x(col.g), y(col.v), theme("bw"), across(col.side)));
  assert.equal(bwNamed, render_svg(
    plot(data(bars2), bar, x(col.g), y(col.v),
         theme({ background: "white", frame: "full", strip: "white" }), across(col.side))));
  assert.ok(!bwNamed.includes("#e4e4ec"), 'theme("bw") must not leave the strip gray');
  assert.ok(render_svg(plot(data(bars2), bar, x(col.g), y(col.v), across(col.side)))
    .includes("#e4e4ec"), "the default strip must not move");
  assert.ok(render_svg(
    plot(data(bars2), bar, x(col.g), y(col.v), theme({ strip: "seagreen" }), across(col.side)))
    .includes("seagreen"), "theme({ strip }) must reach the band");
  refuses(() => render_svg(
    plot(data(bars2), bar, x(col.g), y(col.v), theme({ strip: "whte" }), across(col.side))),
    /is not a color/);

  // The ink derives from the band, so `strip: "black"` is a whole instruction:
  // without it the near-black label would sit on the near-black band.
  assert.ok(render_svg(plot(data(bars2), bar, x(col.g), y(col.v),
    theme({ strip: "black" }), across(col.side))).includes('fill="#ffffff" text-anchor="middle"'),
    "a dark strip must get light type without being asked");
  assert.ok(render_svg(plot(data(bars2), bar, x(col.g), y(col.v), across(col.side)))
    .includes('fill="#3c3c46" text-anchor="middle"'), "the default strip ink must not move");
  assert.ok(render_svg(plot(data(bars2), bar, x(col.g), y(col.v),
    theme({ strip: "navy", strip_text: "gold" }), across(col.side))).includes("gold"),
    "a named ink must win over the derived one");
  refuses(() => render_svg(plot(data(bars2), bar, x(col.g), y(col.v),
    theme({ strip_text: "gld" }), across(col.side))), /is not a color/);
  // The mistake the pixel unit invites: reading the number as a multiplier.
  refuses(() => theme({ font_size: 1.5 }), /not a\s+multiplier/);

  // A preset is only a bundle of properties a caller could set themselves.
  assert.equal(
    render_svg(plot(data(bars2), bar, x(col.g), y(col.v), theme("bw"))),
    render_svg(plot(data(bars2), bar, x(col.g), y(col.v),
                    theme({ background: "white", frame: "full" }))),
    "theme('bw') is its own properties spelled out"
  );
  // The furniture goes black and white; the data does not.
  assert.match(
    render_svg(plot(data(bars2), bar, x(col.g), y(col.v), color(col.g), theme("bw"))),
    /#/
  );
});

test("the zone and span marks are reachable with their own words", () => {
  const band = { x: [1, 2, 3], lo: [1, 2, 1], hi: [3, 4, 3] };
  assert.match(
    render_svg(plot(data(band), layer(ribbon, bounds(col.lo, col.hi)), x(col.x))),
    /^<svg /
  );
  assert.match(
    render_svg(plot(data(band), layer(interval, bounds(col.lo, col.hi)), x(col.x))),
    /^<svg /
  );
  assert.match(render_svg(plot(data(df), layer(zone, bin), x(col.x), y(col.y))), /^<svg /);
});

test("limits state the domain when the data is not the authority", () => {
  const hrs = { hour: [1, 4, 7, 10, 13, 16, 19, 22], n: [2, 5, 9, 14, 20, 15, 8, 3] };

  // The forcing case: a periodic axis cannot tell that a variable is periodic,
  // so the period is stated — and a stated end is flush, or the circle would
  // not close on it.
  assert.match(
    render_svg(plot(data(hrs), line, x(col.hour, { limits: [0, 24] }), y(col.n), polar)),
    />0<\/text>/,
    "a stated cycle should reach its start"
  );

  // Restricting is the instruction, so it draws and reports rather than
  // refusing — the one place this parts from `scale: "log"` at zero.
  assert.match(
    render_svg(plot(data(hrs), point, x(col.hour, { limits: [0, 10] }), y(col.n))),
    /<circle/,
    "a restricted plot should still draw"
  );

  // A domain that keeps no row is the empty panel, and that is fatal.
  refuses(
    () => render_svg(plot(data(hrs), point, x(col.hour, { limits: [100, 200] }), y(col.n))),
    /leaves no rows at all/
  );

  // `limits` reaches every channel that measures, not only the axes (Law 1).
  assert.notEqual(
    render_svg(plot(data(hrs), point, x(col.hour), y(col.n), color(col.n, { limits: [0, 100] }))),
    render_svg(plot(data(hrs), point, x(col.hour), y(col.n), color(col.n, { limits: [0, 200] }))),
    "a stated domain should change the color ramp"
  );

  // A category has no range to lie inside; the refusal points at `order`.
  refuses(
    () => render_svg(plot(data({ g: ["a", "b"], v: [1, 2] }), bar,
                          x(col.g, { limits: [0, 5] }), y(col.v))),
    /order\(g\)/
  );

  // Caught in the binding, at the line that wrote it.
  refuses(() => x(col.hour, { limits: [20, 5] }), /runs backwards/);
  refuses(() => x(col.hour, { limits: [5] }), /needs two numbers/);
});

test("the named ramps render as themselves, and limits center a diverging one", () => {
  // The ruling: a diverging ramp has no midpoint parameter, because the middle
  // of a stated domain already is one. The data is one-sided (0..40), which is
  // what makes the two readings differ.
  const signed = { a: [1, 2, 3, 4, 5], b: [1, 2, 3, 4, 5], d: [0, 10, 20, 30, 40] };
  const fills = (svg) =>
    new Set([...svg.matchAll(/<circle[^>]*fill="([^"]*)"/g)].map((m) => m[1]));

  for (const [name, dark] of [["magma", "#000004"], ["inferno", "#000004"],
                              ["plasma", "#0d0887"], ["cividis", "#00204d"],
                              ["gray", "#a9a9a9"]]) {
    const drawn = fills(render_svg(
      plot(data(signed), point, x(col.a), y(col.b), color(col.d), palette(name))));
    assert.ok(drawn.has(dark), `palette("${name}") did not reach the output`);
    assert.ok(!drawn.has("#8faed5"), `palette("${name}") fell back to the blue ramp`);
  }

  for (const name of ["blue_red", "brown_teal"]) {
    const drawn = fills(render_svg(
      plot(data(signed), point, x(col.a), y(col.b),
           color(col.d, { limits: [-40, 40] }), palette(name))));
    assert.ok(drawn.has("#a9a9a9"), `${name} put nothing on the neutral at zero`);
    assert.ok(!drawn.has("#004383") && !drawn.has("#6b3d10"),
              `${name} reached its low end on data that never goes negative`);
  }

  const fitted = fills(render_svg(
    plot(data(signed), point, x(col.a), y(col.b), color(col.d), palette("blue_red"))));
  assert.ok(fitted.has("#004383"),
            "an unstated domain should fit the ramp to the data, low end included");

  // `gray` is in the vocabulary and `grey` is not — the American-English rule
  // enforced at the door rather than merely obeyed inside it.
  refuses(
    () => render_svg(plot(data(signed), point, x(col.a), y(col.b),
                          color(col.d), palette("grey"))),
    /`gray`/
  );

  // `soft` is the muted categorical set, and it reaches a *fill* — which is the
  // geometry it exists for, so testing it on a point would miss the point.
  const cats = { g: ["a", "b", "a", "c"], v: [1, 2, 3, 4] };
  const bars = render_svg(
    plot(data(cats), layer(bar, count), x(col.g), color(col.g), palette("soft")));
  assert.ok(bars.includes("#66c2a5"), "palette('soft') did not reach the bars");
  assert.ok(!bars.includes("#4e79a7"), "palette('soft') fell back to the default");
  // `shape` measures nothing, so a domain on it is refused: every channel takes
  // the argument and the engine refuses it, in the words all four bindings print.
  refuses(() => render_svg(plot(data({ g: ["a", "b"], v: [1, 2] }), point, x(col.v), y(col.v),
    shape(col.g, { limits: [0, 1] }))), /`limits` on `shape\(g\)`/);

  // A domain on a temporal axis is written in dates, and the binding converts
  // them the way it converts the column — otherwise the two disagree and every
  // row falls outside.
  const days = Array.from({ length: 20 }, (_, i) => new Date(Date.UTC(2024, 2, i + 1)));
  const year = render_svg(plot(data({ day: days, orders: days.map((_, i) => 20 + i) }),
    line, y(col.orders),
    x(col.day, { limits: [new Date(Date.UTC(2024, 0, 1)), new Date(Date.UTC(2024, 11, 31))] })));
  assert.match(year, />Jan 2024<\/text>/);
  assert.match(year, />Nov 2024<\/text>/);

  // A width has no such conversion: a plain number does not say its unit, and
  // the wire is seconds, so a width of 7 on dates cut 7-second bins.
  const dated = data({ day: days, orders: days.map((_, i) => 20 + i) });
  refuses(() => render_svg(plot(dated, layer(bar, bin({ width: 7 })), x(col.day))),
    /does not say what unit[\s\S]*bin\(30\)/);
  assert.match(render_svg(plot(dated, layer(bar, bin(6)), x(col.day))), /<rect/);
});

// ---------------------------------------------------------------------------
// A color bound to a level by name, and a legend turned off (spec §10)
// ---------------------------------------------------------------------------

// The column meets its levels Asia, Europe, Africa; the names are written in
// another order, which is the reason to write them at all.
const lvl = {
  a: [1, 2, 3, 4, 5, 6],
  b: [3, 1, 4, 1, 5, 9],
  g: ["Asia", "Europe", "Africa", "Asia", "Europe", "Africa"],
};

// The last color written before a legend row's name is that row's swatch.
function swatch(svg, level) {
  const before = svg.slice(0, svg.indexOf(`>${level}</text>`));
  return [...before.matchAll(/fill="([^"]+)"/g)].at(-1)[1];
}

test("palette({ Asia: … }) gives each level the color beside its name", () => {
  const named = palette({ Africa: "seagreen", Europe: "steelblue", Asia: "tomato" });
  assert.deepEqual(named.fields.value,
    { levels: { Africa: "seagreen", Europe: "steelblue", Asia: "tomato" } });
  // A Map reads the same way; `JSON.stringify` would otherwise send it as `{}`.
  assert.deepEqual(palette(new Map([["Asia", "tomato"]])).fields.value,
    { levels: { Asia: "tomato" } });

  const svg = render_svg(plot(data(lvl), point, x(col.a), y(col.b), color(col.g), named));
  for (const [level, want] of [["Asia", "tomato"], ["Europe", "steelblue"], ["Africa", "seagreen"]]) {
    assert.equal(swatch(svg, level), want, level);
  }
  assert.ok(svg.indexOf(">Asia</text>") < svg.indexOf(">Africa</text>"),
    "the key should run in the column's order");
});

test("a named palette refuses a name that is no level, and a level with no name", () => {
  const draw = (pal, bind = color(col.g)) =>
    () => render_svg(plot(data(lvl), point, x(col.a), y(col.b), bind, pal));
  refuses(draw(palette({ Africa: "seagreen", Europe: "steelblue", Asai: "tomato" })),
    /names "Asai", and `g` has no level called that.*Did you mean "Asia"\?.*"Asia", "Europe", and "Africa"/s);
  refuses(draw(palette({ Asia: "tomato", Europe: "steelblue" })), /leaves out "Africa"/);
  refuses(draw(palette({ Asia: "tomato" }), color(col.b)), /numbers have no levels to name/);
  refuses(() => palette({ Asia: 3 }), /both written as text/);
});

test("legend: false leaves the key out, keeps the mapping, and says nothing", () => {
  const keyed = render_svg(plot(data(lvl), point, x(col.a), y(col.b), color(col.g)));
  const write = process.stderr.write;
  let said = "";
  process.stderr.write = (chunk) => { said += chunk; return true; };
  let off;
  try {
    off = render_svg(plot(data(lvl), point, x(col.a), y(col.b), color(col.g, { legend: false })));
  } finally {
    process.stderr.write = write;
  }
  assert.equal(said, "", "legend: false should print nothing");
  assert.ok(keyed.includes(">Asia</text>") && !off.includes(">Asia</text>"),
    "legend: false drew the key");
  assert.ok(off.includes("#4e79a7") && off.includes("#f28e2b"),
    "legend: false must not drop the mapping");
  for (const atom of [size(col.a, { legend: false }), opacity(col.a, { legend: false }),
                      shape(col.g, { legend: false })]) {
    const svg = render_svg(plot(data(lvl), point, x(col.a), y(col.b), atom));
    assert.ok(!svg.includes('rx="4"'), `${atom.kind}({ legend: false }) drew a key`);
  }
  const hatched = render_svg(plot(data(lvl), layer(bar, count), x(col.g),
    pattern(col.g, { legend: false })));
  assert.ok(!hatched.includes('rx="4"'), "pattern({ legend: false }) drew a key");
});

test("legend on a channel with no key is refused with direction", () => {
  refuses(() => render_svg(plot(data(lvl), point, x(col.a, { legend: false }), y(col.b))),
    /`x\(a\)` is given `legend` — `x` is read off its axis/);
  refuses(() => render_svg(plot(data(lvl), line, x(col.a), y(col.b),
    group(col.g, { legend: false }))), /`group` splits the rows without encoding anything/);
  refuses(() => render_svg(plot(data(lvl), text, x(col.a), y(col.b),
    label(col.g, { legend: false }))), /`label` is the text a `text` mark writes/);
  refuses(() => color(col.g, { legend: "no" }), /true or false/);
});

// ---------------------------------------------------------------------------
// tick_count — how many ticks an axis aims for (spec §10)
//
// The last property that was real in the IR, read by the renderer, and reachable
// from no binding. It rides the binding beside `scale` and `limits` because it
// describes the **scale**; `theme()` declined it on that ground (§7).
// ---------------------------------------------------------------------------

test("tick_count states how many ticks an axis aims for", () => {
  const g5 = { a: [0, 25, 50, 75, 100], b: [1, 2, 3, 4, 5] };
  const ticks = (p) => (render_svg(p).match(/>[-0-9.]+<\/text>/g) ?? []);

  // A target rather than a promise: the count picks a step and the step is then
  // rounded to a human number, so the claim is monotone rather than exact.
  const few = ticks(plot(data(g5), point, x(col.a, { tick_count: 3 }), y(col.b)));
  const many = ticks(plot(data(g5), point, x(col.a, { tick_count: 11 }), y(col.b)));
  assert.ok(many.length > few.length,
    `tick_count changed nothing: ${few.length} vs ${many.length}`);

  // Thinning the labels is not coarsening the step: a sparse axis's ticks are a
  // subset of a dense one's, so a value read off either is on the same scale.
  const dense = new Set(many);
  assert.ok(few.every((t) => dense.has(t)),
    `a sparse axis invented labels: ${few.filter((t) => !dense.has(t))}`);

  // A legend is not a short axis: `limits` reaches all six magnitude channels,
  // `tick_count` only the three that draw an axis, and the engine says so.
  refuses(() => render_svg(plot(data({ a: [1, 2] }), point, x(col.a), y(col.a),
    color(col.a, { tick_count: 4 }))), /`tick_count` on `color\(a\)`/);
  // Every channel takes all seven parameters and forwards them, so each lands on
  // the engine's refusal rather than on "has no `speed`".
  const two = { g: ["a", "b"], v: [1, 2] };
  refuses(() => render_svg(plot(data(two), point, x(col.v), y(col.v), color(col.g, { speed: 2 }))),
    /`speed` on `color\(g\)`/);
  refuses(() => render_svg(plot(data(two), point, x(col.v), y(col.v), shape(col.g, { scale: "log" }))),
    /`scale` on `shape\(g\)`/);

  // Caught in the binding, at the line that wrote it.
  refuses(() => x(col.a, { tick_count: 1 }), /at least two ticks/);
  refuses(() => x(col.a, { tick_count: 2.5 }), /not a whole number/);
  refuses(() => x(col.a, { tick_count: "8" }), /needs one number/);

  // A category axis has one tick per level, so the count is the data's.
  refuses(
    () => render_svg(plot(data({ g: ["a", "b"], v: [1, 2] }), bar,
                          x(col.g, { tick_count: 5 }), y(col.v))),
    /order\(g\)/
  );

  // One axis, one count — a layer stating its own is the plot-scoped-scale rule.
  refuses(
    () => render_svg(plot(data(g5), x(col.a, { tick_count: 4 }), y(col.b),
                          point, x(col.a, { tick_count: 9 }))),
    /its own tick count/
  );
});

test("a count past the most one axis draws widens the step, and says so", () => {
  // The cut kept the first 26 ticks: 40 on gdp labeled 0K to 25K on an axis
  // that runs to 49K, and said nothing.
  const wide = { gdp: [277.55, 12000, 49357.19], life: [40, 60, 82] };
  const drawn = (n) => {
    const write = process.stderr.write;
    let said = "";
    process.stderr.write = (chunk) => { said += chunk; return true; };
    try {
      const svg = render_svg(plot(data(wide), point, x(col.gdp, { tick_count: n }), y(col.life)));
      return { svg, said };
    } finally {
      process.stderr.write = write;
    }
  };
  const widened = drawn(40);
  const labels = [...widened.svg.matchAll(/>([^<>]*)<\/text>/g)].map((m) => m[1]);
  assert.ok(labels.includes("48K") && !labels.includes("25K"),
    `a count past the maximum should widen the step to the far end: ${labels}`);
  assert.match(widened.said, /a tick every 2K instead/);
  assert.doesNotMatch(drawn(26).said, /ticks on this axis/);
});

// ---------------------------------------------------------------------------
// surface — the sheet through the samples (spec §15)
//
// The engine tests pin the mesh against the lattice; these pin the *binding*:
// that `surface` is exported, that a grid table reaches the engine as a grid, and
// that the refusals a reader will actually hit arrive with direction.
// ---------------------------------------------------------------------------

/** One row per (x, y) crossing — the mark's whole contract with the caller. */
function grid(n) {
  const side = Array.from({ length: n }, (_, i) => -3 + (6 * i) / (n - 1));
  const gx = [], gy = [], h = [];
  for (const a of side) {
    for (const b of side) {
      const r = Math.sqrt(a * a + b * b) + 1e-9;
      gx.push(a); gy.push(b); h.push(Math.sin(r) / r);
    }
  }
  return { gx, gy, h };
}
const faces = (svg) => (svg.match(/<path d="M/g) || []).length;

test("a surface draws one face per complete cell of its grid", () => {
  const surf = grid(15);
  const sheet = render_svg(plot(data(surf), surface, x(col.gx), y(col.gy), z(col.h)));
  assert.equal(faces(sheet), 196, "a 15x15 grid of nodes is 14x14 faces");

  // Binding `z` is what puts a plot in the cube, so a surface needs no `space()` —
  // and `space()` still sets the angle, which must change the picture.
  assert.notEqual(
    sheet,
    render_svg(plot(data(surf), surface, x(col.gx), y(col.gy), z(col.h), space(110, 40)))
  );

  // The mesh lines: the seam hairline each face already carried, handed to the caller.
  assert.match(
    render_svg(plot(data(surf), surface, x(col.gx), y(col.gy), z(col.h),
      style({ border_color: "white", border_size: 0.6 }))),
    /stroke="white"/
  );

  // A mapped opacity fades the sheet face by face — `color`'s per-face reading, one
  // channel over. A mesh has parts small enough to each hold one value, so it can
  // hold this one, and the sheet then thins where its measure is small.
  const faded = render_svg(
    plot(data(surf), surface, x(col.gx), y(col.gy), z(col.h), opacity(col.h)));
  const fades = new Set(faded.match(/fill-opacity="[0-9.]+"/g) || []);
  assert.ok(fades.size >= 20,
    `a mapped opacity should fade face by face, got ${fades.size} values`);
});

test("a surface without the cube, and a scatter, are refused with direction", () => {
  const surf = grid(15);
  // One failure rather than two, and the direction names both routes in plus the
  // mark that draws the same field in the plane.
  refuses(
    () => render_svg(plot(data(surf), surface, x(col.gx), y(col.gy))),
    /needs the cube[\s\S]*`zone`/
  );

  // A scatter is the empty panel this refusal exists to prevent.
  const scat = {
    sx: Array.from({ length: 60 }, (_, i) => ((i * 37) % 101) / 101),
    sy: Array.from({ length: 60 }, (_, i) => ((i * 53) % 97) / 97),
    sh: Array.from({ length: 60 }, (_, i) => ((i * 29) % 89) / 89),
  };
  refuses(
    () => render_svg(plot(data(scat), surface, x(col.sx), y(col.sy), z(col.sh))),
    /scatter rather than a grid/
  );

  // And the sentence that refusal advises must draw: the field raised, no `z()`.
  const est = render_svg(plot(data(scat), layer(surface, density), x(col.sx), y(col.sy), space()));
  assert.ok(faces(est) > 100, "surface * density should raise a mesh");

  // What is still refused is a floor of *slots*: categories leave air between
  // them, and tiles that float apart are not a sheet.
  refuses(
    () => render_svg(plot(data(scat), layer(surface, count), x(col.sx), y(col.sy), space())),
    /surface \* bin/
  );

  // A face spans the gap between two samples; two categories have no gap to span.
  // Toward `bar * count`, not `bar * bin`: over two categorical positions the
  // transform that makes cells is the one that tallies into the slots they already
  // are. This read `/bar.*bin/` until 2026-07-28, pinning a direction that was
  // itself refused.
  const cats = { ...surf, band: surf.gx.map((_, i) => (i % 2 ? "low" : "high")) };
  refuses(
    () => render_svg(plot(data(cats), surface, x(col.band), y(col.gy), z(col.h))),
    /bar \* count/
  );
});

test("a cut floor lays one plateau per cell where a node floor spans the gaps", () => {
  // `bin` cuts the floor into adjacent cells and the sheet lays a flat lid on each —
  // the terraced surface, for a design that measures one value per cell. A 3x3 grid
  // read as *nodes* is 2x2 blocks of four corners, so four faces; read as cells it
  // is nine lids plus the twelve risers that connect them.
  const t = [-2, 0, 2];
  const terr = {
    ta: t.flatMap((a) => t.map(() => a)),
    tb: t.flatMap(() => t),
  };
  terr.tv = terr.ta.map((a, i) => a * a + terr.tb[i] * terr.tb[i]);

  const nodes = render_svg(plot(data(terr), surface, x(col.ta), y(col.tb), z(col.tv)));
  assert.equal(faces(nodes), 4, "nine nodes are four faces");

  const lids = render_svg(
    plot(data(terr), layer(surface, bin(3), mean), x(col.ta), y(col.tb), z(col.tv))
  );
  assert.equal(faces(lids), 21, "nine cells are 9 lids + 12 risers");
});

// ---------------------------------------------------------------------------
// Polar — every mark that draws flat draws bent (spec §15)
//
// Five marks were refused in this space until 2026-07-26 on one recorded ground,
// *their straight edges would have to become arcs*. Three never needed one. What
// each check pins is the property the refusal was really about: a segment that
// **holds** a value across a span must follow the ring, since a chord falls
// inside the circle and puts the mark where the data is not.
// ---------------------------------------------------------------------------

test("every mark that draws flat draws in polar", () => {
  const wind = {
    dir: ["N", "N", "N", "E", "E", "E", "S", "S", "S", "W", "W", "W"],
    spd: [4, 5, 6, 8, 9, 11, 6, 7, 5, 3, 4, 2],
    season: ["Summer", "Winter", "Summer", "Winter", "Summer", "Winter",
             "Summer", "Winter", "Summer", "Winter", "Summer", "Winter"],
  };
  const band = { dir: ["N", "E", "S", "W"], lo: [2, 6, 4, 1], hi: [6, 11, 8, 5] };

  for (const [name, p] of [
    ["step", plot(data(wind), layer(step, mean), x(col.dir), y(col.spd), polar())],
    ["interval", plot(data(wind), layer(interval, range), x(col.dir), y(col.spd), polar())],
    ["box", plot(data(wind), box, x(col.dir), y(col.spd), polar())],
    ["ribbon", plot(data(band), layer(ribbon, bounds(col.lo, col.hi)), x(col.dir), polar())],
    ["zone", plot(data(wind), layer(zone, count), x(col.dir), y(col.season), polar())],
  ]) {
    const svg = render_svg(p);
    assert.ok(svg.includes("<svg"), `${name} does not draw in polar`);
    assert.ok(!svg.includes("NaN"), `${name} wrote NaN coordinates`);
  }
});

test("a stair's treads become arcs, and a band's boundaries do not", () => {
  const wind = {
    dir: ["N", "N", "E", "E", "S", "S", "W", "W"],
    spd: [4, 6, 8, 11, 6, 5, 3, 2],
  };
  const band = { dir: ["N", "E", "S", "W"], lo: [2, 6, 4, 1], hi: [6, 11, 8, 5] };
  const arcs = (svg) => (svg.match(/ A /g) || []).length;

  // The one genuinely new segment: a tread holds its value across a span of
  // angle, so it follows the ring. Flat, the same mark draws no arc at all.
  assert.ok(arcs(render_svg(plot(data(wind), layer(step, mean), x(col.dir), y(col.spd), polar()))) > 0);
  assert.equal(arcs(render_svg(plot(data(wind), layer(step, mean), x(col.dir), y(col.spd)))), 0);

  // A band's two boundaries run through the data's own vertices, which is
  // `line`'s geometry — the correction this made to the recorded refusal.
  assert.equal(
    arcs(render_svg(plot(data(band), layer(ribbon, bounds(col.lo, col.hi)), x(col.dir), polar()))),
    0,
    "a radar band needed no arc and drew one"
  );
});

test("a hexagonal mesh has no polar reading, and a rectangular one does", () => {
  const mesh = {
    a: Array.from({ length: 36 }, (_, i) => i % 6),
    b: Array.from({ length: 36 }, (_, i) => Math.floor(i / 6)),
  };
  // `bin(tiling = )`'s third refusal: a plane is what a tiling partitions, and a
  // bent plane has no distance for a hexagon to be regular against.
  refuses(
    () => render_svg(plot(data(mesh), layer(zone, bin({ tiling: "hex" })), x(col.a), y(col.b), polar())),
    /rect/
  );
  const rect = render_svg(plot(data(mesh), layer(zone, bin({ tiling: "rect" })), x(col.a), y(col.b), polar()));
  assert.ok((rect.match(/ A /g) || []).length > 0, "a rectangular mesh should bend into sectors");
});

// ---------------------------------------------------------------------------
// Space — the three slot marks stand on the cube's floor (spec §15)
//
// `interval` and `box` joined `bar` in the cube on 2026-07-26 and needed no
// ruling of their own: `is_slot_mark` had grouped the three since orientation
// was decided. The cube's remaining blanks are the other half — four *decided*
// refusals and two blocked on occlusion, and until this change every one of
// them said "not drawn yet".
// ---------------------------------------------------------------------------

const cubePlots = {
  site: [...Array(20).fill("North"), ...Array(20).fill("Center"), ...Array(20).fill("South")],
  season: Array.from({ length: 60 }, (_, i) => (i % 2 ? "Dry" : "Wet")),
  yield: Array.from({ length: 60 }, (_, i) => 50 + (i % 11) + (i < 20 ? 0 : i < 40 ? 8 : -4)),
};

test("interval and box stand on the cube's floor", () => {
  for (const [name, p] of [
    ["interval", plot(data(cubePlots), layer(interval, range), x(col.site), y(col.season), z(col.yield), space())],
    ["conf", plot(data(cubePlots), layer(interval, confidence), x(col.site), y(col.season), z(col.yield), space())],
    ["box", plot(data(cubePlots), box, x(col.site), y(col.season), z(col.yield), space())],
  ]) {
    const svg = render_svg(p);
    assert.ok(svg.includes("<svg"), `${name} does not stand in the cube`);
    assert.ok(!svg.includes("NaN"), `${name} wrote NaN coordinates`);
  }

  // One per **cell**, not one per row: six cells, each a span plus a crossed cap
  // at either end — 6 x 5 = 30 strokes carrying a linecap.
  const svg = render_svg(
    plot(data(cubePlots), layer(interval, range), x(col.site), y(col.season), z(col.yield), space()));
  assert.equal((svg.match(/stroke-linecap/g) || []).length, 30);
});

test("the cube's blanks say which of three things they are", () => {
  // Decided: a cube has no left to right, so a `line` would be sorted by an axis
  // the reader cannot see. It must give the ruling, not promise a renderer.
  assert.throws(
    () => render_svg(plot(data(cubePlots), line, x(col.yield), y(col.yield), z(col.yield), space())),
    (e) => {
      assert.match(e.message, /no left to right/);
      assert.match(e.message, /path/);
      assert.ok(!/not drawn yet|does not draw it yet/.test(e.message),
        `a decided refusal must not promise a renderer: ${e.message}`);
      return true;
    }
  );
  // Blocked on occlusion: a plane has no footprint to sort by. A different
  // sentence, and an `Unsupported` rather than an `Illegal`.
  refuses(
    () => render_svg(plot(data(cubePlots), rule, z(col.yield), space())),
    /footprint/
  );
  // An Unsupported refusal says, in the shared words, that the grammar allows the
  // sentence and this engine does not draw it, and it promises no future.
  assert.throws(
    () => render_svg(plot(data(cubePlots), rule, z(col.yield), space())),
    (e) => /valid grammar, but this engine does not draw it/.test(e.message)
      && !/\byet\b|for now|wait for/.test(e.message)
  );
});

test("the composed cut: bin supplies the cells, a statistic measures them", () => {
  // Which transform owns the measurement when two are composed (spec §5).
  // `bin` says where the cells are *and* what is in them, and only the first is
  // what makes it a `bin` — so composed with a statistic it keeps the cut and
  // gives the tally up. The binned mean profile, and the summary heatmap one
  // dimension up.
  const cut = render_svg(plot(data(df), layer(bar, bin, mean), x(col.x), y(col.y)));
  assert.match(cut, /<svg/);

  // Order cannot decide anything here: a cell has to exist before anything can
  // be measured in it, so the cut is prior rather than merely earlier.
  assert.equal(cut, render_svg(plot(data(df), layer(bar, mean, bin), x(col.x), y(col.y))));

  // And the statistic has to reach the plot. Until 2026-07-26 it did not: `bin`
  // overwrote the named column with its own tally, the reduction handed that
  // straight back, and only the axis *title* changed — a histogram under an axis
  // reading `Life`. Geometry, not text, is what settles it.
  const strip = (s) => s.replace(/<text[^<]*<\/text>/g, "");
  assert.notEqual(strip(cut), strip(render_svg(plot(data(df), layer(bar, bin), x(col.x)))),
    "the composed statistic must change the geometry, not just the label");
});

test("the other two synthesizing transforms cannot compose", () => {
  // `count` tallies into the cells the positions already own, so taking its
  // measurement away leaves nothing of it.
  refuses(
    () => render_svg(plot(data(df), layer(bar, count, mean), x(col.group), y(col.y))),
    /measures each cell twice/
  );
  // A `density` cell is a sample point of an estimate rather than a bucket
  // holding rows, so there is nothing inside one to reduce — its own reason.
  refuses(
    () => render_svg(plot(data(df), layer(bar, density, mean), x(col.x), y(col.y))),
    /not a bucket holding rows/
  );
  // Two synthesizing transforms: neither was handed a column, so neither can give
  // way. `proportion` left this class on 2026-07-26 — it rescales a measurement
  // rather than inventing one — so the pair here is `bin * count`.
  refuses(
    () => render_svg(plot(data(df), layer(bar, bin, count), x(col.x))),
    /neither was handed a column/
  );
  // `smooth` is refused against all four, `bin` included, for a reason none of
  // them share: it already averages locally as it fits.
  refuses(
    () => render_svg(plot(data(df), layer(bar, bin, smooth), x(col.x), y(col.y))),
    /asks one question twice/
  );
});

// ---------------------------------------------------------------------------
// `proportion` is a normalizer, and `stack({share})` fills a pile (spec §5)
// ---------------------------------------------------------------------------

// Read the drawn heights back as data values through the axis's own two ticks.
// Comparing the bars *with each other* is the point: the defect behind this
// session was twelve equal bars at 1/12, and the check that missed it read only
// the axis range. A range is not a shape.
function barValues(svg) {
  const ticks = [...svg.matchAll(/<text x="([0-9.]+)" y="([0-9.]+)">([0-9.]+)<\/text>/g)]
    .map((m) => [m[1], Number(m[2]), Number(m[3])]);
  // The y ticks share an x; the x ticks share a y. Take the commonest x rather
  // than a pixel threshold, which a short x label slips under.
  const tally = new Map();
  for (const [tx] of ticks) tally.set(tx, (tally.get(tx) ?? 0) + 1);
  const axis = [...tally.entries()].sort((a, b) => b[1] - a[1])[0][0];
  const on = ticks.filter(([tx]) => tx === axis);
  const perPx = (on[1][2] - on[0][2]) / (on[0][1] - on[1][1]);
  return [...svg.matchAll(/<rect[^>]*height="([0-9.]+)"[^>]*fill-opacity/g)]
    .map((m) => Number(m[1]))
    .filter((h) => h !== 12)          // drop legend swatches
    .map((h) => h * perPx);
}

const share = {
  dir: [...Array(6).fill("N"), ...Array(10).fill("E"), ...Array(4).fill("S"), ...Array(20).fill("W")],
  // Uneven inside each slot as well as between them: an alternating split makes
  // every slot 50/50, which a fill that ignored the values would also draw.
  season: [
    ...Array(4).fill("Su"), ...Array(2).fill("Wi"),
    ...Array(3).fill("Su"), ...Array(7).fill("Wi"),
    ...Array(1).fill("Su"), ...Array(3).fill("Wi"),
    ...Array(15).fill("Su"), ...Array(5).fill("Wi"),
  ],
  v: Array.from({ length: 40 }, (_, i) => i + 1),
};
// Skewed on purpose: a uniform column binned evenly gives near-equal bars, the
// one shape this test must be able to tell apart from the 1/12 defect.
const skew = { v: Array.from({ length: 200 }, (_, i) => Math.round(Math.exp((i * 4.6) / 199))) };
const total = (xs) => xs.reduce((a, b) => a + b, 0);

test("`proportion` normalizes over the whole frame, split or not", () => {
  const plain = total(barValues(render_svg(plot(data(share), layer(bar, proportion), x(col.dir)))));
  assert.ok(Math.abs(plain - 1) < 0.01, `bare proportion summed to ${plain}`);
  // The fix. A `color` split used to give each group its own denominator, so the
  // plot summed to 2 — two conditional distributions, where §5 had always said
  // the word means a share of the whole frame (Law 6).
  const split = total(barValues(render_svg(
    plot(data(share), layer(bar, proportion), x(col.dir), color(col.season)))));
  assert.ok(Math.abs(split - 1) < 0.01, `a split proportion summed to ${split}`);
});

test("the relative-frequency histogram is the histogram's counts over n", () => {
  // Refused for one day as two synthesizing transforms. The bars must *differ* —
  // all-equal is the 1/12 defect itself.
  const h = barValues(render_svg(plot(data(skew), layer(bar, bin(12), proportion), x(col.v))));
  assert.equal(h.length, 12);
  assert.ok(Math.abs(total(h) - 1) < 0.01, `shares summed to ${total(h)}`);
  assert.ok(new Set(h.map((v) => v.toFixed(3))).size > 1, "twelve equal bars — the 1/12 defect is back");
  const n = barValues(render_svg(plot(data(skew), layer(bar, bin(12)), x(col.v))));
  const nt = total(n);
  assert.ok(h.every((s, i) => Math.abs(n[i] / nt - s) < 0.01));
});

test("`stack({ share: true })` fills every pile to 1, on any measurement", () => {
  const tops = barValues(render_svg(plot(data(share), layer(bar, count, stack({ share: true })),
    x(col.dir), color(col.season))));
  const half = tops.length / 2;
  for (let i = 0; i < half; i += 1) {
    assert.ok(Math.abs(tops[i] + tops[i + half] - 1) < 0.01,
      `a filled pile reached ${tops[i] + tops[i + half]}`);
  }
  assert.ok(new Set(tops.map((v) => v.toFixed(3))).size > 1, "the fill lost the composition");
  // It composes with any measurement, which is why it is a `stack` parameter and
  // not a second reading of `proportion`: there is no column for `proportion` to sum.
  render_svg(plot(data(share), layer(bar, sum, stack({ share: true })),
    x(col.dir), y(col.v), color(col.season)));
  refuses(() => stack({ share: 1 }), /is true or false/);
});

test("`stack({ baseline })` hangs the pile, and a displaced axis draws no numbers", () => {
  // The streamgraph. A displaced pile draws no numbers on the measure axis: no
  // value on it corresponds to a measurement once the foot has moved.
  const flows = data({
    t: [1, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5, 6],
    g: ["a", "a", "a", "a", "a", "a", "b", "b", "b", "b", "b", "b",
      "c", "c", "c", "c", "c", "c"],
    v: [4, 9, 3, 8, 2, 7, 5, 5, 5, 5, 5, 5, 2, 3, 9, 2, 8, 3],
  }, { name: "flows" });
  const draw = (t) => render_svg(plot(flows, layer(area, t), x(col.t), y(col.v), color(col.g)));
  const ticks = (s) => (s.match(/>[^<>]+<\/text>/g) || [])
    .map((t) => t.slice(1, -7)).filter((t) => /^-?[0-9.]+$/.test(t));
  const plain = draw(stack);
  const strm = draw(stack({ baseline: "wiggle" }));
  assert.ok(ticks(plain).length > ticks(strm).length,
    "a displaced pile should drop its measure-axis numbers");
  assert.ok(ticks(strm).length > 0, "the domain axis lost its numbers too");
  // Displacing moves the pile; it never changes a thickness, so the band count holds.
  assert.equal((plain.match(/<polygon/g) || []).length,
    (strm.match(/<polygon/g) || []).length);
  refuses(() => stack({ baseline: 1 }), /is one of/);
  refuses(() => draw(stack({ baseline: "sym" })), /is not a baseline/);
  refuses(() => render_svg(plot(flows, layer(area, stack({ baseline: "center" })),
    x(col.t), y(col.v), color(col.g), polar())), /no origin to spare/);
});

test("a composed `proportion` still checks the column it rescales", () => {
  // It synthesizes nothing when something else measured, so its `y` names an
  // input column. Found by a reader looking at a plot: `bar * sum * proportion +
  // y(pop)` — `pop` renamed `population` in the book's own data — drew an empty
  // panel on fabricated 0..1 axes.
  refuses(
    () => render_svg(plot(data(share), layer(bar, sum, proportion), x(col.dir), y(col.nosuchcolumn))),
    /not in the data/
  );
  // …while a bare `proportion` still names the column it writes.
  render_svg(plot(data(share), layer(bar, proportion), x(col.dir), y(col.whatever)));
});

// --- the violin: the slot reading of `density` (spec §5) ---------------------
//
// Not a new mark, and the test says so by drawing it with the two that already
// exist: `ribbon` closes on its own reflection, `area` on the slot's center line.
const viol = {
  grp: [...Array(40).fill("wide"), ...Array(10).fill("narrow")],
  v: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9].flatMap((n) => [n, n, n, n, n]),
};
const npolys = (spec) => (render_svg(spec).match(/<polygon/g) || []).length;

test("`ribbon * density` and `area * density` over a category draw violins", () => {
  assert.equal(npolys(plot(data(viol), layer(ribbon, density), x(col.grp), y(col.v))), 2);
  assert.equal(npolys(plot(data(viol), layer(area, density), x(col.grp), y(col.v))), 2);
  // Lying down, the orientation read off the bindings — the form with room for
  // long category names, exactly as `box + x(pay) + y(dept)` is.
  assert.equal(npolys(plot(data(viol), layer(ribbon, density), x(col.v), y(col.grp))), 2);
});

test("`density({ compare })` reads only in the violin, and by name", () => {
  const counted = render_svg(plot(data(viol), layer(ribbon, density), x(col.grp), y(col.v)));
  const shaped = render_svg(
    plot(data(viol), layer(ribbon, density({ compare: "shape" })), x(col.grp), y(col.v))
  );
  assert.notEqual(counted, shaped, "`compare` had no effect on the plot");
  refuses(
    () => render_svg(plot(data(viol), layer(line, density({ compare: "count" })), x(col.v))),
    /no slots/
  );
  refuses(
    () => render_svg(plot(data(viol), layer(ribbon, density({ compare: "area" })), x(col.grp), y(col.v))),
    /not a reading this engine has/
  );
  // The curve is still not a band: a `ribbon` needs two boundaries, and one
  // estimate along a continuous axis gives it one.
  refuses(() => render_svg(plot(data(viol), layer(ribbon, density), x(col.v))), /violin/);
});

test("the ridgeline: the half violin laid down, overlapped and traced", () => {
  assert.equal(
    npolys(plot(data(viol), layer(area, density({ reach: 2.5 })), x(col.v), y(col.grp))), 2);
  const traced = render_svg(plot(data(viol), layer(line, density), x(col.v), y(col.grp)));
  assert.ok(!traced.includes("<polygon"), "a traced violin fills nothing");
  assert.ok(traced.includes("<path"), "a traced violin strokes something");
  assert.notEqual(
    render_svg(plot(data(viol), layer(area, density({ reach: 2.5 })), x(col.v), y(col.grp))),
    render_svg(plot(data(viol), layer(area, density), x(col.v), y(col.grp))),
    "`density({ reach })` had no effect"
  );
  refuses(
    () => render_svg(plot(data(viol), layer(line, density({ reach: 2 })), x(col.v))),
    /no slots/
  );
  refuses(() => density({ reach: -1 }), /positive number/);
});

// ---------------------------------------------------------------------------
// Nest — the panel packed with regions (spec §15)
//
// The third answer to what carries a share: length flat, angle in polar, area
// here. What is checked is the property a treemap is read for — the regions are
// the panel and each is its own share of it — plus the refusals the space owns.
// ---------------------------------------------------------------------------

const sales = {
  region: ["North", "North", "South", "South", "East", "East", "West"],
  product: ["widgets", "gadgets", "widgets", "gadgets", "widgets", "gadgets", "widgets"],
  revenue: [32, 14, 25, 8, 19, 11, 6],
};

// Every packed cell as [x, y, w, h]. The legend's swatches carry `rx=` and the
// outer region outlines are `fill="none"`; neither is a cell. The leading space
// in each key matters — without it `width=` also matches `stroke-width=`.
function cells(svg) {
  return svg
    .split("\n")
    .filter((l) => l.includes("<rect") && l.includes("fill-opacity"))
    .filter((l) => !l.includes("rx=") && !l.includes('fill="none"'))
    .map((l) => ["x", "y", "width", "height"].map((k) => Number(l.split(` ${k}="`)[1].split('"')[0])));
}

test("every packed region is its share of the panel", () => {
  const one = render_svg(
    plot(data(sales), layer(bar, sum), y(col.revenue), color(col.region), nest()));
  const cl = cells(one);
  assert.equal(cl.length, 4, "expected one region per region-name");
  const total = cl.reduce((t, c) => t + c[2] * c[3], 0);
  const shares = cl.map((c) => (c[2] * c[3]) / total).sort((a, b) => a - b);
  // North 46, South 33, East 30, West 6 — of 115.
  const want = [6, 30, 33, 46].map((v) => v / 115);
  for (const [i, got] of shares.entries()) {
    assert.ok(Math.abs(got - want[i]) < 0.002, `region got ${got} of the panel, wanted ${want[i]}`);
  }
});

test("a packed panel draws no axes and the flat one does", () => {
  const packed = render_svg(
    plot(data(sales), layer(bar, sum), y(col.revenue), color(col.region), nest()));
  const flat = render_svg(
    plot(data(sales), layer(bar, sum), x(col.region), y(col.revenue), color(col.region)));
  assert.ok(!packed.includes('stroke="#5a5a64"'), "a packed panel drew axis lines");
  assert.ok(flat.includes('stroke="#5a5a64"'), "the flat sentence drew none, so this proves nothing");
});

test("a bound position packs a second level inside each region", () => {
  const two = render_svg(
    plot(data(sales), layer(bar, sum), x(col.region), y(col.revenue), color(col.product), nest()));
  const outlines = (s) => s.split("\n").filter((l) => l.includes("<rect") && l.includes('fill="none"'));
  assert.equal(outlines(two).length, 4, "expected one outline per region");
  const one = render_svg(
    plot(data(sales), layer(bar, sum), y(col.revenue), color(col.region), nest()));
  assert.equal(outlines(one).length, 0, "a one-level packing outlined a region against nothing");
});

test("the packed space refuses what a packing cannot hold", () => {
  refuses(
    () => render_svg(plot(data(sales), layer(bar, sum, stack), y(col.revenue), color(col.region), nest())),
    /own region/
  );
  refuses(
    () => render_svg(plot(data(sales), layer(bar, sum), y(col.revenue), color(col.region), nest(),
                          x_label("Revenue"))),
    /names an axis/
  );
  refuses(
    () => render_svg(plot(data(sales), point, x(col.revenue), y(col.revenue), nest())),
    /placed by a position/
  );
  refuses(
    () => render_svg(plot(data(sales), layer(bar, sum), y(col.revenue, { scale: "log" }),
                          color(col.region), nest())),
    /share of the total/
  );
  refuses(() => nest(90), /takes no arguments/);
});

// A label at the center of its own region — what makes a packing readable once
// the split is too wide for a legend to decode (2026-07-27). The label layer
// needs no `x`: a packing places by region, which is Law 7's third relaxation.
test("a packed label sits inside its own region", () => {
  const svg = render_svg(
    plot(data(sales), bar, y(col.revenue), color(col.region),
         text, label(col.product), nest()));
  // A mark's label carries `fill-opacity` and the legend's key entries do not —
  // the same discriminator `cells()` uses one element over, and needed for the
  // same reason: the key spells out the very strings the labels draw, so
  // counting those would pass whether or not the mark drew anything.
  const names = svg.split("\n")
    .filter((l) => l.trim().startsWith("<text") && l.includes("fill-opacity"));
  assert.ok(names.length > 0, "a packed label drew nothing");
  // Every drawn label sits inside a cell the bar drew, which is the property
  // that makes the mark worth having: the two marks read one packing, so a name
  // cannot land in a rectangle its own row did not get.
  const boxes = cells(svg);
  for (const row of names) {
    const lx = Number(row.split('<text x="')[1].split('"')[0]);
    assert.ok(boxes.some(([bx, , bw]) => bx <= lx && lx <= bx + bw),
              `a label landed outside every region: ${row}`);
  }
});

test("a nudge is refused in a packed panel, where a label covers no point", () => {
  refuses(
    () => render_svg(plot(data(sales), bar, y(col.revenue), color(col.region),
                          text, label(col.product), style({ nudge: "up" }), nest())),
    /covers no point/
  );
});

// ---------------------------------------------------------------------------
// Composition — separate plots arranged on one page (spec §11)
//
// `across()`/`down()` split one plot by a column; `beside()`/`below()` arrange
// two plots. The pair R spells with one operator and an operand type, this
// binding spells with two words — the same trade every operator here makes.
//
// The engine's one rule does the rest: the same column on the same axis in two
// composed plots is one axis — one scale, one panel extent, drawn once.
// ---------------------------------------------------------------------------

const cars = {
  speed: [4, 4, 7, 7, 8, 9, 10, 10, 10, 11, 11, 12, 12, 12, 12, 13, 13, 13, 13, 14,
          14, 14, 14, 15, 15, 15, 16, 16, 17, 17, 17, 18, 18, 18, 18, 19, 19, 19,
          20, 20, 20, 20, 20, 22, 23, 24, 24, 24, 24, 25],
  dist: [2, 10, 4, 22, 16, 10, 18, 26, 34, 17, 28, 14, 20, 24, 28, 26, 34, 34, 46,
         26, 36, 60, 80, 20, 26, 54, 32, 40, 32, 40, 50, 42, 56, 76, 84, 36, 46, 68,
         32, 48, 52, 56, 64, 66, 54, 70, 92, 93, 120, 85],
};
const scatter = () => plot(data(cars, { name: "cars" }), point, x(col.speed), y(col.dist));
const topHist = () =>
  plot(data(cars, { name: "cars" }), layer(bar, bin), x(col.speed), theme({ height: 120 }));
const sideHist = () =>
  plot(data(cars, { name: "cars" }), layer(bar, bin), y(col.dist), theme({ width: 120 }));

test("below(top, beside(main, right)) composes three plots into one page", () => {
  const page = below(topHist(), beside(scatter(), sideHist()));
  const svg = render_svg(page);
  assert.equal(svg.match(/<svg/g).length, 4, "one document holding three plots");

  // The panels of the two plots sharing `speed` run over the same pixels — the
  // whole promise of a marginal plot, and the reason it is not just two plots.
  const panels = [...svg.matchAll(/<rect x="([0-9.]+)" y="[0-9.]+" width="[0-9.]+"[^>]*fill="#f5f5f8"/g)];
  assert.ok(Math.abs(Number(panels[0][1]) - Number(panels[1][1])) < 0.01,
            "the marginal histogram's panel starts where the scatter's does");
  // And the shared axis is drawn once, by the plot nearest the edge it lives on.
  assert.equal(svg.match(/>Speed</g).length, 1, "a shared axis is named once");
});

test("unrelated plots compose without sharing anything", () => {
  const svg = render_svg(beside(scatter(), plot(data(cars, { name: "cars" }), layer(bar, bin), x(col.dist))));
  assert.equal(svg.match(/<svg/g).length, 3);
});

// The other three bindings spell grouping with parentheses and had to be taught
// to refuse it: `plot + (data(df) + point + area)` kept the table, returned, and
// the marks inside stopped existing. JavaScript has no `+` to overload, so the
// same mistake arrives as a plot handed to `plot()` as an argument. It must name
// what happened rather than claim a `Plot` is a bare table, which is the branch
// it used to fall into.
test("a plot cannot be an argument to another plot", () => {
  const note = { speed: [10], dist: [40] };
  refuses(
    () => plot(data(cars, { name: "cars" }), point, x(col.speed), y(col.dist),
               plot(data(note, { name: "note" }), point)),
    /cannot be an argument to another plot/
  );
  refuses(
    () => plot(data(cars, { name: "cars" }), point, x(col.speed), y(col.dist),
               plot(data(note, { name: "note" }), point)),
    /beside\(\) *` *or *`? *below\(\)|beside\(\)/
  );
  // The bare-table message is a different mistake and must not have been taken over.
  refuses(() => plot(data(cars, { name: "cars" }), point, note), /starts with `data\(\)`/);

  // Repeating `data()` per mark is the direction the refusal gives, and it works.
  const seq = plot(data(cars, { name: "cars" }), x(col.speed), y(col.dist), line,
                   data(note, { name: "note" }), point,
                   data(note, { name: "note" }), area);
  // `plot()` seals every layer into `spec.layers`; nothing is left open.
  assert.equal(seq.spec.layers.length, 3);
  assert.deepEqual(
    seq.spec.layers.map((l) => l.data ?? "<default>"),
    ["<default>", "note", "note"]
  );
});

test("theme({ width, height }) is the image alone and the cell composed", () => {
  const alone = render_svg(plot(data(cars, { name: "cars" }), point, x(col.speed), y(col.dist),
                                theme({ width: 400, height: 300 })));
  assert.match(alone, /width="400" height="300"/);
  refuses(() => theme({ width: 10 }), /at least 40/);
  const tall = (height) =>
    plot(data(cars, { name: "cars" }), point, x(col.speed), y(col.dist), theme({ height }));
  refuses(() => render_svg(below(tall(500), tall(500), theme({ height: 600 }))),
    /makes 1020px; the page has 600/);
  // An ask is pixels: a page with no size of its own is as big as its plots ask,
  // the gap between them included, and plots side by side share one height.
  assert.match(render_svg(below(tall(300), tall(300))), /width="800" height="620"/);
  refuses(() => render_svg(beside(tall(100), tall(200))), /share one height/);
});

// And a *page* states its own size, which is the one sentence no cell can write.
// Composed side by side, two plots divide the page's width and share one
// height, which the page states, or a plot on it asks for. A
// `theme()` among the figures is how JavaScript spells `(a | b) + theme(...)`.
test("a page states its own size, and takes the canvas when it does not", () => {
  const sized = render_svg(beside(scatter(), scatter(), theme({ height: 310 })));
  assert.match(sized, /width="800" height="310"/);
  assert.match(render_svg(beside(scatter(), scatter())), /width="800" height="600"/);
  // The size is the only theme property whose subject is the figure. Every
  // other one describes a panel, and a page has none.
  refuses(() => beside(scatter(), scatter(), theme({ grid: "none" })), /describes a panel/);
  refuses(() => beside(scatter(), scatter(), theme("minimal")), /describes a panel/);
});

// There is no `+` to refuse here and no `facet()` to mis-join — JavaScript spells
// both with words, so the only way to misuse a page is to hand `beside()`/`below()`
// something that is not a plot, or only one of them.
test("a page is arranged, and says what it arranges", () => {
  refuses(() => beside(scatter()), /two or more plots/);
  refuses(() => beside(scatter(), point), /arranges plots/);
  refuses(() => below(scatter(), across(col.speed)), /arranges plots/);
  // A theme is set aside before the plots are counted, so the arity a reader is
  // asked for is the one they wrote.
  refuses(() => beside(scatter(), theme({ height: 310 })), /two or more plots/);
});

// --- partition: a hierarchy in columns, one ring per level -------------------
test("a partition is the icicle flat and the sunburst bent", () => {
  const budget = {
    group: ["A", "A", "A", "B"],
    item: ["p", "q", "q", "r"],
    detail: [null, "deep", "also", null],
    amount: [4, 3, 3, 10],
  };
  const tree = () => layer(zone, partition(col.group, col.item, col.detail));
  const sun = render_svg(plot(data(budget, { name: "budget" }), tree(),
    x(col.amount), color(col.group), polar()));
  assert.match(sun, /<path/);
  const icicle = render_svg(plot(data(budget, { name: "budget" }), tree(),
    x(col.amount), color(col.group)));
  assert.match(icicle, /<rect/);
  assert.notStrictEqual(sun, icicle, "bending the space changes the picture");

  // The second reader: one computation feeds a rectangle and a label.
  const named = render_svg(plot(data(budget, { name: "budget" }), tree(), x(col.amount),
    layer(text, partition(col.group, col.item, col.detail)), label(col.name), polar()));
  assert.match(named, />deep</);
});

test("a partition refuses what it cannot mean", () => {
  const budget = { group: ["A", "B"], item: ["p", "q"], amount: [4, 6] };
  refuses(() => render_svg(plot(data(budget, { name: "budget" }),
    layer(bar, partition(col.group, col.item)), x(col.amount))), /no reading for a region/);
  refuses(() => partition(), /outermost first/);
  const mixed = { group: ["A", "A"], item: [null, "p"], amount: [5, 5] };
  refuses(() => render_svg(plot(data(mixed, { name: "mixed" }),
    layer(zone, partition(col.group, col.item)), x(col.amount))), /value of its own/);
});

// --- flow: a magnitude laid through its stages -------------------------------
// Three marks read one layout: `ribbon` the bands, `zone` the slots, `text` the
// names. The band is the renderer's first cubic curve, and two renders of one
// sentence are one picture.
test("`flow` lays bands, slots and names from one layout", () => {
  const voyage = {
    klass: ["First", "First", "Third", "Third"],
    survived: ["yes", "no", "yes", "no"],
    n: [203, 122, 178, 528],
  };
  const alluvial = () => render_svg(plot(data(voyage, { name: "voyage" }), y(col.n),
    layer(ribbon, flow(col.klass, col.survived)), color(col.klass),
    layer(zone, flow(col.klass, col.survived)),
    layer(text, flow(col.klass, col.survived)), label(col.name)));
  const first = alluvial();
  assert.match(first, / C /, "a flow's band is a cubic curve");
  assert.match(first, /<rect/, "a flow's slots are rectangles");
  assert.match(first, />First</, "`label(col.name)` names each slot");
  assert.strictEqual(first, alluvial(), "one flow sentence is one picture, every run");

  refuses(() => flow(col.klass), /at least two stage columns/);
  // The endpoint clause, and the spelling. `col.class` is legal here — the
  // accessor is a Proxy, so a reserved word is a fine property key — and it is
  // what the book's table actually calls that column.
  {
    let msg = "";
    try { flow(col.class); } catch (e) { msg = e.message; }
    assert.ok(msg.includes("`col.class` to its `col.survived`"),
      `the refusal must name its example's own endpoints: ${msg}`);
    assert.ok(!msg.includes("col.klass"),
      `the refusal must not send a reader to a column no table has: ${msg}`);
  }
  refuses(() => render_svg(plot(data(voyage, { name: "voyage" }), y(col.n),
    layer(bar, flow(col.klass, col.survived)))), /no reading for that/);
  refuses(() => render_svg(plot(data(voyage, { name: "voyage" }), x(col.n),
    layer(ribbon, flow(col.klass, col.survived)))), /reorder `flow\(\.\.\.\)`'s arguments/);
  refuses(() => render_svg(plot(data(voyage, { name: "voyage" }), y(col.n),
    layer(ribbon, flow(col.klass, col.survived)), color(col.n))),
    /must name one of the atom's stages/);
});

// --- network: a graph placed by its layout ----------------------------------
test("`edge`, `point` and `text` read one layout in `network()`", () => {
  const trade = {
    exporter: ["Korea", "Korea", "Japan", "China"],
    importer: ["Japan", "China", "China", "India"],
    tons: [3, 4, 3, 2],
  };
  const web = () => render_svg(plot(data(trade, { name: "trade" }),
    layer(edge, layout(col.exporter, col.importer)), opacity(col.tons),
    layer(point, layout(col.exporter, col.importer)), size(col.degree),
    layer(text, layout(col.exporter, col.importer), repel), label(col.name),
    network()));
  const first = web();
  assert.match(first, /<line/, "a network draws its edges as strokes");
  assert.match(first, />Korea</, "`label(col.name)` names each node");
  assert.doesNotMatch(first, /tick/, "the graph-theoretic space draws no ticks");
  assert.strictEqual(first, web(), "one network sentence is one picture, every run");
  const cube = render_svg(plot(data(trade, { name: "trade" }),
    layer(edge, layout(col.exporter, col.importer)),
    layer(point, layout(col.exporter, col.importer)),
    network({ turn: 40, tilt: 20 })));
  assert.notStrictEqual(first, cube, "a stated angle changes the picture");

  refuses(() => render_svg(plot(data(trade, { name: "trade" }),
    layer(edge, layout(col.exporter, col.importer)))), /network\(\)/);
  refuses(() => render_svg(plot(data(trade, { name: "trade" }),
    layer(point), network())), /edge \* layout\(from, to\)/);
  refuses(() => layout(col.exporter), /two endpoint columns/);
});

// --- cluster: the tree of merges, and the seriated tile plot -----------------
test("`cluster` draws the tree, lies down, and reorders the tiles", () => {
  const pantry = {
    food: ["rice", "rice", "lentils", "lentils",
           "chicken", "chicken", "oats", "oats"],
    nutrient: ["protein", "iron", "protein", "iron",
               "protein", "iron", "protein", "iron"],
    amount: [2.7, 0.8, 9.0, 3.3, 25.0, 2.6, 3.4, 1.8],
  };
  const dendro = () => render_svg(plot(data(pantry, { name: "pantry" }),
    layer(path, cluster(col.amount, { over: col.nutrient })), x(col.food)));
  const first = dendro();
  assert.match(first, />Distance</, "the unbound axis names itself Distance");
  assert.strictEqual(first.split("<polyline").length - 1, 3,
    "three merges are three elbow strokes");
  assert.strictEqual(first, dendro(), "one cluster sentence is one picture");
  const sideways = render_svg(plot(data(pantry, { name: "pantry" }),
    layer(path, cluster(col.amount, { over: col.nutrient })), y(col.food)));
  assert.match(sideways, />Distance</, "the sideways tree titles its axis");
  const plain = render_svg(plot(data(pantry, { name: "pantry" }),
    layer(zone), x(col.food), y(col.nutrient), color(col.amount)));
  const sorted = render_svg(plot(data(pantry, { name: "pantry" }),
    layer(zone, cluster({ over: col.nutrient })),
    x(col.food), y(col.nutrient), color(col.amount)));
  assert.notStrictEqual(plain, sorted, "the reorder reading must reorder");

  refuses(() => render_svg(plot(data(pantry, { name: "pantry" }),
    layer(bar, cluster(col.amount, { over: col.nutrient })), x(col.food))),
    /path \* cluster/);
  refuses(() => render_svg(plot(data(pantry, { name: "pantry" }),
    layer(path, cluster({ over: col.nutrient })), x(col.food))),
    /value column/);
  refuses(() => render_svg(plot(data(pantry, { name: "pantry" }),
    layer(path, cluster(col.amount, { over: col.nutrient })),
    x(col.food), y(col.amount))), /names itself/);
});

// One parameter apart from the icicle, and it buys the whole plot: the levels
// turn across each other instead of running down one axis. The engine pins the
// arithmetic; here that the sentence draws and that crossing is visible in the
// output rather than silently ignored.
const counts = {
  decade: ["1950s", "1950s", "1960s", "1960s"],
  theme: ["Heartbreak", "Love", "Heartbreak", "Love"],
  n: [10, 10, 30, 40],
};
const crossed = (...extra) => render_svg(plot(data(counts, { name: "counts" }), x(col.n),
  layer(zone, partition(col.decade, col.theme, { cross: true })), color(col.theme), ...extra));

test("`partition({ cross: true })` is the mosaic", () => {
  const mosaic = crossed();
  const nested = render_svg(plot(data(counts, { name: "counts" }), x(col.n),
    layer(zone, partition(col.decade, col.theme)), color(col.theme)));
  assert.match(mosaic, /<rect/, "a crossed partition draws its cells");
  assert.notStrictEqual(mosaic, nested, "`cross: true` must change the picture");
  assert.match(mosaic, /Share of column/, "the second axis names what it carries");

  // The labeling idiom, carried over from the sunburst: a shallower partition
  // of the same table lands its nodes in the same columns.
  const labeled = crossed(
    layer(text, partition(col.decade, { cross: true })), label(col.name));
  assert.match(labeled, />1960s</, "a shallower crossed partition names the columns");

  refuses(() => partition(col.decade, { cross: "yes" }), /true or false/);
  refuses(() => partition(col.decade, { crossed: true }), /takes `cross`/);
});

// The settable rule spans a setting across its geometry class, and `zone` joined
// the closed-glyph fills on 2026-07-27 because a mosaic without cell edges is one
// blob wherever two neighbors share a color. Refused until that day, so this is
// the ruling rather than a feature test.
test("a `zone` carries `style({ border_color, border_size })`", () => {
  const edged = crossed(style({ border_color: "white", border_size: 2 }));
  assert.match(edged, /stroke="white"/, "a zone draws the border it was given");
  assert.doesNotMatch(crossed(), /stroke="white"/, "an unasked-for border must not appear");
});

// ---------------------------------------------------------------------------
// Packaging — an installed copy has to arrive carrying an engine
// ---------------------------------------------------------------------------

// The engine ships as one npm package per platform, and the set of platforms is
// written down three times: `ENGINE_PLATFORMS` in `render.js`, the
// `optionalDependencies` keys here, and the release workflow's build matrix.
// These tests hold the first two together.
//
// What makes them worth having is that the drift they catch is not a crash
// anywhere. npm **omits an optional dependency it cannot install without failing
// the install**, so a platform named in one list and missing from the other
// installs cleanly and then cannot draw — on that platform alone, which is
// reliably the machine the author does not own.
const manifest = JSON.parse(
  fs.readFileSync(new URL("../package.json", import.meta.url), "utf8")
);

test("the built platforms are one list, written twice", () => {
  const prefix = `${manifest.name}-`;
  const pinned = Object.keys(manifest.optionalDependencies ?? {});

  assert.deepEqual(
    pinned.map((name) => name.slice(prefix.length)).sort(),
    [...ENGINE_PLATFORMS].sort(),
    "`ENGINE_PLATFORMS` and `optionalDependencies` name different platforms"
  );

  // A pin that is not this version asks for a package no release published, and
  // that too is reported as silence rather than as an error.
  for (const name of pinned) {
    assert.equal(
      manifest.optionalDependencies[name],
      manifest.version,
      `${name} is pinned away from the package's own version`
    );
  }
});

test("every platform name is one Node would report for some machine", () => {
  // `platform_package()` spells the name from `process.platform` and
  // `process.arch`, so a typo in either half is a package no install ever asks
  // for. Node's own vocabulary is what settles it.
  const platforms = new Set(["darwin", "linux", "win32", "freebsd", "openbsd", "sunos", "aix"]);
  const arches = new Set(["arm64", "x64", "arm", "ia32", "ppc64", "s390x", "riscv64", "loong64"]);

  for (const target of ENGINE_PLATFORMS) {
    const [platform, arch, ...rest] = target.split("-");
    assert.equal(rest.length, 0, `\`${target}\` is not <platform>-<arch>`);
    assert.ok(platforms.has(platform), `\`${platform}\` is not a Node platform`);
    assert.ok(arches.has(arch), `\`${arch}\` is not a Node architecture`);
  }

  // And the machine running this suite. Either the engine is built for it and
  // the name is one of the five, or it is not and the answer is `null` — never a
  // package name that was never published.
  const here = platform_package();
  if (here === null) {
    assert.ok(
      !ENGINE_PLATFORMS.includes(`${process.platform}-${process.arch}`),
      "this platform is built for, so it must have a package name"
    );
  } else {
    assert.ok(Object.hasOwn(manifest.optionalDependencies, here), `${here} is not pinned`);
  }
});

test("nothing in the manifest blocks a publish", () => {
  // `private: true` was correct for as long as there was no engine to ship, and
  // it is npm's refuse-to-publish flag. Leaving it set fails a release late and
  // quietly: one line in a log, after the build matrix has already run.
  assert.ok(!manifest.private, "`private: true` would make `npm publish` refuse");

  // Apache 2.0 §4(a) binds whoever hands out a copy to hand out the License with
  // it, and the repository's own sits above where an installer can reach.
  assert.equal(manifest.license, "Apache-2.0");
  for (const file of ["src", "LICENSE", "NOTICE", "README.md"]) {
    assert.ok(manifest.files.includes(file), `\`${file}\` is not in the published set`);
  }
});

// ---------------------------------------------------------------------------
// query() — the table that is not in memory
//
// The guard is the one that matters and it is the same in all four bindings: the
// *same sentence*, over a materialized table and over a query returning the same
// rows, must render byte-identical SVG. If those diverge, `query()` has stopped
// being a way of naming rows and become a second way of drawing them.
//
// `node:sqlite` is used rather than a driver from npm because it is standard
// library from Node 22, and this package depends on nothing. It is also exactly
// the shape `query()` duck-types on — `.prepare(sql).all()`, synchronous.
// ---------------------------------------------------------------------------

test("query() draws what data() draws, byte for byte", () => {
  const rows = [
    { status: "open", revenue: 120 },
    { status: "shipped", revenue: 240.5 },
    { status: "shipped", revenue: 95.25 },
    { status: "closed", revenue: 310.75 },
    { status: "open", revenue: 60 },
    { status: "refunded", revenue: 45 },
  ];
  const frame = {
    status: rows.map((r) => r.status),
    revenue: rows.map((r) => r.revenue),
  };
  const db = new DatabaseSync(":memory:");
  db.exec("CREATE TABLE orders (status TEXT, revenue REAL)");
  const insert = db.prepare("INSERT INTO orders VALUES (?, ?)");
  for (const r of rows) insert.run(r.status, r.revenue);
  const sql = "SELECT status, revenue FROM orders";

  for (const [label, sentence] of [
    ["point with two positions", (t) => plot(t, point, x(col.revenue), y(col.status))],
    ["layer(bar, count)", (t) => plot(t, layer(bar, count), x(col.status))],
    ["bar with a mapped color",
      (t) => plot(t, bar, x(col.status), y(col.revenue), color(col.status))],
  ]) {
    const fromTable = render_svg(sentence(data(frame, { name: "orders" })));
    const fromQuery = render_svg(sentence(query(db, sql, { name: "orders" })));
    assert.equal(fromQuery, fromTable, `query() and data() disagree on ${label}`);
  }
  db.close();
});

test("query() holds the SQL rather than running it when the sentence is built", () => {
  // An eager query would foreclose pushing the transform down, since the planner
  // has to see the whole sentence before it knows what to ask the database for.
  const atom = query({ prepare: () => { throw new Error("ran too early"); } },
    "SELECT nonsense FROM nowhere");
  assert.ok(atom.fields.table instanceof Query);
});

test("query() refuses what it cannot draw, and says what to do", () => {
  // The mistake `data()` invites, that atom taking one argument.
  assert.throws(() => query("SELECT 1"), /takes the connection first/);
  assert.throws(() => query({}, 123), /takes a SELECT as text/);

  // `render_svg` is synchronous, so an async driver cannot be awaited here. It
  // is named rather than left to fail as `[object Promise]` on the wire.
  assert.throws(
    () => render_svg(plot(query({ query: () => Promise.resolve() }, "SELECT 1"),
      layer(bar, count), x(col.status))),
    /looks asynchronous/
  );
  assert.throws(
    () => render_svg(plot(query({}, "SELECT 1"), layer(bar, count), x(col.status))),
    /must be a synchronous one/
  );
});

// --- gog_table(): the manual's tables, without a CSV reader to copy ---------
//
// Binding plumbing rather than a word of the grammar, which is why
// `book/check_vocabulary.R` excludes it from the kernel block beside
// `render_svg`. This binding is the reason the helper moved into the packages
// at all: JavaScript has no CSV parser in its standard library, so a reader had
// to paste thirty-three lines of quote handling before drawing anything.
test("gog_table: quoting, types, and a name it refuses", async () => {
  const { parse_csv, columns, gog_table, BOOK_DATA_URL } =
    await import("../src/tables.js");

  assert.equal(BOOK_DATA_URL, "https://psychometrician.github.io/gog-book/data/");

  // The reason the parser exists: one country's name holds a comma, so a line
  // split on commas gives that row more fields than the header has.
  const rows = parse_csv('country,gdp\n"Congo, Dem. Rep.",277\nPeru,7409');
  assert.deepEqual(rows[1], ["Congo, Dem. Rep.", "277"]);

  const typed = columns(rows);
  assert.deepEqual(typed.country, ["Congo, Dem. Rep.", "Peru"]);
  assert.deepEqual(typed.gdp, [277, 7409]);

  // A column of labels that look like numbers stays text when it is named.
  const labelled = columns(parse_csv("session,n\n01,5\n02,7"), ["session"]);
  assert.deepEqual(labelled.session, ["01", "02"]);
  assert.deepEqual(labelled.n, [5, 7]);

  // `GogError`, not `TypeError`: one class for every refusal in the package,
  // so `catch (e) { if (e instanceof GogError) … }` catches the lot.
  await assert.rejects(() => gog_table(42), (error) => {
    assert.ok(error instanceof GogError, `not a GogError: ${error}`);
    assert.match(error.message, /one table name/);
    return true;
  });
});

// The near-miss rule, tested where it is deterministic. The list is written here
// rather than fetched, so this says what the rule does and not what the site
// currently holds — and it runs on a laptop with no network.
test("gog_table: a near miss is suggested, a far one is not", async () => {
  const { nearest_table, unknown_table, BOOK_DATA_CHAPTER } =
    await import("../src/tables.js");

  // The rule is the engine's `nearest_color`: within two edits, and fewer edits
  // than the candidate has letters. The last two probes are the ones that
  // matter, because a suggestion rule is judged by what it declines to say.
  // `penguins` is nothing like any of these, and `gm` is short enough that a
  // loose rule would match half the list.
  const known = ["gapminder_2007", "gapminder_asia", "gm_all", "winds", "medals"];
  assert.equal(nearest_table("gapminder2007", known), "gapminder_2007");
  assert.equal(nearest_table("Gapminder_2007", known), "gapminder_2007");
  assert.equal(nearest_table("gapmidner_2007", known), "gapminder_2007");
  assert.equal(nearest_table("wind", known), "winds");
  assert.equal(nearest_table("penguins", known), null);
  assert.equal(nearest_table("gm", known), null);
  // No list to read from is the offline case, and it must not be an error.
  assert.equal(nearest_table("gapminder2007", []), null);

  // The two sentences, in full. All four bindings print these words exactly, so
  // a change here is a change every reader of the manual sees four times.
  assert.equal(
    unknown_table("gapminder2007", known),
    'gog: there is no table called "gapminder2007". Did you mean "gapminder_2007"?',
  );
  assert.equal(
    unknown_table("penguins", known),
    'gog: there is no table called "penguins". The table names are listed in ' +
      `the book's data chapter: ${BOOK_DATA_CHAPTER}`,
  );
});

// The defect this binding had alone, and the reason the status is read rather
// than the body. `fetch` does not throw on a 404: it resolves, with the site's
// 404 page as the body, and the CSV reader above parsed that page into an
// eighty-eight row table whose one column was named `<!DOCTYPE html>`. Nothing
// in the suite could see it, because every check ran on a name that exists.
//
// Served locally rather than fetched, so this runs with no network and asserts
// the mechanism instead of the site's current behavior.
test("gog_table: a 404 page is refused, never parsed as a table", async () => {
  const { gog_table, BOOK_DATA_URL } = await import("../src/tables.js");
  const real = globalThis.fetch;
  // The site as it behaves: the list of names is served, and a name it does not
  // have gets the 404 page. This is the whole chain in one test — status read,
  // list fetched, near miss found, refusal worded.
  globalThis.fetch = async (url) =>
    String(url).endsWith("tables.txt")
      ? new Response("gapminder_2007\nwinds\nmedals\n", { status: 200 })
      : new Response("<!DOCTYPE html>\n<html>\n  <head>\n", {
        status: 404, headers: { "content-type": "text/html" },
      });
  try {
    await assert.rejects(() => gog_table("gapminder2007"), (error) => {
      assert.ok(error instanceof GogError, `not a GogError: ${error}`);
      assert.equal(error.message,
        'gog: there is no table called "gapminder2007". ' +
        'Did you mean "gapminder_2007"?');
      return true;
    });

    // And with no list to read, the chapter is the answer rather than a guess.
    globalThis.fetch = async () =>
      new Response("<!DOCTYPE html>", { status: 404 });
    await assert.rejects(() => gog_table("gapminder2007"), (error) => {
      assert.match(error.message, /there is no table called "gapminder2007"/);
      assert.doesNotMatch(error.message, /DOCTYPE/);
      assert.match(error.message, /data chapter/);
      return true;
    });

    // A table name that is fine, refused because the site itself is down. The
    // two cases ask opposite things of the reader, so they must not share words.
    globalThis.fetch = async () => { throw new TypeError("fetch failed"); };
    await assert.rejects(() => gog_table("gapminder_2007"), (error) => {
      assert.ok(error instanceof GogError, `not a GogError: ${error}`);
      assert.match(error.message, /could not reach/);
      assert.ok(error.message.includes(BOOK_DATA_URL), error.message);
      return true;
    });
  } finally {
    globalThis.fetch = real;
  }
});

// The old name is gone rather than deprecated, and this is the assertion that
// keeps it gone. Both doors have to stay shut: the module could keep exporting
// it and `index.js` could re-export it, and either would put two spellings of
// one function back in the vocabulary quietly.
test("book_table: gone, not deprecated", async () => {
  const tables = await import("../src/tables.js");
  const index = await import("../src/index.js");

  assert.equal(tables.book_table, undefined, "book_table survived in tables.js");
  assert.equal(index.book_table, undefined, "book_table survived in index.js");
  assert.equal(typeof index.gog_table, "function");
});

// ---------------------------------------------------------------------------
// brush — the selection
//
// Four claims, and the second is the one the whole feature rests on: a plot that
// names no brush must be exactly the plot it was before selection existed.
// ---------------------------------------------------------------------------

test("brush dims the rows outside the bound and drops none", () => {
  const d = { v: [1, 2, 3, 4, 5, 6], w: [2, 4, 1, 5, 3, 6],
              kind: ["a", "a", "b", "b", "c", "c"] };
  const svg = render_svg(plot(data(d, { name: "bt" }), point,
    x(col.v), y(col.w), brush(col.v, { at: [2.5, 4.5] })));
  assert.ok(svg.includes('<g opacity="0.150">'), "no dimmed group");
  // A brush highlights; it never removes rows. That is what separates it from
  // `limits`, and it is the claim a reader is most likely to test.
  assert.equal((svg.match(/<circle/g) || []).length, 6, "brush must dim, not filter");
});

// A shape `bounds` places is still one row, so a brush reaches it: the one zone
// and the one whisker outside the bound are pushed back.
test("a brush reaches the shapes bounds places, one row each", () => {
  const d = { lo: [1, 3, 6], hi: [2, 5, 8], g: ["a", "b", "c"] };
  const dimmed = (svg) => svg.split('<g opacity="0.150">')[1].split("</g>")[0];
  const z = render_svg(plot(data(d, { name: "spb" }), layer(zone, bounds(col.lo, col.hi)),
    brush(col.lo, { at: [0, 4] })));
  assert.equal((dimmed(z).match(/<rect/g) || []).length, 1, "one zone pushed back");
  const w = render_svg(plot(data(d, { name: "spb" }), layer(interval, bounds(col.lo, col.hi)),
    x(col.g), brush(col.lo, { at: [0, 4] })));
  assert.equal((dimmed(w).match(/<line/g) || []).length, 3, "one whisker pushed back");
});

test("a plot with no brush is untouched by selection", () => {
  const d = { v: [1, 2, 3], w: [2, 4, 1] };
  const svg = render_svg(plot(data(d, { name: "bt" }), point, x(col.v), y(col.w)));
  assert.ok(!svg.includes("data-gog-panel"));
  assert.ok(!svg.includes("<g opacity="));
});

test("brush on a category column selects slots", () => {
  const d = { v: [1, 2, 3, 4], w: [2, 4, 1, 5], kind: ["a", "a", "b", "b"] };
  const svg = render_svg(plot(data(d, { name: "bt" }), point,
    x(col.v), y(col.w), brush(col.kind, { at: "b" })));
  assert.ok(svg.includes('<g opacity="0.150">'));
});

test("a line has no single row to select, and the refusal names the layers that do", () => {
  const d = { v: [1, 2, 3], w: [2, 4, 1] };
  // Every layer that draws one row per shape is named, an `interval` under `bounds`
  // with its transform, and `group()` is not: a grouped line is refused the same
  // way, so the advice could not be followed.
  assert.throws(() => render_svg(plot(data(d, { name: "bt" }), line,
    x(col.v), y(col.w), brush(col.v, { at: [1, 2] }))),
    (e) => /one shape through many rows[\s\S]*`point`, `text`, `rule`, `zone` and `interval \* bounds`/.test(e.message)
      && !e.message.includes("group()"));
  assert.throws(() => render_svg(plot(data(d, { name: "bt" }), area,
    x(col.v), y(col.w), brush(col.v, { at: [1, 2] }))),
    (e) => e.message.startsWith("gog: an `area` draws"));
});

test("`at` is two numbers or a set of names", () => {
  assert.throws(() => brush(col.v, { at: [1, 2, 3] }), /two numbers/);
});

// The engine beside the package is the package's own.
//
// Thirteen declarations agreeing says nothing about the binary that draws. They
// are separate artifacts and they went out of step exactly once it mattered: a
// package carried an engine a whole release behind its own manifest, and
// nothing in this repository could see it. Not the version guard, which reads
// files; not the parity harness, which drew all 740 sentences of the manual
// through both engines and found them identical, because two builds a patch
// apart agree on every sentence that did not change between them. Bytes cannot
// answer it either: an engine compiled inside an installed package hashes
// differently from the same sources built in a checkout, because the build path
// travels in the binary.
//
// `stdio` closes stdin, and that is not tidiness. An engine older than the flag
// does not reject `--version`; it ignores the argument and blocks reading
// stdin forever, since stdin is how a plot arrives. The obvious spelling of
// this check hangs on exactly the engine it exists to catch.
test("the engine reports the version the package declares", () => {
  const engine = find_gog_cli();
  const answer = spawnSync(engine, ["--version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    timeout: 30_000,
  });
  const reported = (answer.stdout ?? "").trim();

  assert.match(
    reported,
    /^\d+\.\d+\.\d+/,
    `the engine at ${engine} cannot say which version it is; it answered ` +
      `${JSON.stringify(reported)}. An engine without \`--version\` predates ` +
      `this check, so it is older than the package beside it. ` +
      `Rebuild: cargo build --release -p gog-cli`
  );
  assert.equal(
    reported,
    manifest.version,
    `the package says ${manifest.version} and its engine says ${reported}. ` +
      `Engine: ${engine}. A plot drawn now is drawn by the wrong release.`
  );
});

// ---------------------------------------------------------------------------
// range() — the band's two ends, as quantile probabilities
// ---------------------------------------------------------------------------

test("range() takes a quantile band, bare stays the extremes", () => {
  const table = {
    g: Array(10).fill("a"),
    v: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
  };
  const sentence = (transform) =>
    plot(data(table), layer(interval, transform), x(col.g), y(col.v));
  const band = render_svg(sentence(range(0.25, 0.75)));
  const whole = render_svg(sentence(range));
  assert.notEqual(band, whole, "range(0.25, 0.75) drew what bare `range` draws");
  // 1..10 by type 7: Q1 = 3.25 and Q3 = 7.75, the numbers R's `quantile()` gives.
  assert.ok(band.includes(">4</text>"), "the band should reach 3.25..7.75");
  assert.ok(!band.includes(">10</text>"), "the band should not reach the maximum");
  // Bare `range` is the whole group, which is what it has always drawn.
  assert.ok(whole.includes(">10</text>"), "bare `range` should reach the maximum");

  assert.throws(() => range(0.5, 1.5), GogError);
  assert.throws(() => range(-0.1), GogError);
  assert.throws(() => range("a"), GogError);
  assert.throws(() => render_svg(sentence(range(0.75, 0.25))), GogError);
});

// ---------------------------------------------------------------------------
// deviation and quantile — the family's two newest members
// ---------------------------------------------------------------------------

test("deviation bands the spread, quantile needs its probability", () => {
  const table = { g: Array(8).fill("a"), v: [2, 4, 4, 4, 5, 5, 7, 9] };
  const say = (mark, transform) =>
    plot(data(table), layer(mark, transform), x(col.g), y(col.v));
  const oneSd = render_svg(say(interval, deviation));
  assert.notEqual(render_svg(say(interval, deviation(2))), oneSd);
  // A spread band and the mean's interval are different questions, which is the
  // whole reason both atoms exist.
  assert.notEqual(render_svg(say(interval, confidence)), oneSd);
  assert.throws(() => deviation(0), GogError);

  assert.notEqual(render_svg(say(bar, quantile(0.9))), render_svg(say(bar, median)));
  // No default, because the sensible one is already `median`.
  assert.throws(() => render_svg(say(bar, quantile)), GogError);
  assert.throws(() => quantile(1.5), GogError);
  assert.throws(() => quantile(-0.1), GogError);
});

test("the globe draws its disk and graticule, and refuses with direction", () => {
  // The same marks as a map stand on the facing hemisphere; the far half is
  // hidden behind the sphere, and a globe draws no axes at all.
  const places = { lon: [178.44, 139.69, -0.13], lat: [-18.14, 35.69, 51.51] };
  const svg = render_svg(
    plot(data(places), point, x(col.lon), y(col.lat), globe({ turn: 178, tilt: -18 }))
  );
  assert.ok(svg.includes("<circle"), "the globe drew no disk");
  assert.ok(svg.includes("<polyline"), "the globe drew no graticule");
  assert.ok(!svg.includes("<text"), "a globe grew an axis label");
  // The globe carries the engine for the cube's own reason: an angle worth
  // dragging. Its gate missed this on the day the space shipped, so every
  // globe page drew perfectly with zoom buttons and no drag.
  const block = html_block(
    plot(data(places), point, x(col.lon), y(col.lat), globe({ turn: 178, tilt: -18 }))
  );
  assert.ok(block.includes("wasm"), "a globe page must carry the engine to turn");
  refuses(
    () => render_svg(plot(data(places), bar, x(col.lon), y(col.lat), globe())),
    /measures along the radius/
  );
  refuses(
    () => render_svg(plot(data(places), point, x(col.lon), y(col.lat), globe({ tilt: 100 }))),
    /outside -90 to 90/
  );
  // With its measure named, the bar is the spike.
  const spiky = { ...places, v: [3, 9, 5] };
  const spikeSvg = render_svg(
    plot(data(spiky), bar, x(col.lon), y(col.lat), z(col.v), globe({ turn: 178, tilt: -18 }))
  );
  assert.ok(spikeSvg.includes("<line "), "a spike drew no stroke");
});

test("the turnable block is reachable from the package's front door", async () => {
  // The defect this catches was invisible to every other check here, because
  // every other check imports `../src/render.js` by path — which a reader
  // cannot do. `package.json` exports `"."` alone, so what a reader can call is
  // exactly what `index.js` re-exports, and the interactive block was not in
  // that list. Three bindings hand a notebook the turnable plot through a
  // display hook of their host's; JavaScript has no hook to register with, so
  // an unexported function here is a capability that does not exist.
  //
  // Imported the way a reader imports it, by package name resolved through the
  // package's own manifest, rather than by the path this file uses everywhere
  // else. That is the whole point of the test.
  const url = new URL("../package.json", import.meta.url);
  const manifest = JSON.parse(fs.readFileSync(url, "utf8"));
  assert.equal(manifest.exports["."], "./src/index.js",
    "the front door moved; this test is asserting against the wrong one");

  const front = await import(new URL(manifest.exports["."], url));
  assert.equal(typeof front.html_block, "function",
    "`html_block` is not reachable from the package's only entry point");

  // And it is the interactive one, not the static picture wearing its name.
  const p = plot(data({ a: [1, 2, 3], b: [4, 5, 6] }), point, x(col.a), y(col.b));
  const block = front.html_block(p);
  assert.ok(block.includes("<svg"), "the block carries no picture");
  assert.ok(block.includes("gog-plot"), "the block is not the page's container");
});

// JavaScript keeps its own copy of `pattern`'s values, so a vocabulary grown in
// the engine and not here fails silently. Each value is drawn, not compared to a
// second list.
test("the grown which-one vocabularies all draw", () => {
  const d = data({ a: [1, 2, 3], b: [4, 5, 6] });
  for (const s of ["circle", "square", "triangle", "diamond", "cross", "star", "wye"])
    assert.ok(render_svg(plot(d, point, x(col.a), y(col.b), style({ shape: s }))).includes("<svg"),
      `the glyph ${s} did not draw`);
  for (const p of ["solid", "dashed", "dotted", "dotdash", "longdash"])
    assert.ok(render_svg(plot(d, line, x(col.a), y(col.b), style({ pattern: p }))).includes("<svg"),
      `the dash ${p} did not draw`);
  for (const f of ["solid", "hatch", "crosshatch", "stripes", "grid", "dots"])
    assert.ok(render_svg(plot(d, bar, x(col.a), y(col.b), style({ pattern: f }))).includes("<svg"),
      `the fill texture ${f} did not draw`);
});

// `theme({ axis_label })` states one convention for both axis names.
test("axis_label places both axis names by one rule", () => {
  const d = data({ a: [1, 2, 3], b: [4, 5, 6] });
  const base = [d, point, x(col.a), y(col.b), x_label("A"), y_label("B")];
  assert.ok(render_svg(plot(...base)).includes("rotate(-90"),
    "the default should turn the y name through 90 degrees");
  assert.ok(!render_svg(plot(...base, theme({ axis_label: "end" }))).includes("rotate(-90"),
    "`end` should leave every name horizontal");
  assert.throws(() => theme({ axis_label: "sideways" }), /end|beside/);
});

// Two plots in one document may not share an id: a notebook is one document, and
// a second plot borrowing the first's <pattern> draws nothing once a host
// detaches the owning cell.
test("two plots in one document mint no id in common", () => {
  const draw = (v) => render_svg(plot(data({ g: ["a", "b", "c"], v }), bar,
    x(col.g), y(col.v), style({ pattern: "hatch" })));
  const a = draw([1, 2, 3]), b = draw([3, 1, 2]);
  const ids = (s) => new Set([...s.matchAll(/id="([^"]+)"/g)].map((m) => m[1]));
  const [ia, ib] = [ids(a), ids(b)];
  assert.ok(ia.size && ib.size, "both plots must mint ids to compare");
  for (const i of ia) assert.ok(!ib.has(i), `two different plots share an id: ${i}`);
  for (const s of [a, b])
    for (const m of s.matchAll(/url\(#([^)]+)\)/g))
      assert.ok(ids(s).has(m[1]), `a reference points outside its own plot: ${m[1]}`);
});

// The same class one language over: `${…}` inside a single-quoted string is a
// literal, so a refusal would print its own template. Source-level for the same
// reason — a substring assertion sits before the template and passes.
test("no refusal prints its own template", () => {
  const src = fs.readFileSync(new URL("../src/atoms.js", import.meta.url), "utf8");
  for (const line of src.split("\n")) {
    if (line.includes("${") && /'[^']*\$\{/.test(line))
      assert.fail(`a single-quoted string carries a template: ${line.trim()}`);
  }
});

// ---------------------------------------------------------------------------
// Accepted and dropped: each of these sentences drew something other than what
// it said, and said nothing. The same block runs in all four bindings.
// ---------------------------------------------------------------------------

const strip = {
  g: ["A", "A", "A", "A", "B", "B", "B", "B"],
  k: ["p", "q", "p", "q", "p", "q", "p", "q"],
  n: [3, 1, 4, 2, 3, 1, 4, 2],
  v: [1, 2, 3, 4, 5, 6, 7, 8],
};
const refusalOf = (thunk) => {
  try {
    thunk();
  } catch (error) {
    assert.ok(error instanceof GogError, `not a GogError: ${error}`);
    return error.message;
  }
  assert.fail("the sentence was accepted, and should have been refused");
};

test("the refusals of 2026-09-23 refuse once, with direction", () => {
  refuses(() => render_svg(plot(data(strip), layer(point, jitter(2)), x(col.g), y(col.v))),
    /`jitter\(1\.25\)`/);
  render_svg(plot(data(strip), layer(point, jitter(1.25)), x(col.g), y(col.v)));
  refuses(() => render_svg(plot(data(strip), layer(bar, sum), x(col.g), y(col.v), order(col.k))),
    /holds text/);
  refuses(() => render_svg(plot(data(strip), point, x(col.n), y(col.v), rule, x(col.n), y(col.v))),
    /`rule \+ x\(n\) \+ rule \+ y\(v\)`/);
  // In a cube `z` is a position like the other two: a rule takes one, and is
  // refused as the plane that one makes.
  refuses(() => render_svg(plot(data(strip), rule, x(col.n), z(col.v))),
    /`rule \+ x\(n\)` or `rule \+ z\(v\)`/);
  refuses(() => render_svg(plot(data(strip), rule, z(col.v))),
    /`rule \+ z\(v\)` in a cube is a \*\*plane\*\*/);
  refuses(() => render_svg(plot(data(strip), point, x(col.n), y(col.v),
    style({ color: "none", border_color: "steelblue", opacity: 0.3 }))), /#4682b44d/);
  refuses(() => render_svg(plot(data(strip), layer(zone, bin), x(col.g), y(col.k))),
    /are both categorical/);
  for (const thunk of [
    () => render_svg(plot(data(strip), layer(path, mean), x(col.g), y(col.v))),
    () => render_svg(plot(data(strip), layer(zone, bin), x(col.g), y(col.k))),
  ]) {
    assert.equal(refusalOf(thunk).split("gog: `").length - 1, 1, "a mistake is refused once");
  }
});

test("a statistic splits by every channel, and a zone takes a share", () => {
  const summed = render_svg(plot(data(strip), layer(zone, sum), x(col.g), y(col.k), color(col.v)));
  const shared = render_svg(
    plot(data(strip), layer(zone, sum, proportion), x(col.g), y(col.k), color(col.v)));
  assert.notEqual(summed, shared, "`proportion` was dropped on a zone");
  const lines = render_svg(
    plot(data(strip), layer(line, mean), x(col.n), y(col.v), color(col.g), group(col.k)));
  assert.equal(lines.split("<polyline").length - 1, 4, "color and group split a mean four ways");
  assert.equal(
    render_svg(plot(data(strip), layer(interval, range), x(col.g), y(col.v), pattern(col.k))),
    render_svg(plot(data(strip), layer(interval, range), x(col.g), y(col.v), pattern(col.k),
      group(col.k))),
    "`pattern` alone should split a statistic");
});

test("a declared order over numbers draws as categories", () => {
  const numbers = { year: ordered([2019, 2020, 2021], [2019, 2020, 2021]), sales: [3, 5, 4] };
  const text = { year: ordered(["2019", "2020", "2021"], ["2019", "2020", "2021"]), sales: [3, 5, 4] };
  assert.equal(
    render_svg(plot(data(numbers, { name: "years" }), bar, x(col.sales), y(col.year))),
    render_svg(plot(data(text, { name: "years" }), bar, x(col.sales), y(col.year))));
});

test("a tally's axis names no column, and a box's refusal is said once", () => {
  refuses(() => render_svg(plot(data(strip), layer(bar, count), x(col.g), y(col.v))),
    /names a column it never reads/);
  refuses(() => render_svg(plot(data(strip), layer(point, bin, stack), x(col.n), y(col.g))),
    /`color\(g\)`/);
  render_svg(plot(data(strip), layer(bar, count), x(col.g), y(col.count)));
  const text = refusalOf(() => render_svg(plot(data(strip), layer(box, mean), x(col.g), y(col.v))));
  assert.equal(text.split("gog: `").length - 1, 1, "`box * mean` should be refused once");
});

// Each of these drew something other than the sentence said, or said nothing
// where it should have spoken: a count a log, calendar or map axis never read, a
// date on `z` in epoch seconds, a page coarsening its plots' ticks, two keys for
// one column, a transparent figure painted white, a name far from its axis, a
// dash an `edge` refused, a label the layout had nowhere to read from.
test("counts on every axis, calendar z, shared ticks, one key, and the rest", () => {
  const labels = (svg) => [...svg.matchAll(/>([^<>]*)<\/text>/g)].map((m) => m[1]);
  const powers = [...Array(25).keys()];
  const big = { i: powers, v: powers.map((k) => 10 ** k) };
  const lab = labels(render_svg(plot(data(big), point, x(col.i), y(col.v, { scale: "log" }))));
  assert.ok(lab.includes("10\u00b9\u00b2") && !lab.some((l) => l.includes("000B")), `${lab}`);

  const wide = { gdp: [277.55, 2000, 12000, 49357.19], life: [40, 55, 70, 82] };
  const logged = (opts = {}) => labels(render_svg(
    plot(data(wide), point, x(col.gdp, { scale: "log", ...opts }), y(col.life))));
  assert.ok(!logged().includes("2K"), `${logged()}`);
  assert.ok(["2K", "5K"].every((l) => logged({ tick_count: 12 }).includes(l)), `${logged({ tick_count: 12 })}`);
  // A derived axis left with two ticks takes a finer step: 13 to 34 had only 20
  // and 30, and now reads 15, 20, 25, 30, 35.
  const twoTicks = labels(render_svg(plot(data({ v: [13, 34], w: [1, 2] }), point, x(col.w), y(col.v))));
  assert.ok(["15", "25", "35"].every((l) => twoTicks.includes(l)), `${twoTicks}`);
  // Days at UTC midnight, so the table reads the same in every zone.
  const days = {
    day: [...Array(42).keys()].map((i) => new Date(Date.UTC(2024, 2, 1 + i))),
    orders: [...Array(42).keys()].map((i) => 20 + (i % 7) * 2),
  };
  const dated = (opts = {}, width = 800) => labels(render_svg(
    plot(data(days), line, x(col.day, opts), y(col.orders), theme({ width }))))
    .filter((l) => /^(Feb|Mar|Apr) /.test(l)).length;
  // Twenty asks the calendar for 22 dates: at 800 pixels they do not fit side by
  // side, so one in every two is drawn; at 1600 all 22 are.
  assert.deepEqual([dated(), dated({ tick_count: 3 }), dated({ tick_count: 20 }),
    dated({ tick_count: 20 }, 1600)], [6, 3, 11, 22]);
  assert.ok(labels(render_svg(plot(data(days), point, x(col.orders), y(col.orders), z(col.day))))
    .includes("Mar 4"), "a date on z is ticked on the calendar");

  const quakes = { east: [165, 170, 175, 180, 185], north: [-35, -30, -25, -20, -15] };
  const degrees = (opts = {}) => labels(render_svg(
    plot(data(quakes), point, x(col.east, opts), y(col.north), map()))).filter((l) => l.endsWith("\u00b0")).length;
  assert.ok(degrees({ tick_count: 20 }) > degrees() + 5, `${degrees({ tick_count: 20 })} vs ${degrees()}`);

  const page = beside(
    plot(data(wide), point, x(col.gdp, { tick_count: 3 }), y(col.life)),
    plot(data(wide), point, x(col.gdp, { tick_count: 12 }), y(col.life)));
  assert.ok(["5K", "15K", "45K"].every((l) => labels(render_svg(page)).includes(l)),
    "a page coarsened its plots' ticks");

  const kinds = { x: [1, 2, 3], y: [1, 2, 3], kind: ["a", "b", "c"] };
  const merged = render_svg(plot(data(kinds), point, x(col.x), y(col.y), color(col.kind), shape(col.kind)));
  assert.equal(labels(merged).filter((l) => l === "Kind").length, 1, "two keys for one column");
  refuses(() => render_svg(plot(data(kinds), point, x(col.x), y(col.y), color(col.kind), palette("gray"))),
    /`shape\(<column>\)`/);
  const clear = render_svg(plot(data(kinds), point, x(col.x), y(col.y), theme({ background: "transparent" })));
  assert.ok(!clear.includes('<rect width="800" height="600" fill="white"/>'), "a transparent figure is white");
  const squared = render_svg(plot(data(kinds), point, x(col.x), y(col.y), theme({ ratio: 1 })));
  assert.ok(Number(/rotate\(-90 ([0-9.]+) /.exec(squared)[1]) > 50, "the y name stayed at the edge");

  refuses(() => render_svg(plot(data(kinds), point, x(col.x), y(col.y),
    style({ shape: "cross", border_color: "red", border_size: 2 }))), /a `cross` is two strokes with no fill/);

  const links = { src: ["a", "a", "b"], dst: ["b", "c", "c"] };
  refuses(() => render_svg(plot(data(links), layer(text, layout(col.src, col.dst)), label(col.src), network())),
    /`label\(name\)`/);
  assert.match(render_svg(plot(data(links), layer(edge, layout(col.src, col.dst)), pattern(col.src), network())),
    /stroke-dasharray/);

  const boxes = { g: ["a", "a", "a", "a", "a", "b", "b", "b", "b", "b"], v: [1, 2, 3, 4, 5, 2, 4, 6, 8, 10] };
  const caps = (on) => render_svg(plot(data(boxes), box, x(col.g), y(col.v), style({ caps: on })))
    .split('stroke-linecap="round"/>').length - 1;
  assert.deepEqual([caps(true), caps(false)], [4, 0], "style({ caps: false }) should leave a box's whiskers bare");
  // Two rows at every x, which is when a stroke zigzags and the note is said.
  const ramp = { x: [0, 1, 2, 3, 0, 1, 2, 3], y: [1, 3, 2, 4, 3, 5, 4, 6], v: [0, 1, 2, 3, 4, 5, 6, 7] };
  const write = process.stderr.write;
  let said = "";
  process.stderr.write = (chunk) => { said += chunk; return true; };
  try {
    render_svg(plot(data(ramp), line, x(col.x), y(col.y), color(col.v)));
  } finally {
    process.stderr.write = write;
  }
  assert.match(said, /`color\(v\)` holds numbers/);
  const heads = render_svg(plot(data(links), layer(edge, layout(col.src, col.dst)), style({ arrow: "end" }), network()));
  assert.equal(heads.split("<polygon").length - 1, 3, "style({ arrow: 'end' }) should put a head on each edge");
});

// A `Date` is an instant with no zone of its own, and the engine draws a clock
// with none. JavaScript sent the instant, so the axis showed the UTC clock: noon
// in Seoul drew as 03:00, where the other three bindings draw the clock time the
// reader wrote. It is read on the session's clock, with no exception: a day is a
// local midnight, `new Date("2024-01-01T00:00")`, which is a day in every zone.
// Asserted in three zones, each in a process of its own.
test("a Date is drawn on the clock the session shows it on, in every zone", () => {
  const index = new URL("../src/index.js", import.meta.url).href;
  const code = `
    import { render_svg, plot, data, line, x, y, col } from ${JSON.stringify(index)};
    const labels = (s) => [...s.matchAll(/>([^<>]*)<\\/text>/g)].map((m) => m[1]);
    const draw = (t) => labels(render_svg(plot(data(t), line, x(col.when), y(col.v))));
    const clock = { when: [new Date(2024, 0, 1, 12), new Date(2024, 0, 1, 16)], v: [1, 2] };
    const days = { when: [new Date("2024-01-01T00:00"), new Date("2024-01-08T00:00")], v: [1, 2] };
    console.log(JSON.stringify([draw(clock), draw(days)]));
  `;
  for (const tz of ["Asia/Seoul", "America/Chicago", "UTC"]) {
    const run = spawnSync(process.execPath, ["--input-type=module", "-e", code],
      { env: { ...process.env, TZ: tz }, encoding: "utf8" });
    assert.equal(run.status, 0, run.stderr);
    const [clock, days] = JSON.parse(run.stdout);
    assert.ok(clock.includes("12:00") && clock.includes("16:00"), `${tz}: ${clock}`);
    assert.ok(days.includes("Jan 1") && !days.some((l) => l.includes(":")), `${tz}: ${days}`);
  }
});

// A partition read as proportion divides its measure axis by the total and keeps
// its other axis's name; a spine plot's share axis runs 0 to 1 instead of -0 to 2;
// the refusal of a proportion beside a filled pile names `stack(share = TRUE)` and a
// way out that draws; and `box * jitter` is refused toward `dodge`.
test("partition * proportion in shares, the spine plot's axis, and the filled pile", () => {
  const labels = (svg) => [...svg.matchAll(/>([^<>]*)<\/text>/g)].map((m) => m[1]);
  const trips = { city: ["A", "A", "B", "B"], mode: ["car", "bus", "car", "bus"],
    people: [30, 10, 20, 40] };
  let lab = labels(render_svg(plot(data(trips), x(col.people),
    layer(zone, partition(col.city, col.mode, { cross: true }), proportion), color(col.mode))));
  const nums = lab.filter((l) => /^-?[0-9.]+$/.test(l)).map(Number);
  assert.ok(lab.includes("Share of column") && !lab.includes("Proportion")
    && nums.length > 0 && Math.max(...nums) <= 1, `${lab}`);
  lab = labels(render_svg(plot(data(trips), x(col.people),
    layer(text, partition(col.city, col.mode), proportion), label(col.name))));
  assert.ok(lab.includes("A") && lab.includes("car"), `${lab}`);
  const spine = labels(render_svg(plot(data(trips),
    layer(zone, partition(col.city, { cross: true })), x(col.people), color(col.city))));
  assert.ok(!spine.includes("-0") && spine.includes("0.2") && spine.includes("1.0"), `${spine}`);
  assert.match(refusalOf(() => render_svg(plot(data(trips),
    layer(bar, stack({ share: true }), proportion), x(col.city), color(col.mode)))),
    /`bar \* count \* stack` with its `share` set to true for shares within each pile/);
  assert.match(refusalOf(() => render_svg(plot(data(trips),
    layer(box, jitter), x(col.city), y(col.people)))), /`dodge` sets them side by side/);
});

// A name written with `y_label()` is drawn where an axis draws no numbers (a moved
// pile, a partition's ring), where it used to be dropped in silence; `jitter(0)`
// draws with a note that it moved nothing; and the `bounds` refusal names `zone`,
// the fifth mark that takes the pair.
test("a written name on an axis with no numbers, jitter(0)'s note, and bounds on a bar", () => {
  const labels = (svg) => [...svg.matchAll(/>([^<>]*)<\/text>/g)].map((m) => m[1]);
  const weeks = { week: [1, 2, 3, 4, 1, 2, 3, 4], plays: [3, 4, 5, 4, 2, 3, 2, 4],
    genre: ["folk", "folk", "folk", "folk", "jazz", "jazz", "jazz", "jazz"] };
  const lab = labels(render_svg(plot(data(weeks), layer(area, stack({ baseline: "wiggle" })),
    x(col.week), y(col.plays), color(col.genre), y_label("Plays per week"))));
  assert.ok(lab.includes("Plays per week"), `${lab}`);
  const strip = { g: ["a", "a", "a", "b", "b", "b"], v: [1, 2, 3, 2, 3, 4] };
  const write = process.stderr.write;
  let said = "";
  process.stderr.write = (chunk) => { said += chunk; return true; };
  try {
    render_svg(plot(data(strip), layer(point, jitter(0)), x(col.g), y(col.v)));
  } finally {
    process.stderr.write = write;
  }
  assert.match(said, /`jitter\(0\)` moves no point/);
  assert.match(refusalOf(() => render_svg(plot(data(strip), layer(bar, bounds(col.v, col.v)),
    x(col.g)))), /`zone` shades the region between them/);
});

// A partition's axes run from 0: they are ticked over the cells, not over the
// cells' centers, so a mosaic's share axis reads 0.0 to 1.0, and a sunburst's
// angle labels 0 once, at the top, where its total would share the spoke.
test("a partition's axes are ticked from 0 over its cells", () => {
  const labels = (svg) => [...svg.matchAll(/>([^<>]*)<\/text>/g)].map((m) => m[1]);
  const trips = { city: ["A", "A", "B", "B"], mode: ["car", "bus", "car", "bus"],
    people: [30, 10, 20, 40] };
  const mosaic = labels(render_svg(plot(data(trips),
    layer(zone, partition(col.city, col.mode, { cross: true })), x(col.people), color(col.mode))));
  for (const t of ["0", "0.0", "1.0"]) assert.ok(mosaic.includes(t), `${t}: ${mosaic}`);
  const sunburst = labels(render_svg(plot(data(trips),
    layer(zone, partition(col.city, col.mode)), x(col.people), color(col.city), polar())));
  assert.equal(sunburst.filter((l) => l === "0").length, 1, `${sunburst}`);
  assert.ok(!sunburst.includes("100"), `${sunburst}`);
});

// A summary whose every group is one row draws the rows themselves, and now says
// so: a key of numbers that never repeat gives one group per row.
test("a summary of one-row groups says so, and a real summary does not", () => {
  const said = (table) => {
    const write = process.stderr.write;
    let text = "";
    process.stderr.write = (chunk) => { text += chunk; return true; };
    try {
      render_svg(plot(data(table), layer(bar, mean), x(col.gdp), y(col.life)));
    } finally {
      process.stderr.write = write;
    }
    return text;
  };
  const spread = said({ gdp: [1.5, 2.5, 3.5, 4.5], life: [50, 60, 70, 80] });
  assert.match(spread, /every group holds a single row/);
  assert.match(spread, /`bar \* bin \* mean`/);
  assert.doesNotMatch(said({ gdp: [1, 1, 2, 2], life: [50, 60, 70, 80] }),
    /every group holds a single row/);
});

// Every plot is a named image for a screen reader: `role="img"` and an
// `aria-label`, the title when there is one, else what the plot draws.
test("every plot carries an accessible name", () => {
  const pts = { gdp: [1, 2, 3], life: [50, 60, 70] };
  assert.ok(render_svg(plot(data(pts), point, x(col.gdp), y(col.life)))
    .includes('role="img" aria-label="Points, x is gdp, y is life"'));
  assert.ok(render_svg(plot(data(pts), point, x(col.gdp), y(col.life), title("Longer lives")))
    .includes('aria-label="Longer lives"'));
});

// A color name means one color: a palette ramp through "green" ends at CSS's
// green, the one a set `style({ color: "green" })` shows, not X11's #00FF00.
test("a palette's green is CSS's green", () => {
  const d = { a: [1, 2, 3, 4, 5], b: [1, 2, 3, 4, 5], v: [1, 2, 3, 4, 5] };
  const svg = render_svg(plot(data(d), point, x(col.a), y(col.b), color(col.v),
    palette(["white", "green"])));
  assert.ok(svg.includes("#008000") && !svg.toLowerCase().includes("#00ff00"));
});

// A page carries only the columns its plot names: the engine prunes the request
// a brushed plot embeds, so an unmapped column never reaches the page.
test("a page carries only the columns its plot names", () => {
  const people = { gdp: [1000, 20000, 40000], life: [50, 70, 80],
    email: ["a@x.org", "b@x.org", "c@x.org"] };
  const block = html_block(plot(data(people), point, x(col.gdp), y(col.life),
    brush(col.gdp, { at: [2000, 30000] })));
  if (!block.includes("<script")) return; // the browser engine is not built here
  assert.ok(!block.includes("a@x.org") && !block.includes('"email"'), "an unmapped column was published");
  assert.ok(block.includes('"gdp"') && block.includes('"life"'), "a mapped column was dropped");
});

// `smooth_band` is the band around a `smooth` line: a ribbon fills it, its level
// reaches the engine, and a level outside (0, 1) is refused.
test("smooth_band draws the band around smooth, at its level", async () => {
  const { smooth_band, smooth, ribbon: rb } = await import("../src/index.js");
  const g = Array.from({ length: 30 }, (_, i) => i + 1);
  const pts = { g, v: g.map((i) => Math.sin(i / 5) * 3 + (i % 4)) };
  const band = (...args) => render_svg(plot(data(pts),
    layer(rb, smooth_band(...args)), x(col.g), y(col.v),
    layer(line, smooth), x(col.g), y(col.v)));
  assert.ok(band().includes("<polygon") && band().includes("Ribbons derived by smooth_band"));
  assert.notEqual(band(0.5), band(0.99), "the level did not reach the engine");
  refuses(() => smooth_band(1.5), /strictly between 0 and 1/);
  // The band needs five rows where its curve needs three: below five every local
  // fit passes through its own points and leaves nothing to measure a width from.
  const four = { g: g.slice(0, 4), v: pts.v.slice(0, 4) };
  const thin = () => render_svg(plot(data(four), layer(rb, smooth_band()), x(col.g), y(col.v)));
  refuses(thin, /`smooth_band` fits a curve/);
  refuses(thin, /at least 5/);
  refuses(thin, /`line \* smooth`/);
});

// The `bar * dodge` refusal names the channels a bar takes, and `pattern`
// divides a one-slot bar as `color` does.
test("a bar is split, and advised, by the channels it takes", async () => {
  const { dodge } = await import("../src/index.js");
  const eras = { continent: ["Asia", "Asia", "Europe", "Europe"], era: ["1957", "2007", "1957", "2007"],
    life: [50, 60, 65, 75] };
  assert.match(refusalOf(() => render_svg(plot(data(eras), layer(bar, mean, dodge),
    x(col.continent), y(col.life)))), /Add `color\(<field>\)` or `pattern\(<field>\)`/);
  assert.ok(render_svg(plot(data(eras), layer(bar, count, stack), pattern(col.era))).includes("<rect"));
});

// Law 7's second half: positions with no mark are refused, as a mark with no
// positions always was, and told which mark to add. Each drew an empty panel with
// made-up axes. The same block runs in all four bindings.
test("a plot with no mark is refused, with the mark to add", () => {
  const quiet = { gdp: [1, 2, 3], life: [4, 5, 6], continent: ["Asia", "Europe", "Africa"] };
  assert.match(refusalOf(() => render_svg(plot(data(quiet), x(col.gdp), y(col.life)))),
    /`point` draws a dot for each row/);
  assert.match(refusalOf(() => render_svg(plot(data(quiet), x(col.gdp)))),
    /`bar \* bin` shows how the values of `gdp` are spread/);
  assert.match(refusalOf(() => render_svg(plot(data(quiet), y(col.continent)))),
    /`bar \* count` counts the rows for each value of `continent`/);
  assert.match(refusalOf(() => render_svg(plot(data(quiet)))), /this plot has no mark/);
  assert.ok(render_svg(plot(data(quiet), layer(bar, bin), x(col.gdp))).includes("<rect"));
});

// An `edge` with no `layout` has nothing to draw: its geometry is the two nodes a
// layout places. It drew an empty panel with made-up axes, in silence. The same
// block runs in all four bindings.
test("an edge with no layout is refused, with the sentence that draws it", () => {
  const links = { from: ["a", "b", "c"], to: ["b", "c", "a"] };
  assert.match(refusalOf(() => render_svg(plot(data(links), edge))),
    /an `edge` with no `layout` has nothing to draw/);
  assert.match(refusalOf(() => render_svg(plot(data(links), edge, color(col.from)))),
    /`edge \* layout\(<from>, <to>\) \+ network\(\)`/);
  assert.ok(render_svg(plot(data(links), layer(edge, layout(col.from, col.to)), network())).includes("<svg"));
});

// `bounds`, `partition` and a cluster tree place things in the plane, and each
// stood a bare `space()` in an empty cube. It is drawn flat now, and said, as over
// every other flat plot. The same block runs in all four bindings.
test("a bare space() over a transform in the plane is drawn flat", () => {
  const spans = { t: [1, 2, 3, 4], lo: [1, 2, 3, 4], hi: [2, 3, 5, 6] };
  const flat = render_svg(plot(data(spans), layer(zone, bounds(col.lo, col.hi)), x(col.t)));
  const cube = render_svg(plot(data(spans), layer(zone, bounds(col.lo, col.hi)), x(col.t), space()));
  assert.equal(cube, flat);
});

// A smoothed point in the cube lost its third position and drew an empty cube,
// in silence. It is refused as every smooth in the cube is. The same block runs
// in all four bindings.
test("a smoothed point in the cube is refused toward the plane", () => {
  const drift = { g: [1.5, 2.5, 3.1, 4.2, 5.3, 6.1], h: [3, 1, 4, 1, 5, 9], n: [1, 2, 3, 4, 5, 6] };
  assert.match(refusalOf(() => render_svg(plot(data(drift), layer(point, smooth),
    x(col.g), y(col.h), z(col.n)))), /`bar \* mean \+ x\(<a>\) \+ y\(<b>\) \+ z\(<column>\) \+ space\(\)`/);
  assert.ok(render_svg(plot(data(drift), layer(point, smooth), x(col.g), y(col.h))).includes("<svg"));
});

// Stages are placed by their column's name, so a second `class` fell on the first,
// with no message. The same block runs in all four bindings.
test("a flow stage named twice is refused", () => {
  const voyage = { class: ["1st", "2nd", "3rd", "1st"], fate: ["lived", "died", "died", "lived"] };
  assert.match(refusalOf(() => render_svg(plot(data(voyage), layer(zone, flow(col.class, col.class))))),
    /`class` is named twice in `flow\(\)`/);
  assert.match(refusalOf(() => render_svg(plot(data(voyage),
    layer(ribbon, flow(col.class, col.fate, col.class))))), /Name each column once/);
  assert.ok(render_svg(plot(data(voyage), layer(zone, flow(col.class, col.fate)))).includes("<svg"));
});

// A tally's `y`, split and stacked with no `x`, is the key of a bar on its side, as
// it is unsplit. The one-pile rule swallowed it, and the counts were drawn in one
// pile under an axis titled after the column. The same block runs in all four.
test("a split tally with a key on y lies on its side", () => {
  const winds = { dir: ["N", "S", "N", "S"], season: ["win", "win", "sum", "sum"], speed: [3, 5, 2, 8] };
  const sideways = render_svg(plot(data(winds), layer(bar, count, stack), y(col.season), color(col.dir)));
  assert.ok(sideways.includes(">win<") && sideways.includes(">Count<"),
    "a stacked tally with a category on y should lie on its side");
  assert.ok(render_svg(plot(data(winds), layer(bar, sum, stack), y(col.speed), color(col.dir))).includes("<rect"));
});

// A network row with a missing end is left out, and said so, but its named end was
// still added as a node, standing alone. The same block runs in all four bindings.
test("a network row left out adds no node", () => {
  const links = { from: ["a", "a", "b", "c"], to: ["b", "c", "c", "d"] };
  const extra = { from: ["a", "a", "b", "c", "e"], to: ["b", "c", "c", "d", null] };
  const draw = (t) => render_svg(plot(data(t, { name: "links" }), layer(edge, layout(col.from, col.to)),
    layer(point, layout(col.from, col.to)), network()));
  assert.equal(draw(extra), draw(links));
});

// After a `bar`, a second table's points past the last bar were placed beyond the
// panel and clipped away, in silence. The same block runs in all four bindings.
test("a second table's points after a bar are drawn inside the panel", () => {
  const actuals = { year: [2019, 2020, 2021, 2022, 2023], sales: [3, 4, 5, 4, 6] };
  const forecast = { year: [2024, 2025, 2026], sales: [6.5, 7, 7.4] };
  const svg = render_svg(plot(data(actuals, { name: "actuals" }), x(col.year), y(col.sales), bar,
    data(forecast, { name: "forecast" }), point));
  const cx = [...svg.matchAll(/cx="([0-9.]+)/g)].map((m) => Number(m[1]));
  assert.equal(cx.length, 3);
  assert.ok(cx.every((v) => v <= 800), `a forecast point is off the canvas: ${cx}`);
  assert.ok(svg.includes(">2026<"), "the axis should reach 2026");
});

// A one-column `rule` table on a map kept its raw degrees, and a parallel at 45
// collapsed the map to a sliver. It is placed now exactly as the same rule with both
// columns. The same block runs in all four bindings.
test("a one-column rule on a map is projected", () => {
  const pts = { lon: [-10, 0, 10, 5], lat: [30, 50, 60, 40] };
  const par = (t) => render_svg(plot(data(pts, { name: "pts" }), point, x(col.lon), y(col.lat),
    data(t, { name: "par" }), rule, y(col.lat), map()));
  assert.equal(par({ lat: [45] }), par({ lon: [0], lat: [45] }));
});

// A plot splits once across and once down, so a second split in one direction
// replaced the first, in silence. The same block runs in all four bindings;
// JavaScript cannot spell a pair of facets with no plot.
test("a second facet in one direction is refused", () => {
  const split = { a: [1, 2, 3, 4], b: [1, 2, 3, 4], g: ["p", "q", "p", "q"], h: ["u", "u", "v", "v"] };
  assert.match(refusalOf(() => plot(data(split), point, x(col.a), y(col.b), across(col.g), across(col.h))),
    /already split into panel columns by `g`/);
  assert.match(refusalOf(() => plot(data(split), point, x(col.a), y(col.b), down(col.g), down(col.h))),
    /already split into panel rows by `g`/);
  assert.ok(render_svg(plot(data(split), point, x(col.a), y(col.b), across(col.g), down(col.h))).includes("<svg"));
});

// Each space word replaced the one before it, so the engine saw only the last:
// `space(), polar()` drew polar, in silence. A second, different space is refused
// now; restating one re-angles it. The same block runs in all four bindings.
test("a second coordinate space is refused, and a restated one re-angles", () => {
  const view = { a: [1, 2, 3, 4], b: [2, 3, 1, 4], c: [1, 2, 3, 4] };
  assert.match(refusalOf(() => plot(data(view), point, x(col.a), y(col.b), space(), polar())),
    /already drawn in `space\(\)`, and `polar\(\)` asks for a second space/);
  assert.match(refusalOf(() => plot(data(view), point, x(col.a), y(col.b), polar(), space(), z(col.c))),
    /already drawn in `polar\(\)`, and `space\(\)` asks for a second space/);
  assert.match(refusalOf(() => plot(data(view), point, x(col.a), y(col.b), map(), polar())),
    /already drawn in `map\(\)`/);
  assert.ok(render_svg(plot(data(view), point, x(col.a), y(col.b), z(col.c), space(), space({ turn: 60 }))).includes("<svg"));
});

// A page shares an axis by column, and it merged two plots that read the column
// through different scales, in silence. The same block runs in all four bindings.
test("a page refuses one column read through two scales", () => {
  const wealth = { gdp: [1000, 2000, 5000, 20000, 40000], life: [50, 60, 65, 72, 80] };
  assert.match(refusalOf(() => render_svg(below(plot(data(wealth), layer(bar, bin), x(col.gdp)),
    plot(data(wealth), point, x(col.gdp, { scale: "log" }), y(col.life))))),
    /`gdp` is on the x axis of two plots on this page/);
  assert.ok(render_svg(below(plot(data(wealth), layer(bar, bin), x(col.gdp, { scale: "log" })),
    plot(data(wealth), point, x(col.gdp, { scale: "log" }), y(col.life)))).includes("<svg"));
});

// A cluster tree under a bare `space()` is drawn flat, as every plot with no third
// dimension is. The same block runs in all four bindings.
test("a tree under a bare space() is drawn flat", () => {
  const menu = { food: ["a", "a", "b", "b", "c", "c"], nutrient: ["p", "q", "p", "q", "p", "q"],
    amount: [1, 2, 1.5, 2.5, 5, 1] };
  const tree = (...extra) => render_svg(plot(data(menu),
    layer(path, cluster(col.amount, { over: col.nutrient })), x(col.food), ...extra));
  assert.equal(tree(space()), tree());
});

// A border on `text` is a halo under its letters: the glyph's outline stroked
// beneath its fill, so a name written over a line reads clearly. It was refused
// until 2026-09-26 ("a `text` mark draws glyphs, not a filled shape").
test("a border on `text` is a halo under its letters", () => {
  const haloT = { x: [1, 2, 3], y: [1, 3, 2], name: ["a", "b", "c"] };
  const draw = (...extra) => render_svg(plot(data(haloT, { name: "halo_t" }), x(col.x), y(col.y),
    line, text, label(col.name), ...extra));
  assert.match(draw(style({ border_color: "white", border_size: 5 })),
    /fill="none" stroke="white" stroke-width="5"/, "a set border draws a halo");
  assert.doesNotMatch(draw(), /text-anchor="middle" fill="none" stroke="/, "a label with no border has no halo");
});

// One density layer is smoothed by one bandwidth: the mean of the bandwidths its
// groups would each choose alone. So two groups of one shape draw one violin,
// whatever their row counts. Until 2026-09-26 each group took its own, and a
// group with every row written twice was drawn sharper than its twin.
test("a density layer smooths every group by one bandwidth", () => {
  const v = [1, 2, 2.5, 4, 4.2, 5, 7, 7.5, 9, 12];
  const sameShape = { g: [...Array(10).fill("once"), ...Array(20).fill("twice")], v: [...v, ...v, ...v] };
  const svg = render_svg(plot(data(sameShape, { name: "same_shape" }),
    layer(ribbon, density({ compare: "shape" })), x(col.g), y(col.v)));
  const heights = [...svg.matchAll(/<polygon points="([^"]*)"/g)]
    .map((m) => m[1].split(" ").map((p) => p.split(",")[1]));
  assert.equal(heights.length, 2, "one violin per group");
  assert.deepEqual(heights[0], heights[1], "two groups of one shape draw one violin, whatever their row counts");
});

// `point * dodge` is the beeswarm: each point in a category moves across its slot
// only as far as it must to clear the others, and never along the measure axis.
// Six rows at one value must land at six places. It was refused toward `jitter`
// until 2026-09-26.
test("point * dodge sets tied points apart across the slot only", () => {
  const tied = { g: Array(6).fill("a"), v: Array(6).fill(1) };
  const svg = render_svg(plot(data(tied, { name: "tied" }), layer(point, dodge), x(col.g), y(col.v)));
  const dots = [...svg.matchAll(/<circle cx="([0-9.]+)" cy="([0-9.]+)"/g)];
  assert.equal(dots.length, 6, "six points");
  assert.equal(new Set(dots.map((m) => m[1])).size, 6, "six tied points at six places");
  assert.equal(new Set(dots.map((m) => m[2])).size, 1, "no point moves along the measure axis");
});

// A network draws its separate parts at one scale: a pair's one edge is about as
// long as the edges of a large ring beside it. Until 2026-09-26 each part was
// stretched to fill a cell sized by its node count, which drew a pair as a long
// stroke beside short ring edges.
test("a network draws its separate parts at one scale", () => {
  const ring = Array.from({ length: 24 }, (_, i) => `r${String(i).padStart(2, "0")}`);
  const parts = { a: [...ring, "a0", "a1", "a2", "a3"], b: [...ring.slice(1), ring[0], "b0", "b1", "b2", "b3"] };
  const svg = render_svg(plot(data(parts, { name: "parts" }), layer(edge, layout(col.a, col.b)), network()));
  const len = [...svg.matchAll(/<line x1="([0-9.]+)" y1="([0-9.]+)" x2="([0-9.]+)" y2="([0-9.]+)"/g)]
    .map((m) => Math.hypot(m[3] - m[1], m[4] - m[2]));
  assert.equal(len.length, 28, "24 ring edges and 4 pairs");
  const mean = len.slice(0, 24).reduce((s, v) => s + v, 0) / 24;
  for (const l of len.slice(24)) {
    assert.ok(l / mean > 0.5 && l / mean < 2, `a pair's edge is drawn ${(l / mean).toFixed(2)} times the ring's`);
  }
});

// A played column's frames are held in proportion to the gap to the next value:
// 2002 stands for 18 years before 2020 and is held four times the usual 0.8s,
// the longest a frame is held. Until 2026-09-26 every frame held 0.8s.
test("a played column's frames are held in proportion to its gaps", () => {
  const uneven = { a: [1, 2, 3, 4], b: [4, 3, 2, 1], t: [2000, 2001, 2002, 2020] };
  const svg = render_svg(plot(data(uneven, { name: "uneven" }), point, x(col.a), y(col.b), play(col.t)));
  const begins = [...new Set([...svg.matchAll(/begin="([0-9.]+)s"/g)].map((m) => Number(m[1])))].sort((p, q) => p - q);
  assert.deepEqual(begins, [0, 0.8, 1.6, 4.8]);
});

// Category names that do not fit side by side are turned to read upward, and when
// even turned they are too close, one in every few is drawn and the engine says
// how many. Until 2026-09-26 forty names printed over each other, and so did
// three hundred.
test("crowded category names turn, then thin, and say so", () => {
  const crowd = (n) => ({
    g: Array.from({ length: n }, (_, i) => `name ${String(i + 1).padStart(3, "0")}`),
    v: Array.from({ length: n }, (_, i) => i + 1),
  });
  const draw = (n) => {
    const write = process.stderr.write;
    let said = "";
    process.stderr.write = (chunk) => { said += chunk; return true; };
    try {
      const svg = render_svg(plot(data(crowd(n), { name: "crowd" }), bar, x(col.g), y(col.v)));
      return { svg, said };
    } finally {
      process.stderr.write = write;
    }
  };
  const forty = draw(40);
  assert.equal(forty.svg.split("rotate(-90.00 ").length - 1, 40, "forty names, every one turned");
  assert.equal(forty.said, "", "turning names that fit is silent");
  const many = draw(300);
  const drawn = [...many.svg.matchAll(/>name [0-9]{3}<\/text>/g)].length;
  assert.ok(drawn < 300, "three hundred names are thinned");
  assert.ok(many.said.includes(`(${drawn} of 300)`), many.said);
});

// A treemap's label report says "do not fit", since a name can fail on height as
// well as width; it agrees in number; and it calls the packing whole only when
// every share has a region. The same block runs in all four bindings.
test("a treemap's label report agrees in number, and calls the packing whole only when it is", () => {
  const saidBy = (p) => {
    const write = process.stderr.write;
    let said = "";
    process.stderr.write = (chunk) => { said += chunk; return true; };
    try { render_svg(p); } finally { process.stderr.write = write; }
    return said;
  };
  const one = { g: ["roomy", "cramped"], v: [240, 1] };
  const tiny = { g: ["big", "mid", "gone"], v: [1e9, 5e8, 1] };
  const once = saidBy(plot(data(one), bar, y(col.v), color(col.g), text, label(col.g), nest()));
  assert.ok(once.includes("1 of 2 labels are drawn — one does not fit inside the region it names"), once);
  const lost = saidBy(plot(data(tiny), bar, y(col.v), color(col.g), text, label(col.g), nest()));
  assert.ok(lost.includes("one share is too small to have a region at all"), lost);
  assert.ok(!lost.includes("drew every share"), lost);
});

// An axis holds one kind of value: two tables that give one position column two
// kinds are refused, and the message names each table. Each drew its numbers as
// categories' places, off the plot, with nothing said. The same block runs in
// all four bindings.
test("an axis holds one kind of value, whichever table a layer reads", () => {
  const cats = { g: ["a", "b", "c"], v: [1, 2, 3] };
  const nums = { g: [10], v: [2], t: ["ten"] };
  const pts = { g: [1, 2, 3], v: [1, 2, 3] };
  const lab = { g: ["b"], v: [2], t: ["bee"] };
  assert.match(refusalOf(() => render_svg(plot(data(cats, { name: "cats" }), bar, x(col.g), y(col.v),
    data(nums, { name: "nums" }), text, label(col.t)))),
    /`g` holds text in `cats` \(read by `bar`\), and `g` holds numbers in `nums` \(read by `text`\)/);
  assert.match(refusalOf(() => render_svg(plot(data(pts, { name: "pts" }), point, x(col.g), y(col.v),
    data(lab, { name: "lab" }), text, label(col.t)))), /the `x` axis is read two ways/);
  assert.ok(render_svg(plot(data(cats, { name: "cats" }), bar, x(col.g), y(col.v),
    data(lab, { name: "lab" }), text, label(col.t))).includes(">bee</text>"));
});

// A map is not brushed: it projects longitude and latitude before it draws, so a
// range in degrees is no rectangle on the page, and a selection counted one set
// of rows while it dimmed another. The refusal points at a flat plot of the same
// columns, which draws. The same block runs in all four bindings.
test("a map is not brushed, and the flat plot it points to draws", () => {
  const places = { lon: [-10, 0, 20], lat: [40, 50, 60] };
  assert.match(refusalOf(() => render_svg(plot(data(places), point, x(col.lon), y(col.lat), map(),
    brush(col.lon, { at: [-5, 25] })))), /`brush` cannot select on a `map\(\)`/);
  assert.match(refusalOf(() => render_svg(plot(data(places), point, x(col.lon), y(col.lat), map(),
    brush))), /`point \+ x\(lon\) \+ y\(lat\) \+ brush\(lon\)`/);
  assert.ok(render_svg(plot(data(places), point, x(col.lon), y(col.lat),
    brush(col.lon, { at: [-5, 25] }))).includes("<circle"));
});

// A plot draws one color legend, so its layers map `color` from one column. A
// second column's colors reached the reader with no key; it is refused, with a
// channel of its own that draws a key. The same column on every layer, and a
// legend turned off on purpose, still draw. The same block runs in all four
// bindings.
test("a second color column is refused, with a channel of its own", () => {
  const asia = { year: [2000, 2001, 2000, 2001], life: [60, 61, 70, 71],
    country: ["A", "A", "B", "B"], continent: ["Asia", "Asia", "Asia", "Asia"] };
  assert.match(refusalOf(() => render_svg(plot(data(asia), x(col.year), y(col.life), point,
    color(col.country), line, color(col.continent)))), /`pattern\(continent\)` on the `line`/);
  assert.ok(render_svg(plot(data(asia), x(col.year), y(col.life), point, color(col.country),
    line, color(col.country))).includes("<circle"));
  assert.ok(render_svg(plot(data(asia), x(col.year), y(col.life), point, color(col.country),
    line, color(col.continent, { legend: false }))).includes("<circle"));
});

// Crowded numbers thin as names do: five narrow panels printed 0K to 50K through
// each other, and now keep every other number, the ones a coarser step would
// choose, with nothing said. A `tick_count` the caller wrote that is thinned is
// said out loud. The same block runs in all four bindings.
test("crowded numbers thin, and a written tick_count says so", () => {
  const labels = (svg) => [...svg.matchAll(/>([^<>]*)<\/text>/g)].map((m) => m[1]);
  const crowd = { gdp: [300, 12000, 25000, 49000, 900, 30000],
    life: [45, 60, 70, 80, 50, 75], g: ["a", "b", "c", "d", "e", "a"] };
  const lab = labels(render_svg(plot(data(crowd), point, x(col.gdp), y(col.life), across(col.g))));
  assert.ok(["20K", "40K"].every((l) => lab.includes(l)) && !lab.includes("10K") && !lab.includes("30K"),
    lab.join(" "));
  const write = process.stderr.write;
  let said = "";
  process.stderr.write = (chunk) => { said += chunk; return true; };
  try {
    render_svg(plot(data(crowd), point, x(col.gdp, { tick_count: 12 }), y(col.life), across(col.g)));
  } finally {
    process.stderr.write = write;
  }
  assert.ok(said.includes("tick_count = 12") && said.includes("so 3 of them are drawn"), said);
});

// A negative weight has no share and no thickness, so `partition` and `flow`
// refuse it as `nest()` does. Both clamped it to 0 and drew parts that no longer
// summed to the table, with nothing said. The same block runs in all four
// bindings.
test("a negative weight is refused by a partition and a flow", () => {
  const neg = { a: ["p", "p", "q"], b: ["u", "v", "u"], w: [3, -2, 4] };
  assert.match(refusalOf(() => render_svg(plot(data(neg), layer(zone, partition(col.a, col.b)),
    x(col.w)))), /`x\(w\)` weighs each branch/);
  assert.match(refusalOf(() => render_svg(plot(data(neg), layer(ribbon, flow(col.a, col.b)),
    y(col.w)))), /`y\(w\)` weighs each path/);
});

// A `y` under `partition` was ignored as data and printed as the axis title; a
// partition reads its weight from `x`, so `y` is refused toward it. The same
// block runs in all four bindings.
test("a y under a partition is refused toward x", () => {
  const trips = { city: ["A", "A", "B"], mode: ["car", "bus", "car"], people: [3, 2, 4] };
  assert.match(refusalOf(() => render_svg(plot(data(trips), layer(zone, partition(col.city, col.mode)),
    y(col.people)))), /`y\(people\)` has no reading under `partition`/);
});

// A shared axis lines up across a page, so two plots sharing it are split into
// panels the same way along it: a histogram over a faceted scatter spanned every
// panel and lined up with none. Both split the same way still draw. The same
// block runs in all four bindings.
test("a shared axis is split into panels the same way on a page", () => {
  const g = { gdp: [1, 2, 3, 4, 5, 6], life: [50, 55, 60, 65, 70, 75],
    continent: ["A", "A", "B", "B", "C", "C"] };
  assert.match(refusalOf(() => render_svg(below(plot(data(g), layer(bar, bin), x(col.gdp)),
    plot(data(g), point, x(col.gdp), y(col.life), across(col.continent))))),
    /split into panels differently/);
  assert.ok(render_svg(below(plot(data(g), layer(bar, bin), x(col.gdp), across(col.continent)),
    plot(data(g), point, x(col.gdp), y(col.life), across(col.continent)))).includes("<circle"));
});

// `bounds` draws its axis from its own two columns, so a category named on it
// was never read and one band combed through every group; refused, with the
// splits that draw a band per group. The same block runs in all four bindings.
test("a category on the axis bounds draws is refused", () => {
  const rid = { at: [1, 2, 1, 2], zero: [0, 0, 0, 0], height: [1, 3, 2, 1], g: ["a", "a", "b", "b"] };
  assert.match(refusalOf(() => render_svg(plot(data(rid), layer(ribbon, bounds(col.zero, col.height)),
    x(col.at), y(col.g)))), /`ribbon \* bounds` draws its band/);
});

// `area` and `step` join their rows in x order, as `line` does, and rows that
// share one `x` zigzag inside it; that is now said, and only when it happens.
// The same block runs in all four bindings.
test("an area through rows at one x says it zigzags", () => {
  const zig = { c: ["a", "a", "a", "b", "b"], v: [1, 3, 2, 4, 1] };
  const saidBy = (p) => {
    const write = process.stderr.write;
    let said = "";
    process.stderr.write = (chunk) => { said += chunk; return true; };
    try { render_svg(p); } finally { process.stderr.write = write; }
    return said;
  };
  assert.ok(saidBy(plot(data(zig), area, x(col.c), y(col.v))).includes("zigzags inside that value"));
  assert.ok(!saidBy(plot(data(zig), layer(area, mean), x(col.c), y(col.v))).includes("zigzags"));
  // `line` asks the same question, and one long series is not a zigzag.
  assert.ok(saidBy(plot(data(zig), line, x(col.c), y(col.v))).includes("zigzags inside that value"));
  const one = { day: [...Array(40).keys()], v: [...Array(40).keys()].map((i) => i % 7) };
  assert.equal(saidBy(plot(data(one), line, x(col.day), y(col.v))), "");
});

// A summarized point reads its orientation as a bar does: with the category on
// `y` it summarizes along `x`, one dot per category, where it drew every row.
// The same block runs in all four bindings.
test("a summarized point lies on its side as a bar does", () => {
  const side = { life: [40, 50, 60, 45, 55, 65], year: ["a", "a", "a", "b", "b", "b"] };
  const svg = render_svg(plot(data(side), layer(point, median), x(col.life), y(col.year)));
  assert.equal((svg.match(/<circle/g) || []).length, 2);
});

// A flow's bands are fills, so `pattern` hatches them; the legend drew the hatch
// while every band stayed solid. The same block runs in all four bindings.
test("a flow band draws the pattern its legend shows", () => {
  const fl = { a: ["p", "p", "q", "q"], b: ["u", "v", "u", "v"], n: [3, 2, 4, 1] };
  const svg = render_svg(plot(data(fl), layer(ribbon, flow(col.a, col.b)), y(col.n), pattern(col.a)));
  assert.match(svg, /<path d="M [^"]*" fill="url\(#/);
});

// A flow that leaves a category's rows out for a missing stage draws no band for
// it, so the legend keys it no more. The same block runs in all four bindings.
test("a legend keys only the categories the plot colored", () => {
  const gone = { a: ["p", "p", "q", "gone"], b: ["u", "v", "u", null], n: [3, 2, 4, 5] };
  const write = process.stderr.write;
  process.stderr.write = () => true;
  let svg;
  try {
    svg = render_svg(plot(data(gone), layer(ribbon, flow(col.a, col.b)), y(col.n), color(col.a)));
  } finally {
    process.stderr.write = write;
  }
  assert.ok(!svg.includes(">gone</text>") && svg.includes(">q</text>"));
});

// Under `repel` an empty label names nothing: no text and no leader line, where
// it drew a leader to nothing. The same block runs in all four bindings.
test("an empty label under repel draws no text and no leader", () => {
  const emp = { x: [...Array(12).keys()].map((i) => i % 4),
    y: [...Array(12).keys()].map((i) => Math.floor(i / 4) * 0.2),
    lab: ["named", ...Array(11).fill("")] };
  const svg = render_svg(plot(data(emp), point, x(col.x), y(col.y), layer(text, repel), label(col.lab)));
  assert.ok((svg.match(/stroke-width="0.7"/g) || []).length <= 1 && !svg.includes("></text>"));
});

// A pattern key over a different `color` column takes neutral ink, not the color
// map's first hue, which read as an association with that category. The same
// block runs in all four bindings.
test("a pattern key beside another color column takes neutral ink", () => {
  const pk = { c: ["a", "a", "b", "b"], sex: ["m", "f", "m", "f"], kept: ["yes", "no", "no", "yes"],
    n: [3, 2, 4, 1] };
  const svg = render_svg(plot(data(pk), layer(bar, sum, dodge), x(col.c), y(col.n), color(col.sex),
    pattern(col.kept)));
  assert.ok(svg.split("Kept")[1].includes("#3c3c46"));
});

// A facet panel draws its share of a layer on the whole layer's size scale, the
// one the legend decodes, so the low panel's largest dot is not drawn at the
// largest size. The same block runs in all four bindings.
test("a facet panel reads the whole layer's size scale", () => {
  const sh = { x: [1, 2, 3, 1, 2, 3], y: [1, 2, 3, 2, 3, 4], n: [1, 2, 3, 10, 20, 30],
    g: ["low", "low", "low", "high", "high", "high"] };
  const radii = (svg) => [...svg.matchAll(/<circle[^>]* r="([^"]*)"/g)].map((m) => m[1]).sort();
  const whole = radii(render_svg(plot(data(sh), point, x(col.x), y(col.y), size(col.n))));
  const faceted = radii(render_svg(plot(data(sh), point, x(col.x), y(col.y), size(col.n),
    across(col.g))));
  assert.equal(whole.length, 9);
  assert.deepEqual(faceted, whole);
});

// A dot under a summary is sized by its group's mean, so the size key decodes the
// means rather than the raw column. The same block runs in all four bindings.
test("a size key under a summary reads the summaries", () => {
  const svg = render_svg(plot(data({ g: ["a", "a", "b", "b"], v: [10, 30, 50, 70] }),
    layer(point, mean), x(col.g), y(col.v), size(col.v)));
  // The key's top row and the axis's top tick both read 60; the raw column's ends,
  // 10 and 70, appear nowhere.
  assert.equal(svg.split(">60</text>").length - 1, 2);
  assert.ok(!svg.includes(">10</text>") && !svg.includes(">70</text>"));
});

// A legend that would leave its panel narrower than itself is left out and said,
// where the panel of a thin plot went to a negative width in silence. The same
// block runs in all four bindings.
test("a legend too wide for its plot is left out and said", () => {
  const lw = { x: [1, 2, 3, 4], y: [1, 2, 3, 4], g: ["a long category", "another long one", "a", "b"] };
  const write = process.stderr.write;
  let said = "";
  process.stderr.write = (chunk) => { said += chunk; return true; };
  let svg;
  try {
    svg = render_svg(beside(plot(data(lw), point, x(col.x), y(col.y)),
      plot(data(lw), point, x(col.x), y(col.y), color(col.g), theme({ width: 150 }))));
  } finally { process.stderr.write = write; }
  assert.ok(said.includes("`G` legend needs"), said);
  assert.ok(!svg.includes(">G</text>"));
});

// A y name goes where its panel went: in `(a | b) / c` with `c` sharing a column
// with `b`, the page moves `c`'s panel to run under `b`, and its name stayed at the
// cell's edge, 450px away. The same block runs in all four bindings.
test("a y name follows a panel a shared column moved", () => {
  const yn = { speed: [4, 7, 8, 12, 15, 18, 20, 24], dist: [2, 4, 16, 24, 36, 56, 64, 120] };
  const svg = render_svg(below(
    beside(plot(data(yn), point, x(col.dist), y(col.speed)),
      plot(data(yn), point, x(col.speed), y(col.dist))),
    plot(data(yn), layer(bar, bin), x(col.speed))));
  const at = svg.match(/rotate\(-90 ([0-9.]+) [^)]*\)[^>]*>Count<\/text>/);
  assert.ok(at && Number(at[1]) > 300, String(at && at[1]));
});

// Two folded facets stacked on a shared column keep their own width: each was
// squeezed into one column's width, the rest of its cell empty. The same block
// runs in all four bindings.
test("a folded facet sharing a column keeps its width", () => {
  const fw = { speed: [4, 7, 8, 12, 15, 18], dist: [2, 4, 16, 24, 36, 56], g: ["a", "b", "c", "a", "b", "c"] };
  const svg = render_svg(below(
    plot(data(fw), point, x(col.speed), y(col.dist), across(col.g, { wrap: 2 })),
    plot(data(fw), point, x(col.speed), y(col.speed), across(col.g, { wrap: 2 }))));
  const reach = svg.split("<svg ").slice(2).map((cell) => Math.max(...[...cell.matchAll(
    /<clipPath id="[^"]*"><rect x="([^"]*)" y="[^"]*" width="([^"]*)"/g)].map((m) => Number(m[1]) + Number(m[2]))));
  assert.equal(reach.length, 2);
  assert.ok(reach.every((r) => r > 600), String(reach));
});

// A `data()` at the end of a plot has no mark after it to read its table, so it
// is refused, where the plot drew as though it had never been written. The same
// block runs in all four bindings.
test("a trailing data() is refused", () => {
  const actuals = { year: [2019, 2020, 2021], sales: [1, 2, 3] };
  const forecast = { year: [2024, 2025], sales: [6, 7] };
  assert.throws(
    () => plot(data(actuals), x(col.year), y(col.sales), line, point,
      data(forecast, { name: "forecast" })),
    /`data\(forecast\)` ends the plot/
  );
  assert.throws(
    () => plot(data(actuals), x(col.year), y(col.sales), line, point, data(forecast)),
    /the last `data\(\)` ends the plot/
  );
});

// A plot-wide channel that every mark overrides reaches nothing, so it is refused,
// where it drew as though it had never been written. The same block runs in all
// four bindings.
test("a plot-wide channel every mark overrides is refused", () => {
  const ov = { gdp: [1, 2, 3], life: [4, 5, 6], pop: [7, 8, 9] };
  assert.throws(
    () => render_svg(plot(data(ov), x(col.gdp), y(col.life), size(col.pop), point, size(col.life))),
    /`point` maps `size\(life\)`.*reaches none of them/s
  );
});

// A `nest()` plot has no axes, so the settings that describe one are refused,
// where each was accepted and drew the same bytes as the sentence without it. The
// same block runs in all four bindings.
test("a nest refuses the settings of an axis", () => {
  const nt = { g: ["a", "b", "c"], v: [3, 2, 1] };
  assert.throws(() => render_svg(plot(data(nt), layer(bar, sum), y(col.v), color(col.g), nest(),
    theme({ grid: "both" }))), /`theme\(grid = \)`/);
  assert.throws(() => render_svg(plot(data(nt), layer(bar, sum), y(col.v, { tick_count: 3 }),
    color(col.g), nest())), /`y\(v, tick_count = 3\)`/);
  assert.throws(() => render_svg(plot(data(nt), layer(bar, sum), y(col.v, { limits: [0, 10] }),
    color(col.g), nest())), /under `bar \* sum`/);
});

// `order()` needs a column: `order({ desc: true })` alone was refused without
// saying what a column is for. The refusal names the spelling that runs a category
// axis backward. The same block runs in all four bindings.
test("order() with no column is refused by name", () => {
  assert.throws(() => order({ desc: true }),
    /`order\(\{ desc: true \}\)` names no column.*col\.<category>/s);
  assert.throws(() => order(), /`order\(\)` names no column/);
});

// A `Date` is read on the local clock, always. A column of UTC midnights was read
// as calendar days, but a local time can be a UTC midnight too: in New York in
// winter every 19:00 is one, and a column of evening readings drew a day late,
// in silence. Run in that zone, so the test can fail wherever it runs.
test("a Date is read on the local clock, even when it falls on a UTC midnight", () => {
  const render = new URL("../src/render.js", import.meta.url).href;
  const script = `
    import { dateSeconds } from ${JSON.stringify(render)};
    const evening = [new Date(2024, 0, 1, 19), new Date(2024, 0, 2, 19)];
    const midnights = [new Date(2024, 0, 1), new Date("2024-01-02T00:00")];
    const utc = [new Date("2024-01-01")];
    console.log(JSON.stringify([dateSeconds(evening), dateSeconds(midnights), dateSeconds(utc)]));`;
  const run = spawnSync(process.execPath, ["--input-type=module", "-e", script],
    { env: { ...process.env, TZ: "America/New_York" }, encoding: "utf8" });
  assert.equal(run.status, 0, run.stderr);
  const [evening, midnights, utc] = JSON.parse(run.stdout);
  assert.equal(evening.days, false, "19:00 is a time of day, not a day");
  assert.equal(evening.seconds[0] % 86400, 19 * 3600, "read at 19:00, as the clock shows it");
  assert.equal(midnights.days, true, "a column of local midnights is a column of dates");
  assert.equal(utc.seconds[0], Date.UTC(2023, 11, 31, 19) / 1000,
    "a UTC midnight is the evening before in New York, as JavaScript shows it");
});

// Text stays text, as it does in the other three bindings: a column of
// `"2024-01-01"` strings is categories. Only a `Date` crosses as a date.
test("a column of date-shaped text stays text", () => {
  const wire = to_wire({ d: ["2024-01-01", "2024-03-04"], v: [1, 2] }, "t");
  assert.deepEqual(wire.strings.d, ["2024-01-01", "2024-03-04"]);
  assert.equal(wire.dates.d, undefined);
});

// `order()` ranks a category by the layer's statistic over all of its rows, never by
// one piece of a split. `a` totals 11 from a first piece of 1 and `b` totals 6 from a
// first piece of 5, so ranking by the first piece put `b` first.
test("order() ranks a split category by its whole statistic", () => {
  const pieces = { g: ["a", "a", "b", "b"], era: ["e1", "e2", "e1", "e2"], v: [1, 10, 5, 1] };
  const svg = render_svg(plot(data(pieces), layer(bar, sum), x(col.g), y(col.v),
    color(col.era), order(col.v, { desc: true })));
  const tickX = (name) => {
    const end = svg.indexOf(`>${name}</text>`);
    const start = svg.lastIndexOf('<text x="', end) + '<text x="'.length;
    return Number(svg.slice(start).split('"')[0]);
  };
  assert.ok(tickX("a") < tickX("b"), "ranked by one piece of the split");
});

// In a `nest()` every layer packs its own rows, so a summed layer beside a plain
// one lays out two sets of regions, and the names landed in other groups'
// regions. The same block runs in all four bindings.
test("a nest refuses layers that pack different rows", () => {
  const pk2 = { g: ["a", "a", "b", "c"], v: [3, 1, 2, 1] };
  assert.throws(
    () => render_svg(plot(data(pk2), layer(bar, sum), y(col.v), color(col.g), text, label(col.g), nest())),
    /`bar \* sum` packs one region per group/
  );
});

// A layout makes `degree`, so the table does not hold it, and nodes sized by it
// drew no key. The key reads the frame the nodes were drawn from. The same block
// runs in all four bindings.
test("a size on a layout's degree draws its key", () => {
  const hub = { a: ["Hub", "Hub", "Hub", "Hub"], b: ["p", "q", "r", "s"] };
  const svg = render_svg(plot(data(hub), layer(edge, layout(col.a, col.b)),
    layer(point, layout(col.a, col.b)), size(col.degree), network()));
  assert.ok(svg.includes(">Degree</text>"));
});

// A key takes the opacity its layer was set to: areas drawn at 1.0 were keyed at
// 0.82, paler than the bands they name. The same block runs in all four bindings.
test("a key takes its layer's set opacity", () => {
  const op = { x: [1, 2, 1, 2], y: [1, 2, 2, 3], g: ["a", "a", "b", "b"] };
  const svg = render_svg(plot(data(op), area, x(col.x), y(col.y), color(col.g), style({ opacity: 1 })));
  assert.ok(svg.split(">G</text>")[1].includes('fill-opacity="1.000"'));
});

// A histogram's axis reaches its bins' outer edges: at three bins the first bar
// started at x = -78 and the panel cut 40% of it away. The same block runs in all
// four bindings.
test("a histogram's end bars are drawn whole", () => {
  const hv = { v: [...Array(40).keys()].map((i) => (i % 17) * 1.3) };
  const svg = render_svg(plot(data(hv), layer(bar, bin(3)), x(col.v)));
  const bars = [...svg.matchAll(/<rect x="([-0-9.]+)" y="[-0-9.]+" width="([0-9.]+)" height="[0-9.]+" fill="#4e79a7"/g)];
  assert.equal(bars.length, 3);
  assert.ok(bars.every((m) => Number(m[1]) >= 0 && Number(m[1]) + Number(m[2]) <= 800));
});

// A flow's count axis is ticked over its whole range, as a partition's is: two
// stages labeled 500 / 1000 / 1500 and left out 0 and 2000. The same block runs in
// all four bindings.
test("a flow's count axis is ticked over its whole range", () => {
  const ft = { a: ["p", "p", "q", "q"], b: ["u", "v", "u", "v"], n: [900, 500, 300, 500] };
  const svg = render_svg(plot(data(ft), layer(ribbon, flow(col.a, col.b)), y(col.n)));
  assert.ok(svg.includes(">0</text>") && svg.includes(">2K</text>"));
});

// Only a flat plot offers its axes to a page, so two stacked map projections each
// label their own longitude, where the top one was read against the bottom one's.
// The same block runs in all four bindings.
test("only a flat plot offers its axes to a page", () => {
  const mp = { lon: [-150, -20, 60, 150], lat: [-40, 10, 30, 60] };
  const svg = render_svg(below(
    plot(data(mp), point, x(col.lon), y(col.lat), map({ preserve: "area" })),
    plot(data(mp), point, x(col.lon), y(col.lat), map())));
  assert.equal(svg.split(">0\u00b0</text>").length - 1, 4);
});

// A map's meridians are the curves its projection makes: Equal Earth bends them,
// and Mercator keeps them straight. They were drawn straight in both. The same
// block runs in all four bindings.
test("a map's meridians are the curves its projection makes", () => {
  const mt = { lon: [176, 180, 186, 178], lat: [-38, -20, -15, -30] };
  const curve = /<polyline points="[^"]*" fill="none"\/>/;
  assert.match(render_svg(plot(data(mt), point, x(col.lon), y(col.lat), map())), curve);
  assert.doesNotMatch(render_svg(plot(data(mt), point, x(col.lon), y(col.lat), map({ preserve: "angle" }))), curve);
});

// A globe's labels are clipped by the panel, not by the disk: a name beside a place
// near the limb lost its last letters. The same block runs in all four bindings.
test("a globe's labels are clipped by the panel, not the disk", () => {
  const gl = { lon: [-150, 10], lat: [61, 50], name: ["Anchorage", "Frankfurt"] };
  const svg = render_svg(plot(data(gl), point, x(col.lon), y(col.lat), text, label(col.name), globe()));
  // Frankfurt faces the default view; Anchorage is behind the sphere.
  assert.ok(svg.includes(">Frankfurt</text>"));
  const groups = [...svg.split(">Frankfurt</text>")[0].matchAll(/<g clip-path="url\(#[^)]*\)/g)];
  assert.ok(groups.length && groups[groups.length - 1][0].includes("-labels)"));
});

// A cube's floor names every category: a name that met its neighbor was dropped at
// once, as a number is, and "Americas" went missing. The same block runs in all four
// bindings.
test("a cube's floor names every category", () => {
  const conts = ["Asia", "Europe", "Africa", "Americas", "Oceania"];
  const cf = { life: [...Array(60).keys()].map((i) => 40 + (i % 43)), continent: [...Array(60).keys()].map((i) => conts[i % 5]) };
  const svg = render_svg(plot(data(cf), layer(bar, bin(12)), x(col.life), y(col.continent), space()));
  assert.deepEqual(conts.filter((k) => !svg.includes(`>${k}</text>`)), []);
});

// Zero is written 0 on an axis counted in thousands or millions: 0K and 0M read as
// a quantity where there is none. The same block runs in all four bindings.
test("zero is written 0 on a thousands axis", () => {
  const svg = render_svg(plot(data({ g: ["a", "b", "c"], v: [1200, 2500, 4100] }), bar, x(col.g), y(col.v)));
  assert.ok(svg.includes(">0</text>") && !svg.includes(">0K</text>"));
});

// An unmapped layer beside a color legend is neutral: in the palette's first hue it
// read as the legend's first category. A texture legend's swatches take the ink
// their marks take. The same block runs in all four bindings.
test("an unmapped layer beside a color legend is neutral", () => {
  const nt = { x: [1, 2, 3, 4], y: [1, 3, 2, 4], g: ["a", "b", "a", "b"] };
  const keyed = render_svg(plot(data(nt), x(col.x), y(col.y), point, color(col.g), line));
  assert.match(keyed, /<polyline [^>]*stroke="#3c3c46"/);
  const plain = render_svg(plot(data(nt), x(col.x), y(col.y), point, line));
  assert.match(plain, /<polyline [^>]*stroke="#4e79a7"/);
  const nb = { a: ["p", "q", "r"], b: [3, 5, 4], g: ["u", "v", "w"] };
  const hatched = render_svg(plot(data(nb), bar, x(col.a), y(col.b), pattern(col.g), style({ color: "firebrick" })));
  assert.ok(!hatched.includes("#4e79a7"));
});

// A polar step draws its closing jump: on a wrapped angle the last category is
// adjacent to the first, so the value changes at the first spoke as it does at
// every other. The same block runs in all four bindings.
test("a polar step draws its closing jump", () => {
  const svg = render_svg(plot(data({ g: ["a", "b", "c", "d"], v: [1, 4, 2, 3] }), step, x(col.g), y(col.v), polar()));
  assert.ok(svg.includes('Z" stroke='));
});

// A stated angle domain is the whole turn: the bins are cut on it and the axis is
// not widened past it, so 0 is at the top. The same block runs in all four bindings.
test("a stated angle domain is the whole turn", () => {
  const deg = [...Array(40).keys()].map((i) => 2.6 + i * 9.2);
  const svg = render_svg(plot(data({ deg }), layer(bar, bin), x(col.deg, { limits: [0, 360] }), polar()));
  assert.ok(svg.includes('text-anchor="middle">0</text>'));
});

// A written value outside a stated domain is left out: a count of 3 has no place
// on `y(count, limits = [0, 2])`, where it was drawn cut off at the top. The same
// block runs in all four bindings.
test("a written value outside a stated domain is left out", () => {
  const svg = render_svg(plot(data({ g: ["a", "a", "a", "b"] }), layer(bar, count), x(col.g), y(col.count, { limits: [0, 2] })));
  assert.equal((svg.match(/<rect[^>]*fill="#4e79a7"/g) || []).length, 1);
});

// A banded crater leaves its middle to the bands below: each level is one region
// with its rings under even-odd, so a crater's middle is not painted as its highest
// band. The same block runs in all four bindings.
test("a banded crater leaves its middle to the bands below", () => {
  const i = [...Array(300).keys()];
  const r = i.map((k) => 3 + 0.25 * Math.sin(1.3 * k));
  const ring = { a: i.map((k) => r[k] * Math.cos(0.37 * k)), b: i.map((k) => r[k] * Math.sin(0.37 * k)) };
  const svg = render_svg(plot(data(ring), layer(zone, density({ levels: 6 })), x(col.a), y(col.b)));
  assert.ok(svg.includes('fill-rule="evenodd"') && !svg.includes("<polygon"));
});

// A zone that reads its positions checks their columns: `zone * density` with a
// misspelled `y` drew an empty panel in silence, where `point` refuses it. The same
// block runs in all four bindings.
test("a zone that reads its positions checks their columns", () => {
  const zt = { a: [1, 2, 3, 4, 5, 6], b: [2, 1, 4, 3, 6, 5] };
  assert.throws(
    () => render_svg(plot(data(zt), layer(zone, density), x(col.a), y(col.bb))),
    /`y\(bb\)` refers to a column that is not in the data/
  );
});

// A plot that gives up its y axis keeps no margin under a shared x: in
// `below(top, beside(left, right))`, `right` gives its y axis to `left` and shares its
// x with `top`, and it kept a blank strip where the axis would have been. The same
// block runs in all four bindings.
test("a plot that gives up its y axis keeps no margin under a shared x", () => {
  const gt = { u: [1, 2, 3, 4, 5, 6], v: [0.0012, 0.0031, 0.0054, 0.0087, 0.0102, 0.014], w: [3, 1, 4, 1, 5, 9] };
  const left = plot(data(gt), point, x(col.w), y(col.v));
  const right = plot(data(gt), point, x(col.u), y(col.v));
  const top = plot(data(gt), layer(bar, bin), x(col.u));
  const lastPanel = (svg) => svg.split("<svg ").at(-1).match(/<clipPath[^>]*><rect x="([^"]*)"/)[1];
  assert.equal(lastPanel(render_svg(below(top, beside(left, right)))), lastPanel(render_svg(beside(left, right))));
});

// The title follows a panel a ratio shortened, as the names beside it do: left at
// the top of the rectangle, it stood far over a map set beside a globe. The same
// block runs in all four bindings.
test("the title follows a panel a ratio shortened", () => {
  const svg = render_svg(plot(data({ u: [1, 2, 3, 4], v: [1, 2, 3, 4] }), point, x(col.u), y(col.v), title("Wide"), theme({ ratio: 3 })));
  assert.ok(Number(svg.match(/<text x="[^"]*" y="([^"]*)"[^>]*font-weight="600"/)[1]) > 100);
});

// An area refused in the cube gets its own direction: it said "A `area`" and called
// `path` "`area` with that sort removed", which is `line`'s sentence. The same block
// runs in all four bindings.
test("an area refused in the cube gets its own direction", () => {
  const t = { a: [1, 2, 3, 4], b: [2, 1, 4, 3], c: [5, 6, 7, 8] };
  assert.throws(
    () => render_svg(plot(data(t), x(col.a), y(col.b), area, z(col.c))),
    (e) => e.message.includes("An `area` in space") && e.message.includes("draw the area's edge with `path`")
  );
});

// save_svg writes the drawing byte for byte, returns its path, refuses a path not
// ending in `.svg` toward the same path with that ending, and draws before it
// writes, so a refused plot leaves the file there as it was. The same block runs in
// all four bindings.
test("save_svg writes the drawing byte for byte and refuses a wrong ending", () => {
  const t = { a: [1, 2], b: [3, 4] };
  const p = plot(data(t), point, x(col.a), y(col.b));
  const file = `${os.tmpdir()}/gog-save-svg.svg`;
  assert.equal(save_svg(p, file), file);
  const bytes = fs.readFileSync(file);
  assert.ok(bytes.equals(Buffer.from(render_svg(p), "utf8")));
  assert.throws(() => save_svg(p, `${os.tmpdir()}/life.png`), /life\.svg"\)`/);
  assert.throws(() => save_svg(plot(data(t), point, x(col.a), y(col.missing_col)), file));
  assert.ok(fs.readFileSync(file).equals(bytes));
  fs.unlinkSync(file);
});

// A zone's transform refusal says why and lists what a zone takes: it was one fixed
// sentence that called `bounds` refused, though `zone * bounds` draws. The same
// block runs in all four bindings.
test("a zone's transform refusal says why and lists what a zone takes", () => {
  const t = { a: [1, 2, 3, 4], b: [2, 1, 4, 3] };
  assert.throws(
    () => render_svg(plot(data(t), layer(zone, smooth), x(col.a), y(col.b))),
    (e) => e.message.includes("`smooth` fits a curve along a domain") && e.message.includes("A zone takes `bin`")
  );
});

// The circular-tree refusal names the tree's horizontal bars, not "treads", a word
// the book does not use. The same block runs in all four bindings.
test("the circular-tree refusal names the tree's horizontal bars", () => {
  const ct = { g: ["a", "a", "b", "b", "c", "c"], k: ["u", "v", "u", "v", "u", "v"], v: [1, 2, 2, 1, 3, 3] };
  assert.throws(
    () => render_svg(plot(data(ct), layer(path, cluster(col.v, { over: col.k })), x(col.g), polar())),
    (e) => e.message.includes("the tree's horizontal bars") && !e.message.includes("treads")
  );
});

// A clustered tile plot with one position says it needs a category on each: it
// refused the empty leaf axis as "``", a name the table does not have. The same
// block runs in all four bindings.
test("a clustered tile plot with one position says it needs a category on each", () => {
  const tc = { g: ["a", "a", "b", "b"], k: ["u", "v", "u", "v"], v: [1, 2, 3, 4] };
  assert.throws(
    () => render_svg(plot(data(tc), layer(zone, cluster({ over: col.g })), x(col.g), color(col.v))),
    (e) => e.message.includes("binds `x` but not `y`") && !e.message.includes("``")
  );
});

// A refused transform is not followed by a missing y: `line * cluster` also said
// "Add `y(<column>)`", and adding one reached only another refusal. The same block
// runs in all four bindings.
test("a refused transform is not followed by a missing y", () => {
  const rt = { g: ["a", "a", "b", "b"], k: ["u", "v", "u", "v"], v: [1, 2, 3, 4] };
  assert.throws(
    () => render_svg(plot(data(rt), layer(line, cluster(col.v, { over: col.k })), x(col.g))),
    (e) => e.message.includes("`cluster` joins the closest leaves") && !e.message.includes("but none is set")
  );
});

// A layer the graph places is refused once outside `network()`, by the refusal that
// names it. `edge + polar()` also said "Drop `polar()` to draw it flat", and a flat
// `edge` is refused too; `point * layout` in `map()` was also told to add `x`. The same
// block runs in all four bindings.
test("a layer the graph places is refused once outside the network", () => {
  const t = { a: ["u", "v", "w"], b: ["v", "w", "u"] };
  for (const p of [plot(data(t), edge, polar()), plot(data(t), edge, map()),
                   plot(data(t), layer(point, layout(col.a, col.b)), map())]) {
    assert.throws(
      () => render_svg(p),
      (e) => e.message.includes("network()") && !e.message.includes("Drop `")
        && !e.message.includes("but none is set")
    );
  }
});

// Two categories under a bar are a floor. `bar * count + x(a) + y(b)` was told there
// was nothing to measure and to use `bar * count`; its own refusal now offers
// `space()`. A floor summary with no `z` is asked for `z`, not told to count, and is not
// also told it "is drawn flat". The same block runs in all four bindings.
test("a bar over two categories is sent to the cube, not to count", () => {
  const t = { g: ["a", "a", "b", "b"], k: ["u", "v", "u", "v"], v: [1, 2, 3, 4] };
  assert.throws(
    () => render_svg(plot(data(t), layer(bar, count), x(col.g), y(col.k))),
    (e) => e.message.includes("`+ space()` stands one bar on each pair")
      && !e.message.includes("nothing for it to measure")
  );
  assert.throws(
    () => render_svg(plot(data(t), layer(bar, mean), x(col.g), y(col.k), space())),
    (e) => e.message.includes("add `z(<column>)`") && !e.message.includes("drawn flat")
      && !e.message.includes("bar * count")
  );
});

// A direction to declare an order names how in every language: "set the column's
// factor levels" was R's word, printed to all four. The `order()` refusal and the
// `play()` note now name a factor in R and `ordered()` in the other three. The same
// block runs in all four bindings.
test("asking for a declared order names all four languages", () => {
  const t = { g: ["b", "a", "c"], v: [1, 2, 3], w: [4, 6, 5] };
  const four = "a factor in R, `ordered()` in Python, Julia and JavaScript";
  let sorted = "";
  try {
    render_svg(plot(data(t), point, x(col.v), y(col.w), order(col.v)));
  } catch (e) {
    sorted = e.message;
  }
  const write = process.stderr.write;
  let played = "";
  process.stderr.write = (chunk) => { played += chunk; return true; };
  try {
    render_svg(plot(data(t), point, x(col.v), y(col.w), play(col.g)));
  } finally {
    process.stderr.write = write;
  }
  for (const s of [sorted, played]) {
    assert.ok(s.includes(four) && !s.includes("factor levels"), s);
  }
});

// A flat `zone` with `group()` is told every transform that gives a zone its sides.
// The list named `bounds`, `bin` and `density` and left out `count`, `proportion`,
// `partition` and `flow`. The same block runs in all four bindings.
test("a flat zone with group() names every transform that gives it sides", () => {
  const t = { g: ["a", "a", "b"], v: [1, 2, 3], w: [4, 6, 5] };
  assert.throws(
    () => render_svg(plot(data(t), layer(zone, bin), x(col.v), y(col.w), group(col.g))),
    (e) => e.message.includes("`bin`, `count`, `density`, `proportion`, `bounds`, `partition` and `flow`")
  );
});

// The cube's log-`z` refusal speaks only in the cube. On a globe it printed first,
// above the globe's own refusal of a `z` scale. The same block runs in all four
// bindings.
test("a log z is refused by the cube only in the cube", () => {
  const t = { lon: [1, 2, 3], lat: [4, 5, 6], v: [1, 10, 100] };
  assert.throws(
    () => render_svg(plot(data(t), point, x(col.lon), y(col.lat), z(col.v, { scale: "log" }))),
    (e) => e.message.includes("log `z`-axis")
  );
  assert.throws(
    () => render_svg(plot(data(t), bar, x(col.lon), y(col.lat), z(col.v, { scale: "log" }), globe())),
    (e) => !e.message.includes("log `z`-axis") && e.message.includes("a `globe()` plot draws none")
  );
});

// What a globe hides is said once per table, and names it. Two tables with the same
// counts printed one unnamed line. The same block runs in all four bindings.
test("a globe's hidden rows are said once per table, and name it", () => {
  const places = { lon: [-150, 10, 170], lat: [61, 50, -20] };
  const write = process.stderr.write;
  let said = "";
  process.stderr.write = (chunk) => { said += chunk; return true; };
  try {
    render_svg(plot(data(places, { name: "cities" }), point, x(col.lon), y(col.lat),
      data(places, { name: "copy" }), point, x(col.lon), y(col.lat), globe()));
  } finally {
    process.stderr.write = write;
  }
  assert.ok(said.includes("row(s) of `cities` face away") && said.includes("row(s) of `copy` face away"), said);
});

// A span mark on a map is refused by the map alone, not first told to add a range
// transform the map refuses too. On a globe: a sphere has no straight side for an axis
// label, a `z` with no spike keeps the marks drawn on the sphere, and `turn` is a
// longitude. The same block runs in all four bindings.
test("the globe and map refusals use the globe chapter's words", () => {
  const t = { lon: [1, 2, 3], lat: [4, 5, 6], v: [1, 2, 3] };
  const said = (p) => {
    try { render_svg(p); } catch (e) { return e.message; }
    throw new Error("drew");
  };
  const spanned = said(plot(data(t), interval, x(col.lon), y(col.lat), map()));
  assert.ok(!spanned.includes("produces those extents") && spanned.includes("map()"), spanned);
  assert.ok(said(plot(data(t), point, x(col.lon), y(col.lat), x_label("Longitude"), globe()))
    .includes("no straight side to write one along"));
  assert.ok(said(plot(data(t), point, x(col.lon), y(col.lat), z(col.v), globe()))
    .includes("to keep the marks drawn on the sphere"));
  assert.ok(said(plot(data(t), point, x(col.lon), y(col.lat), globe({ tilt: 100 })))
    .includes("a longitude wraps and a latitude does not"));
});

// A network's refusals are said once, name the table, and can be followed. The
// self-loop refusal printed once per layer and offered to carry the fact as a node's
// own column, which a node does not have; a brush was first told to use `group()` on
// an edge, which an edge refuses. The same block runs in all four bindings.
test("a network's refusals are said once and can be followed", () => {
  const said = (p) => {
    try { render_svg(p); } catch (e) { return e.message; }
    throw new Error("drew");
  };
  const loop = { a: ["u", "v", "w"], b: ["v", "v", "u"] };
  const looped = said(plot(data(loop, { name: "t" }), layer(edge, layout(col.a, col.b)),
    layer(point, layout(col.a, col.b)), layer(text, layout(col.a, col.b)), label(col.name), network()));
  assert.equal(looped.split("connect a node to itself").length - 1, 1, looped);
  assert.ok(looped.includes("row(s) of `t` connect") && !looped.includes("own column"), looped);
  const ring = { a: ["u", "v", "w"], b: ["v", "w", "u"] };
  const brushed = said(plot(data(ring, { name: "t" }), layer(edge, layout(col.a, col.b)),
    layer(point, layout(col.a, col.b)), brush, network()));
  assert.ok(brushed.includes("a network's positions are the layout's") && !brushed.includes("group()")
    && !brushed.includes("Give it an `x()`"), brushed);
});

// A line cut into panels by its own `x` is refused once, for that. On a number column
// the facet's type refusal followed, saying to make the column text, and its advice
// led straight back to the first refusal. The same block runs in all four bindings.
test("a line faceted by its own x is refused once", () => {
  const t = { year: [2000, 2001, 2000, 2001], v: [1, 2, 3, 4] };
  assert.throws(
    () => render_svg(plot(data(t), line, x(col.year), y(col.v), across(col.year))),
    (e) => e.message.includes("both cuts the plot into panels and supplies `x`")
      && !e.message.includes("splits on a number column")
  );
});

// A `ribbon` with `z` hears only the cube. It was first told to add a range transform,
// and `ribbon * range` with `z` was then refused by the cube alone. The same block runs
// in all four bindings.
test("a ribbon with z hears only the cube", () => {
  const t = { east: [1, 2, 3], north: [2, 3, 4], altitude: [10, 20, 30] };
  assert.throws(
    () => render_svg(plot(data(t), ribbon, x(col.east), y(col.north), z(col.altitude))),
    (e) => e.message.includes("a cube has no left to right") && !e.message.includes("produces those extents")
  );
});

// A type refusal offers only the channels that take the column on that mark.
// `size(<category>)` on a point offered `pattern`, which a point refuses, and
// `bar + color(<number>)` was given no direction, though `opacity` draws it. The same
// block runs in all four bindings.
test("a type refusal offers only the channels the mark takes", () => {
  const t = { g: ["a", "b", "c"], v: [1, 2, 3], w: [3, 5, 4] };
  assert.throws(
    () => render_svg(plot(data(t), point, x(col.v), y(col.w), size(col.g))),
    (e) => e.message.includes("Use `color` or `shape` to distinguish categories") && !e.message.includes("pattern")
  );
  assert.throws(
    () => render_svg(plot(data(t), bar, x(col.g), y(col.v), color(col.w))),
    (e) => e.message.includes("Use `opacity` to show a numeric column")
  );
});

// A channel a mark does not have is refused toward the marks that do. The mapping
// refusal and its `style()` sibling said "use a mark that has one" and named none. The
// same block runs in all four bindings.
test("a missing feature is refused toward the marks that have it", () => {
  const t = { g: ["a", "b", "c"], v: [1, 2, 3], w: [3, 5, 4] };
  assert.throws(
    () => render_svg(plot(data(t), point, x(col.v), y(col.w), group(col.g))),
    (e) => e.message.includes("use a mark that maps it: `line`, `area`")
  );
  assert.throws(
    () => render_svg(plot(data(t), bar, x(col.g), y(col.v), style({ size: 3 }))),
    (e) => e.message.includes("use a mark that has one: `point`, `line`")
  );
  // An edge's positions come from the layout, and the network says so alone.
  assert.throws(
    () => render_svg(plot(data(t), edge, x(col.v), y(col.w), network())),
    (e) => !e.message.includes("cannot be bound to `edge`")
  );
});

// `line * dodge` is not sent to `stack`, which a line refuses too. It is told that
// lines cross rather than cover, and that `area * stack` piles the groups. The same
// block runs in all four bindings.
test("line * dodge is not sent to stack", () => {
  const t = { g: ["a", "a", "b", "b"], yr: [1, 2, 1, 2], v: [1, 2, 3, 4] };
  assert.throws(
    () => render_svg(plot(data(t), layer(line, dodge), x(col.yr), y(col.v), color(col.g))),
    (e) => e.message.includes("cross rather than cover each other") && e.message.includes("`area * stack`")
      && !e.message.includes("(`stack`)")
  );
});

// A transform written twice is told so once. `mean * mean` was offered "`bar * mean` or
// `bar * mean`", and `proportion * proportion` was told about `stack(share = TRUE)`,
// which it never wrote. The same block runs in all four bindings.
test("a transform written twice is told so once", () => {
  const t = { g: ["a", "a", "b", "b"], v: [1, 2, 3, 4] };
  assert.throws(
    () => render_svg(plot(data(t), layer(bar, mean, mean), x(col.g), y(col.v))),
    (e) => e.message.includes("names `mean` twice") && !e.message.includes("or `bar * mean`")
  );
  assert.throws(
    () => render_svg(plot(data(t), layer(bar, proportion, proportion), x(col.g))),
    (e) => e.message.includes("names `proportion` twice") && !e.message.includes("stack(share = TRUE)")
  );
});

// A flow in `polar()` is refused once, in words true of every layer. It said "the bands
// bent round a rim" for a zone and a text too, once per layer. The same block runs in
// all four bindings.
test("a flow in polar is refused once for every layer", () => {
  const t = { a: ["u", "u", "v"], b: ["p", "q", "q"] };
  assert.throws(
    () => render_svg(plot(data(t), layer(ribbon, flow(col.a, col.b)), layer(zone, flow(col.a, col.b)), polar())),
    (e) => e.message.split("chord diagram").length === 2 && e.message.includes("a flow bent round a rim")
      && !e.message.includes("bands")
  );
});

// A path is sent to `line` only with a statistic. `path * flow` was also told "Use
// `line * flow`", which a line refuses too; `path * count` was also told to write
// `path * count + x() + y()`, refused in turn. The same block runs in all four bindings.
test("a path is sent to line only with a statistic", () => {
  const t = { a: ["u", "u", "v"], b: ["p", "q", "q"], v: [1, 2, 3], w: [2, 3, 4] };
  assert.throws(
    () => render_svg(plot(data(t), layer(path, flow(col.a, col.b)))),
    (e) => !e.message.includes("Use `line * flow`") && e.message.includes("`flow` lays")
  );
  assert.throws(
    () => render_svg(plot(data(t), layer(path, count), x(col.v), y(col.w))),
    (e) => e.message.includes("Use `line * count`") && !e.message.includes("contours")
  );
});

// A transform that refuses a mark in its own check is refused once. `surface * bounds`
// printed two refusals of `bounds`, and a text was told that `bounds` "replaces those
// rows with one summary per key". The same block runs in all four bindings.
test("a transform that refuses a mark itself is refused once", () => {
  const t = { a: ["u", "v", "w"], v: [1, 2, 3], w: [2, 3, 4], h: [5, 6, 7] };
  assert.throws(
    () => render_svg(plot(data(t), layer(surface, bounds(col.v, col.w)), x(col.v), y(col.w), z(col.h))),
    (e) => e.message.includes("`bounds` supplies") && !e.message.includes("gives each row two edges")
  );
  assert.throws(
    () => render_svg(plot(data(t), layer(text, bounds(col.v, col.w)), x(col.a), label(col.a))),
    (e) => !e.message.includes("replaces those rows")
  );
});

// An atom handed to `beside()` is named as it is written, and so is the example: every
// atom was shown by its `typeof`, so `color(col.a)` read `plot(data(df), object, …)`.
// The same block runs in all four bindings.
test("a page refusal names the atom it was given", () => {
  const t = { a: [1, 2], b: [3, 4], g: ["u", "v"] };
  const p = plot(data(t), point, x(col.a), y(col.b));
  // The sentence R, Python and Julia say for `page + atom`, in JavaScript's words:
  // it opens on what the atom belongs to and closes on where to write it.
  refuses(() => beside(p, p, color(col.g)),
    /`color\(\)` belongs to a plot, and `beside\(\)` arranges plots\. Write it into the plot it describes, before composing: `beside\(plot\(data\(df\), …, color\(…\)\), other_plot\)`/);
  refuses(() => beside(p, p, point), /`point` belongs to a plot/);
  // A mark with its transforms is named with its parts, as the other three name it
  // `bar * count`; it was `layer()`, which says nothing about which one.
  refuses(() => beside(p, p, layer(bar, count, proportion)),
    /`layer\(bar, count, proportion\)` belongs to a plot, and `beside\(\)` arranges plots\. Write it into the plot it describes, before composing: `beside\(plot\(data\(df\), …, layer\(bar, count, proportion\)\), other_plot\)`/);
  refuses(() => below(p, data(t)),
    /`data\(\)` belongs to a plot, and `below\(\)` arranges plots\. Write it into the plot it holds the table for, before composing: `below\(plot\(data\(df\), …\), other_plot\)`/);
});

// A position handed to `beside()` is named as it is written, `x()`, not the `coord_x()`
// it is held as. (JavaScript spells `/` as `below()`, so the unparenthesized page that
// R, Python and Julia test beside this has no JavaScript form.)
test("a position atom on a page is named as it is written", () => {
  const t = { a: [1, 2], b: [3, 4] };
  const p = plot(data(t), point, x(col.a), y(col.b));
  refuses(() => beside(p, p, x(col.a)),
    /`x\(\)` belongs to a plot, and `beside\(\)` arranges plots\. Write it into the plot it describes, before composing: `beside\(plot\(data\(df\), …, x\(…\)\), other_plot\)`/);
});

// An atom written after a facet draws as one written before it. In JavaScript `plot()`
// takes its atoms in any order, so the two orders are one sentence; R and Python, whose
// `+` binds before `|`, carry the atom with the facet to the same result. The same block
// runs in all four bindings.
test("an atom written after a facet draws as one written before it", () => {
  const t = { a: [1, 2], b: [3, 4], g: ["u", "v"] };
  const after = render_svg(plot(data(t), point, x(col.a), y(col.b), across(col.g), title("t")));
  const before = render_svg(plot(data(t), point, x(col.a), y(col.b), title("t"), across(col.g)));
  assert.equal(after, before);
});

// `data(rows)` takes an array of row objects, one per row: the three asynchronous-
// driver refusals and `query()`'s own tell the reader to pass their rows that way,
// and the table was then refused as not an object of columns. The rows are turned
// into the same columns, so the plot is the same bytes; a key a row lacks is missing.
test("data(rows) takes an array of row objects, as the query refusals advise", () => {
  const rows = [{ a: 1, b: 3 }, { a: 2, b: 4 }, { a: 3, b: 5 }];
  const fromRows = render_svg(plot(data(rows), point, x(col.a), y(col.b)));
  const fromColumns = render_svg(plot(data({ a: [1, 2, 3], b: [3, 4, 5] }), point, x(col.a), y(col.b)));
  assert.equal(fromRows, fromColumns);
  // The advice, followed: an asynchronous connection is refused toward `data(rows)`,
  // with the line that awaits the rows for each driver. A `mysql2` connection has a
  // `prepare()` as well, and was told only that the result "has no `.all()`".
  const pgClient = { query: () => Promise.resolve({ rows }) };
  const mysqlConnection = { prepare: () => Promise.resolve({}), query: () => Promise.resolve([rows, []]) };
  for (const con of [pgClient, mysqlConnection]) {
    refuses(() => render_svg(plot(query(con, "SELECT a, b FROM t"), point, x(col.a), y(col.b))),
      /`const \{ rows \} = await con\.query\(sql\)` with `pg`, `const \[rows\] = await con\.query\(sql\)` with `mysql2`, then `data\(rows\)`/);
  }
  refuses(() => data([]), /no rows/);
  refuses(() => data([1, 2]), /something other than rows/);
});

// An invented table name gives way to one the reader wrote: an unnamed table followed
// by `data(df, { name: "data" })` was refused as two tables with one name, while the
// other order drew. The invented name moves, as it does on a page. The same block runs
// in all four bindings.
test("an invented table name gives way to one the reader wrote", () => {
  const a = { year: [1, 2, 3], sales: [4, 5, 6] };
  const b = { year: [4, 5], sales: [7, 8] };
  const moved = render_svg(plot(data(a), line, x(col.year), y(col.sales), data(b, { name: "data" }), point));
  const named = render_svg(plot(data(a, { name: "data2" }), line, x(col.year), y(col.sales),
    data(b, { name: "data" }), point));
  assert.equal(moved, named);
});

// An expression in a channel is refused with direction: `col.gdp / 1000` was `NaN`,
// which reached the atom as "Got number". A column in a string still reads as the
// accessor. The same block runs in all four bindings.
test("an expression in a channel is refused with direction", () => {
  refuses(() => x(col.gdp / 1000), /not an expression[\s\S]*Compute the column in JavaScript first/);
  assert.equal(`${col.gdp}`, "col.gdp");
});

// A blank numeric cell is a missing value: gog_table()'s reader read it as 0, and an
// `NA` turned the column into text. R's read.csv, the reference, reads the empty cell
// and `NA` as missing; a text column keeps its text. A cell of nothing but spaces is
// blank too: R read it as missing while this read the column as text, so one file drew
// two pictures. ` NA ` is not `NA`, in R or here. The same block runs in all four
// bindings.
test("a blank numeric cell is a missing value", async () => {
  const { columns } = await import("../src/tables.js");
  const cols = columns([["a", "b", "c", "d"], ["1", "x", "1", "1"], ["", "", " ", " NA "],
    ["NA", "z", "3", "3"], ["4", "w", "4", "4"]]);
  assert.deepEqual(cols.a, [1, null, null, 4]);
  assert.deepEqual(cols.b, ["x", "", "z", "w"]);
  assert.deepEqual(cols.c, [1, null, 3, 4]);
  assert.deepEqual(cols.d, ["1", " NA ", "3", "4"]);
});

// Messages that pointed the wrong way point at a spelling that draws: the missing-value
// drop says it reaches every layer of its table, a density on the axis it draws is sent
// to the axis it reads, a border on a flow's bands is sent to the strata, and a second
// column in a facet word is sent to the crossing. A bare count where the options go was
// dropped in silence. The same block runs in all four bindings.
test("four messages point at a spelling that draws", () => {
  const write = process.stderr.write;
  let said = "";
  process.stderr.write = (chunk) => { said += chunk; return true; };
  try {
    render_svg(plot(data({ x: [1, 2, null, 4], y: [1, 2, 3, 4] }, { name: "gaps" }),
      point, x(col.x), y(col.y)));
  } finally {
    process.stderr.write = write;
  }
  assert.ok(said.includes("left out of every layer drawn from `gaps`")
    && !said.includes("other plotting tools"), said);
  refuses(() => render_svg(plot(data({ v: [1, 2, 2, 3, 3, 4] }, { name: "spread" }),
    layer(line, density), y(col.v))), /write it on `x` instead: `line \* density \+ x\(v\)`/);
  const stages = { stage_a: ["p", "p", "q"], stage_b: ["u", "v", "u"] };
  refuses(() => render_svg(plot(data(stages, { name: "stages" }),
    layer(ribbon, flow(col.stage_a, col.stage_b)), style({ border_color: "white" }))),
    /`zone \* flow\(<a>, <b>\) \+ style\(border_color = "white"\)`/);
  refuses(() => across(col.stage_a, col.stage_b),
    /split each way: `across\(col\.stage_a\), down\(col\.stage_b\)`/);
  refuses(() => down(col.stage_a, 3), /`down\(col\.stage_a, \{ wrap: 3 \}\)`/);
});

// A column two tables share is one set of categories: each table's `k` was ordered on
// its own, so `shape(k)` drew the second table's `c` and `d` in the first table's
// circle and square under a key of `a` and `b`, and `color(k)` colored four and keyed
// two. The same block runs in all four bindings.
test("a column two tables share is one set of categories", () => {
  const ta = { x: [1, 2], y: [1, 2], k: ["a", "b"] };
  const tb = { x: [3, 4], y: [3, 4], k: ["c", "d"] };
  const both = (channel) => render_svg(plot(
    data(ta, { name: "ta" }), point, x(col.x), y(col.y), channel(col.k),
    data(tb, { name: "tb" }), point, x(col.x), y(col.y), channel(col.k)));
  for (const svg of [both(color), both(shape)]) {
    for (const k of ["a", "b", "c", "d"]) assert.ok(svg.includes(`>${k}</text>`), `the key lists ${k}`);
  }
  assert.equal(both(shape).split("<polygon").length - 1, 4, "`c` and `d` take their own glyphs");
});

// Each `show()` writes its own file. The name counted the plot's layers, so two
// one-layer plots shown in one run shared a path and the second overwrote the
// first. JavaScript alone names the file itself; the other three bindings display
// in a notebook or take a fresh temporary file.
test("each show() writes its own file", () => {
  const first = show(plot(data({ a: [1, 2], b: [3, 4] }, { name: "first" }), point, x(col.a), y(col.b)));
  const second = show(plot(data({ a: [5, 6], b: [7, 8] }, { name: "second" }), point, x(col.a), y(col.b)));
  try {
    assert.notEqual(first, second);
    assert.ok(fs.existsSync(first) && fs.existsSync(second));
    assert.notEqual(fs.readFileSync(first, "utf8"), fs.readFileSync(second, "utf8"));
  } finally {
    for (const f of [first, second]) fs.rmSync(f, { force: true });
  }
});

// What `repel` could not separate is said once per layer, naming the panels: a
// faceted plot printed one unnamed line per panel. The same block runs in all four
// bindings.
test("a crowded repel is said once and names its panels", () => {
  const crowd = {
    x: [...Array(200).fill(5), 1, 9], y: [...Array(200).fill(5), 1, 9],
    n: [...Array.from({ length: 200 }, (_, i) => `a rather long label, number ${i + 1}`), "p", "q"],
    side: [...Array(200).fill("busy"), "calm", "calm"],
  };
  const write = process.stderr.write;
  let said = "";
  process.stderr.write = (chunk) => { said += chunk; return true; };
  try {
    render_svg(plot(data(crowd, { name: "crowd" }), layer(text, repel), x(col.x), y(col.y),
      label(col.n), across(col.side)));
  } finally {
    process.stderr.write = write;
  }
  assert.ok(said.includes("in 1 of 2 panels: `busy` (") && !said.includes("`calm`"), said);
});

// Seen from below, a cube's floor is numbered along its outline, not across the data:
// the floor edges nearest the camera run through the middle of the picture from below,
// and at `tilt = -25` their numbers sat on the points. The same block runs in all four
// bindings.
test("seen from below, a cube is numbered along its outline", () => {
  const svg = render_svg(plot(data({ a: [1, 2, 3, 4, 5], b: [5, 4, 3, 2, 1], c: [2, 4, 1, 5, 3] }),
    point, x(col.a), y(col.b), z(col.c), space({ tilt: -25 })));
  const dots = [...svg.matchAll(/<circle cx="([0-9.]+)" cy="([0-9.]+)"/g)].map((m) => [+m[1], +m[2]]);
  const frame = svg.split('paint-order="stroke"')[1].split("</g>")[0];
  const labels = [...frame.matchAll(/<text x="([0-9.-]+)" y="([0-9.-]+)"/g)].map((m) => [+m[1], +m[2]]);
  const xs = dots.map((d) => d[0]), ys = dots.map((d) => d[1]);
  const inside = labels.filter(([lx, ly]) =>
    lx > Math.min(...xs) && lx < Math.max(...xs) && ly > Math.min(...ys) && ly < Math.max(...ys));
  assert.ok(labels.length > 0 && inside.length === 0, `${inside}`);
});

// A cube's numbers read on a dark panel: they were written in the dark defaults
// whatever `theme({ background })` said, dark on dark. The same block runs in all four
// bindings.
test("a cube's labels read on a dark panel", () => {
  const svg = render_svg(plot(data({ a: [1, 2, 3], b: [3, 1, 2], c: [2, 3, 1] }),
    point, x(col.a), y(col.b), z(col.c), theme({ background: "black" })));
  const frame = svg.split('paint-order="stroke"')[1].split("</g>")[0];
  const inks = new Set([...frame.matchAll(/fill="(#[0-9a-f]+)"/g)].map((m) => m[1]));
  assert.deepEqual([...inks], ["#ffffff"]);
});

// A layer that stands still in a played plot is drawn once where that keeps its place
// in the order: written before every played layer, once under the moments. It was
// copied into every moment. The same block runs in all four bindings.
test("a still layer is drawn once in a played plot", () => {
  const t = { x: [1, 2, 3, 4], y: [1, 2, 3, 4], year: [1957, 1957, 1962, 1962] };
  const svg = render_svg(plot(data(t), rule, y(col.y), point, x(col.x), y(col.y), play(col.year)));
  const rules = svg.split("\n").filter((l) => l.includes("<line") && l.includes(" stroke=")).length;
  assert.equal(rules, 4);
});

// A ramped stroke is drawn as runs of one color: a numeric `color` that does not change
// along a route is one polyline, where it was a `<line>` per segment whose caps
// overlapped at every vertex. The same block runs in all four bindings.
test("a ramped stroke is drawn as runs of one color", () => {
  const t = {
    x: [1, 2, 3, 4, 1, 2, 3, 4], y: [1, 3, 2, 4, 2, 4, 3, 5],
    g: ["a", "a", "a", "a", "b", "b", "b", "b"], band: [1, 1, 1, 1, 2, 2, 2, 2],
  };
  const lines = render_svg(plot(data(t), path, x(col.x), y(col.y), group(col.g), color(col.band))).split("\n");
  // A data segment carries its own cap; the legend's divider does not.
  assert.equal(lines.filter((l) => l.includes("<line") && l.includes("stroke-linecap=")).length, 0);
  assert.equal(lines.filter((l) => l.includes("<polyline")).length, 2);
});

// A plot too small for what is drawn around its panel is refused: 60 px of width drew a
// panel -9 px wide with no message. The same block runs in all four bindings.
test("a plot too small for its axes is refused", () => {
  refuses(() => render_svg(plot(data({ x: [1, 20000], y: [1, 80] }), point, x(col.x), y(col.y),
    theme({ width: 60 }))), /with no room to draw in/);
});

// A surface takes `quantile` beside `bin`, as it takes the other five reductions and as
// a 3-D `bar` and a `zone` take it. The same block runs in all four bindings.
test("a surface takes quantile", () => {
  const grid = {
    a: Array.from({ length: 20 }, (_, i) => (i % 5) + 1),
    b: Array.from({ length: 20 }, (_, i) => Math.floor(i / 5) + 1),
    v: Array.from({ length: 20 }, (_, i) => (i + 1) % 7),
  };
  assert.match(render_svg(plot(data(grid), layer(surface, bin, quantile(0.9)),
    x(col.a), y(col.b), z(col.v))), /^<svg /);
});

// The engine reads its request from a file, never from `spawnSync`'s `input`. On
// macOS that pipe can deliver every byte and never the end of it, so the engine
// waited for the end of its input and node waited for the engine: the book's
// parity run hung that way for 44 minutes and again for 20. The file lives in a
// directory of its own and must not outlive the call.
test("the engine reads its request from a file that does not outlive the call", () => {
  const source = fs.readFileSync(new URL("../src/render.js", import.meta.url), "utf8");
  assert.doesNotMatch(source, /\binput\s*:/,
    "no engine call hands its request to spawnSync's `input`");
  const before = process.env.TMPDIR;
  const tmp = fs.mkdtempSync(`${os.tmpdir()}/gog-request-`);
  process.env.TMPDIR = tmp;
  try {
    const svg = render_svg(plot(data({ a: [1, 2, 3], b: [3, 1, 2] }), point, x(col.a), y(col.b)));
    assert.match(svg, /^<svg /);
    assert.throws(() => render_svg(plot(data({ a: [1, 2] }), point, x(col.a), y(col.nope))),
      GogError);
    assert.deepEqual(fs.readdirSync(tmp), [],
      "the request's file is removed after the call, whether the engine drew or refused");
  } finally {
    if (before === undefined) delete process.env.TMPDIR;
    else process.env.TMPDIR = before;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

// Two blocks of one plot share no id. Ids come from the drawing, and a page
// resolves an id against the whole document, so the same plot twice on one page,
// or in two notebooks JupyterLab holds in one document, shared its clips and
// textures: the second plot's controls landed on the first, and the second copy
// drew with the first's. Each block names them after itself; `render_svg()`
// stays one file.
test("two blocks of one plot share no id", () => {
  const p = plot(data({ g: ["a", "b"], v: [1, 2] }), bar, x(col.g), y(col.v), pattern(col.g));
  const svgOf = (h) => /<svg[\s\S]*<\/svg>/.exec(h)[0];
  const ids = (s) => [...s.matchAll(/ id="([^"]+)"/g)].map((m) => m[1]);
  const refs = (s) => [...s.matchAll(/url\(#([^)]+)\)/g)].map((m) => m[1]);
  const [ha, hb] = [html_block(p), html_block(p)];
  const idOf = (h) => /<div class="gog-plot" id="([^"]+)"/.exec(h)?.[1];
  if (idOf(ha)) assert.notEqual(idOf(ha), idOf(hb), "each block has its own container");
  const [a, b] = [svgOf(ha), svgOf(hb)];
  assert.ok(ids(a).length > 0, "the premise: a textured bar mints ids");
  assert.ok(ids(a).every((i) => !ids(b).includes(i)), "two blocks of one plot share an id");
  for (const s of [a, b]) {
    assert.ok(refs(s).every((r) => ids(s).includes(r)), "a reference left its own block");
  }
  assert.equal(render_svg(p), render_svg(p), "render_svg() stays one file");
});

test("a width puts an edge at zero, and the largest value gets a bin of its own", () => {
  // Edges from the smallest value would put 39.6 and 44 in one bin, 39.6 to
  // 44.6; folded into the last bin, 35 would share 34's bar.
  const bars = (svg) =>
    svg.split("\n").filter((l) => l.includes("<rect") && l.includes("fill-opacity")).length;
  assert.equal(bars(render_svg(plot(data({ v: [39.6, 44, 82.6] }), layer(bar, bin({ width: 5 })), x(col.v)))), 3);
  assert.equal(bars(render_svg(plot(data({ v: [34, 35] }), layer(bar, bin({ width: 1 })), x(col.v)))), 2);
});
