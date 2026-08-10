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

import { GogError } from "./errors.js";

export const BOOK_DATA_URL = "https://psychometrician.github.io/gog-book/data/";

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
 *     const gapminder_2007 = await book_table("gapminder_2007");
 *     plot(data(gapminder_2007), point, x(col.gdp), y(col.life));
 */
export async function book_table(name, text = []) {
  if (typeof name !== "string") {
    throw new GogError(
      'gog: book_table() takes one table name, as in book_table("gapminder_2007"). ' +
      "The names are listed in the book's data chapter.",
    );
  }
  const body = (await (await fetch(`${BOOK_DATA_URL}${name}.csv`)).text()).trim();
  return columns(parse_csv(body), text);
}
