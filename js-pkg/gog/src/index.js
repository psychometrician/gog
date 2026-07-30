// gog — one graphics engine, written in Rust, spoken here in JavaScript.
//
// A plot is a *specification*, not drawing code. The discipline is Wilkinson's
// Grammar of Graphics with Hangeul-style regularity: a tiny orthogonal kernel,
// systematic derivation, no exceptions.
//
//     import { plot, data, point, x, y, col, render_svg } from "grammar-of-graphics";
//
//     const gm = { gdp: [1, 2, 3], life: [60, 70, 80] };
//     render_svg(plot(data(gm), point, x(col.gdp), y(col.life)));
//
// **Why this binding reads differently from the other three.** JavaScript cannot
// overload `+`, `*`, `|` or `/`, so it is the one target that must write a
// different *sentence* rather than a differently-captured one (spec §8). Four
// words carry the four operators, and they are the operators rather than new
// grammar:
//
//     data(gm) + bar * bin + x(life) | facet(era)                     R
//     plot(data(gm), layer(bar, bin), x(col.life), across(col.era))   here
//
// Everything else is the same word in every binding, underscores included: Law 3
// names `_` as the joiner, so the kernel is not re-spelled to suit a
// community's habit.
//
// Sub-expressions stay nameable, which is Law 6 and the reason this surface was
// chosen over a tagged template:
//
//     const piled = layer(point, bin, stack);
//     plot(data(heights), piled, x(col.height));

export { GogError } from "./errors.js";
export { col, Column } from "./columns.js";
export { Atom, Page, Plot, plot, layer, across, down, beside, below, facet, data } from "./spec.js";
export {
  find_gog_cli,
  ordered,
  render_svg,
  save,
  show,
  svg_block,
  to_wire,
} from "./render.js";

export {
  // marks — the "consonants"
  point,
  line,
  path,
  rule,
  zone,
  area,
  bar,
  step,
  interval,
  box,
  ribbon,
  text,
  surface,
  // transforms
  bin,
  smooth,
  count,
  density,
  sum,
  mean,
  median,
  max,
  min,
  proportion,
  range,
  confidence,
  bounds,
  partition,
  dodge,
  stack,
  jitter,
  // positions and spaces
  x,
  y,
  z,
  space,
  polar,
  nest,
  // channels — the "vowels"
  color,
  // exported only to be refused: the British spelling names its fix
  colour,
  group,
  size,
  shape,
  opacity,
  label,
  pattern,
  play,
  // settings and plot-level atoms
  style,
  theme,
  order,
  palette,
  title,
  x_label,
  y_label,
  z_label,
} from "./atoms.js";
