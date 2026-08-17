// tables.js — the book's example tables, fetched by name.
//
// Not a word of the grammar, and deliberately so. This is the same category as
// `render_svg`: something the binding needs and the vocabulary does not.
//
// This file is the reason the whole helper moved into the four packages. R
// reads a CSV in one call and Python and Julia in a handful, but JavaScript has
// no CSV parser in its standard library, so the reader below is the long one.
// It is not decoration: one country in `gapminder_2007.csv` is
// "Congo, Dem. Rep." and its name holds a comma, so splitting a line on commas
// gives that row seven fields where the header has six. Asking a reader to
// paste thirty lines of quote handling before drawing anything was the problem.
//
// The tables are not shipped with the package. They are fetched from the book's
// own site, so one copy serves all four languages.
//
// The name carries the package's, and that is the whole of why it is no longer
// `book_table()`. This package and `god` are built to be loaded together, so
// `gog_table()` and `god_table()` stand side by side at a prompt and read as one
// idea in two spellings. They still differ by the one letter that separates the
// two projects everywhere else, so neither masks the other.
//
// The old name is gone rather than deprecated. An alias would have been the
// careful move on a package with a readership, and this one does not have one
// yet: the window where a rename costs nobody anything is open now and closes
// for good. Two spellings of one function is a debt Law 3 would have carried
// until someone finally removed it, so it was not taken on.

import { GogError } from "./errors.js";

export const BOOK_DATA_URL = "https://psychometrician.github.io/gog-book/data/";
export const BOOK_DATA_CHAPTER =
  "https://psychometrician.github.io/gog-book/book-data.html";

/**
 * The names of the tables, read from the site rather than carried.
 *
 * A list shipped inside the package would be fixed at the version it shipped
 * with, so the day a table is added an installed copy would deny a table that
 * exists. That is the worst kind of refusal: confident and wrong. The site
 * publishes the list beside the tables themselves, generated from the
 * directory, so the answer is always the one the site can actually serve.
 *
 * Read only when a name has already failed, so the cost falls on the error path
 * and never on a plot. It returns nothing rather than failing, because a
 * diagnostic that can itself fail is not a diagnostic.
 */
export async function table_names() {
  try {
    const response = await fetch(`${BOOK_DATA_URL}tables.txt`);
    if (!response.ok) return [];
    const body = await response.text();
    return body.split("\n").map((line) => line.trim()).filter(Boolean);
  } catch {
    return [];
  }
}

/** Levenshtein distance, two-row variant — the engine's, in JavaScript. */
export function edit_distance(a, b) {
  let previous = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const current = [i];
    for (let j = 1; j <= b.length; j++) {
      current.push(Math.min(previous[j] + 1, current[j - 1] + 1,
                            previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1)));
    }
    previous = current;
  }
  return previous[b.length];
}

/**
 * The closest name, or null.
 *
 * The rule is the engine's, which suggests a color the same way: within two
 * edits, and fewer edits than the candidate has letters, so a short name cannot
 * match everything. Deliberately conservative — a wrong suggestion sends the
 * reader to a second wall, which is worse than sending them to the chapter.
 */
export function nearest_table(name, known) {
  const lower = name.trim().toLowerCase();
  let best = null, shortest = Infinity;
  for (const candidate of known) {
    const distance = edit_distance(lower, candidate);
    if (distance <= 2 && distance < candidate.length && distance < shortest) {
      best = candidate;
      shortest = distance;
    }
  }
  return best;
}

/**
 * What to say about a name the site does not have.
 *
 * A near-miss is named on its own, because it is the whole answer. Without one
 * the chapter is the answer, and the full list of names is not printed here:
 * the engine declines a color the same way, naming the one candidate or
 * pointing at the vocabulary, never reciting it.
 */
export function unknown_table(name, known) {
  const near = nearest_table(name, known);
  if (near !== null) {
    return `gog: there is no table called "${name}". Did you mean "${near}"?`;
  }
  return `gog: there is no table called "${name}". The table names are ` +
    `listed in the book's data chapter: ${BOOK_DATA_CHAPTER}`;
}

/**
 * The site answered nothing at all — a different problem, said differently.
 *
 * Kept apart from the unknown-name refusal because the two ask opposite things
 * of the reader: one is a name to correct, the other is a connection to check.
 * Telling someone their table does not exist when the network is down is the
 * confidently-wrong refusal this whole path exists to avoid.
 */
function unreachable(name) {
  return `gog: could not reach the book's data site to read "${name}". ` +
    `The tables are fetched from ${BOOK_DATA_URL}, so this needs a ` +
    "network connection.";
}

/**
 * Parse CSV text into rows of fields.
 *
 * A comma can sit inside a quoted field, so a line cannot be split on commas.
 * Exported for the test suite, which checks the quoting without a network call.
 */
export function parse_csv(text) {
  const rows = [[]];
  let field = "", quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c !== '"') field += c;
      else if (text[i + 1] === '"') { field += '"'; i++; }
      else quoted = false;
    } else if (c === '"') quoted = true;
    else if (c === ",") { rows.at(-1).push(field); field = ""; }
    else if (c === "\n") { rows.at(-1).push(field); field = ""; rows.push([]); }
    else if (c !== "\r") field += c;
  }
  rows.at(-1).push(field);
  if (rows.at(-1).length === 1 && rows.at(-1)[0] === "") rows.pop();
  return rows;
}

/**
 * Turn parsed rows into columns with the right types.
 *
 * A CSV is text, so every value arrives as text. A column becomes numbers when
 * every value in it parses as one, and stays text otherwise. Naming a column in
 * `text` keeps it text no matter what it looks like.
 */
export function columns(rows, text = []) {
  const [head, ...body] = rows;
  return Object.fromEntries(head.map((key, i) => {
    const values = body.map((row) => row[i]);
    if (text.includes(key)) return [key, values];
    const numbers = values.map(Number);
    return [key, numbers.some((n) => Number.isNaN(n)) ? values : numbers];
  }));
}

/**
 * Read one of the book's example tables.
 *
 * `name` is the table's name without the extension, such as "gapminder_2007";
 * the full list is in the book's data chapter. `text` names columns that must
 * stay text, because a CSV records what a value is and never what kind of thing
 * it is, so a column of 01, 02, 03 comes back as the numbers 1, 2, 3 otherwise.
 *
 *     const gapminder_2007 = await gog_table("gapminder_2007");
 *     plot(data(gapminder_2007), point, x(col.gdp), y(col.life));
 */
export async function gog_table(name, text = []) {
  if (typeof name !== "string") {
    throw new GogError(
      'gog: gog_table() takes one table name, as in gog_table("gapminder_2007"). ' +
      "The names are listed in the book's data chapter.",
    );
  }
  // A misspelt name is the commonest mistake this function has, and this binding
  // answered it worst of the four. `fetch` does not throw on a 404: it resolves,
  // and the body it resolves with is the site's 404 page. That page went
  // straight into the CSV reader above, which parsed it happily, so the caller
  // received an eighty-eight row table whose one column was named
  // `<!DOCTYPE html>`. The reader then debugged a `data()` refusal about a
  // column they never wrote, or plotted a web page. The status is the signal;
  // the body is not.
  let response;
  try {
    response = await fetch(`${BOOK_DATA_URL}${name}.csv`);
  } catch {
    throw new GogError(unreachable(name));
  }
  if (response.status === 404) {
    throw new GogError(unknown_table(name, await table_names()));
  }
  if (!response.ok) throw new GogError(unreachable(name));

  const body = (await response.text()).trim();
  return columns(parse_csv(body), text);
}
