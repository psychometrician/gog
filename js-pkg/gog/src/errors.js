// errors.js — the one error type the binding throws
//
// A refusal is a refusal whether the engine made it or the binding did, so both
// arrive as `GogError` and both carry a message that starts `gog: ` and says
// what to write instead (spec §12: errors must give direction). The mirror of
// `py-pkg/gog/gog/errors.py`, and it exists here for the same reason: R's
// `stop()` is already one thing, and a language with an error taxonomy needs a
// name so callers can catch "the grammar refused" as one category rather than
// fishing it out of `TypeError`.

export class GogError extends Error {
  constructor(message) {
    super(message);
    this.name = "GogError";
  }
}
