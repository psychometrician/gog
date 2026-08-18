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
  mean,
  sum,
  ordered,
  palette,
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
  z,
  zone,
  surface,
  html_block,
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
    /factor\(gdp\)/
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

  save(good, file);
  const before = fs.readFileSync(file, "utf8");
  assert.ok(before.length > 0);

  assert.throws(() => save(bad, file), GogError);
  assert.equal(fs.readFileSync(file, "utf8"), before);
});

test("a plot starts with its table", () => {
  refuses(() => plot(point, x(col.x)), /starts with its table/);
  refuses(() => plot(), /nothing to draw/);
  refuses(() => plot(data(df), 42), /takes gog atoms/);
  refuses(() => data(point), /not an atom/);
  refuses(() => data("gm"), /takes a table/);
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
      /does not play[\s\S]*play\(year\)/,
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
  // `shape` measures nothing, so it offers no domain either — and JavaScript's
  // options object refuses an unknown key rather than ignoring it.
  refuses(() => shape(col.g, { limits: [0, 1] }), /limits/);

  // A domain on a temporal axis is written in dates, and the binding converts
  // them the way it converts the column — otherwise the two disagree and every
  // row falls outside.
  const days = Array.from({ length: 20 }, (_, i) => new Date(Date.UTC(2024, 2, i + 1)));
  const year = render_svg(plot(data({ day: days, orders: days.map((_, i) => 20 + i) }),
    line, y(col.orders),
    x(col.day, { limits: [new Date(Date.UTC(2024, 0, 1)), new Date(Date.UTC(2024, 11, 31))] })));
  assert.match(year, />Jan 2024<\/text>/);
  assert.match(year, />Nov 2024<\/text>/);
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
  // `tick_count` only the three that draw an axis. JavaScript's options object
  // refuses an unknown key rather than ignoring it.
  refuses(() => color(col.a, { tick_count: 4 }), /tick_count/);

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
    () => render_svg(plot(data(cubePlots), rule, x(col.yield), z(col.yield), space())),
    /footprint/
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
  refuses(
    () => render_svg(below(
      plot(data(cars, { name: "cars" }), point, x(col.speed), y(col.dist), theme({ height: 500 })),
      plot(data(cars, { name: "cars" }), point, x(col.speed), y(col.dist), theme({ height: 500 })))),
    /ask for 1000px/
  );
});

// And a *page* states its own size, which is the one sentence no cell can write.
// Composed side by side, two plots divide the page's width and each keep the
// whole of its height, so only the page can say how much height that is. A
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

test("a line has no single row to select, and the refusal names group()", () => {
  const d = { v: [1, 2, 3], w: [2, 4, 1] };
  assert.throws(() => render_svg(plot(data(d, { name: "bt" }), line,
    x(col.v), y(col.w), brush(col.v, { at: [1, 2] }))),
    /one shape through many rows[\s\S]*group\(\)/);
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
