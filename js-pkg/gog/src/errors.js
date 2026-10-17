// errors.js — the one error type the binding throws
//
// A refusal is a refusal whether the engine made it or the binding did, so both
// arrive as `GogError` and both carry a message that starts `gog: ` and says
// what to write instead (spec §12: errors must give direction). The mirror of
// `py-pkg/gog/gog/errors.py`, and it exists here for the same reason: R's
// `stop()` is already one thing, and a language with an error taxonomy needs a
// name so callers can catch "the grammar refused" as one category rather than
// fishing it out of `TypeError`.

/**
 * The error gog throws when it refuses a sentence.
 *
 * Both kinds of refusal throw a `GogError`: one this package makes when the
 * sentence is written, and one the engine makes when the plot is drawn. Its
 * message starts with `gog: ` and says what to write instead. Catch it to tell
 * gog's refusals apart from a `TypeError` or an error of your own. You do not
 * make one yourself; the class is exported so that code can check for one with
 * `instanceof`.
 *
 * @param {string} message What was refused, and what to write instead.
 *
 * @example
 * const cars = {
 *   wt: [2.62, 2.875, 2.32, 3.215, 3.44, 3.46, 3.57, 3.19],
 *   mpg: [21, 21, 22.8, 21.4, 18.7, 18.1, 14.3, 24.4],
 * };
 *
 * // A plain string is a value, not a column, so this is refused.
 * try {
 *   x("wt");
 * } catch (error) {
 *   console.log(error instanceof GogError); // true
 *   console.log(error.message); // names the fix, `x(col.wt)`
 * }
 *
 * // The sentence the message asks for.
 * const p = plot(data(cars), point, x(col.wt), y(col.mpg));
 */
export class GogError extends Error {
  constructor(message) {
    super(message);
    this.name = "GogError";
  }
}

// An options object may hold only the names its function takes. A key it does
// not know is refused by name rather than ignored, because a setting that does
// nothing and says nothing is the silent drop §12 forbids, and a misspelt key
// in an options object is how one arrives. `readArgs` in `atoms.js` says the
// same, in the same words, for the atoms it reads.
export function refuseUnknownOptions(options, call, names) {
  if (options === null || typeof options !== "object") return;
  for (const key of Object.keys(options)) {
    if (!names.includes(key)) {
      const takes = names.map((name) => `\`${name}\``).join(", ");
      throw new GogError(`gog: \`${call}()\` has no \`${key}\` — it takes ${takes}.`);
    }
  }
}
