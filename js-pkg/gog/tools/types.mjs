// types.mjs — write `index.d.ts` from the JSDoc blocks in `src/*.js`.
//
//     node js-pkg/gog/tools/types.mjs
//
// The help a JavaScript reader sees on hover is the JSDoc block above each
// definition, and it is the one source for the TypeScript declarations too:
// this reads every block, builds one declaration per export from its `@param`
// and `@returns` types, and copies the block above it so that hover shows the
// same help in TypeScript. Nothing here is written by hand, so a change to the
// help is a change to the declarations once this is run again, and
// `test/types.test.mjs` fails until it is.
//
// The atoms are built at run time (`bareAtom`, `callableAtom`), and a reader
// of the code alone would type them as functions of anything. So the shape of
// each declaration comes from how its definition is built, and its types from
// its block:
//
//   * a class is declared with the members its help names, typed by the
//     constructor's `@param` of the same name, and its methods;
//   * a bare mark or transform (`bareAtom`) is an `Atom`: calling one throws;
//   * an atom that is also called (`callableAtom`) is `Atom` and a function at
//     once, unless its first argument is required, as `quantile`'s is, in
//     which case writing it bare is refused and it is a function alone;
//   * everything else is a function;
//   * a value whose block has an `@type` is declared with that type (`col`).
//
// Arguments read by `readArgs` follow its rule as well as the block's: each
// argument may be written in order, or by name in one options object. So an
// atom whose arguments are all optional also takes the options object alone
// (`bin({ width: 5 })`, `range({ high: 0.9 })`), and one whose block names no
// argument but options takes its options in order too (`space(60, 20)`).
//
// `object` in an argument's type means the function's own options object
// wherever the function has one, so `{object|string} [options]` becomes
// `{ name?: string } | string`, and `{...(Column|object)} levels` becomes the
// columns with an options object after them.
//
// It needs nothing installed: it reads source text with `tools/jsdoc.mjs`, and
// imports the package only to check that it declares every name exported.

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import * as gog from "../src/index.js";
import { PACKAGE, docBlock, exportOrder, parseBlock, sources } from "./jsdoc.mjs";

export const OUTPUT = path.join(PACKAGE, "index.d.ts");
export const COMMAND = "node js-pkg/gog/tools/types.mjs";

// ---------------------------------------------------------------------------
// JSDoc types, as TypeScript
// ---------------------------------------------------------------------------

// A JSDoc type expression, read into a tree: unions, intersections, generic
// references, string and number literals, object types, and `T[]`.
function parseType(text) {
  const tokens = [];
  const re = /\s*("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|\.\.\.|-?\d+(?:\.\d+)?|[A-Za-z_$][\w$]*|[|&()<>,{}:;[\]?=])/y;
  let at = 0;
  while (at < text.length) {
    re.lastIndex = at;
    const m = re.exec(text);
    if (!m) {
      if (/^\s*$/.test(text.slice(at))) break;
      throw new Error(`cannot read the type \`${text}\` at \`${text.slice(at)}\``);
    }
    tokens.push(m[1]);
    at = re.lastIndex;
  }
  let i = 0;
  const peek = () => tokens[i];
  const take = (want) => {
    const token = tokens[i++];
    if (want !== undefined && token !== want) {
      throw new Error(`cannot read the type \`${text}\`: expected \`${want}\`, got \`${token}\``);
    }
    return token;
  };
  const union = () => {
    const members = [intersection()];
    while (peek() === "|") { take(); members.push(intersection()); }
    return members.length === 1 ? members[0] : { kind: "union", members };
  };
  const intersection = () => {
    const members = [postfix()];
    while (peek() === "&") { take(); members.push(postfix()); }
    return members.length === 1 ? members[0] : { kind: "intersection", members };
  };
  const postfix = () => {
    let node = primary();
    while (peek() === "[" && tokens[i + 1] === "]") {
      take(); take();
      node = { kind: "ref", name: "Array", args: [node] };
    }
    return node;
  };
  const primary = () => {
    const token = take();
    if (token === "(") {
      const inner = union();
      take(")");
      return inner;
    }
    if (token === "[") {
      const items = [];
      while (peek() !== "]") { items.push(union()); if (peek() === ",") take(); }
      take("]");
      return { kind: "tuple", items };
    }
    if (token === "{") {
      const entries = [];
      while (peek() !== "}") {
        let key = take();
        if (key === "readonly" && peek() !== ":" && peek() !== "?") key = take();
        const optional = peek() === "?" ? (take(), true) : false;
        take(":");
        entries.push({ key, optional, type: union() });
        if (peek() === "," || peek() === ";") take();
      }
      take("}");
      return { kind: "object", entries };
    }
    if (/^["'-\d]/.test(token)) return { kind: "literal", text: token.replace(/^'(.*)'$/, '"$1"') };
    if (/^[A-Za-z_$]/.test(token)) {
      const args = [];
      if (peek() === "<") {
        take();
        while (peek() !== ">") { args.push(union()); if (peek() === ",") take(); }
        take(">");
      }
      return { kind: "ref", name: token, args };
    }
    throw new Error(`cannot read the type \`${text}\` at \`${token}\``);
  };
  const node = union();
  if (i !== tokens.length) throw new Error(`cannot read the type \`${text}\`: \`${tokens.slice(i).join(" ")}\` left over`);
  return node;
}

// The tree as TypeScript. `input` is true for what a caller hands in, where an
// array may be read-only. `object` as one member of the union an argument's
// type is, is replaced by the function's options object when it has one
// (`options`); `object` alone stays `object`, as a database connection's does.
function printType(node, { input = false, options = null } = {}) {
  const inner = (n) => printType(n, { input });
  const group = (n) => (n.kind === "union" || n.kind === "intersection" ? `(${inner(n)})` : inner(n));
  switch (node.kind) {
    case "union": {
      const members = node.members.map((m) =>
        options && m.kind === "ref" && m.name === "object" && !m.args.length ? options : inner(m));
      return [...new Set(members)].join(" | ");
    }
    case "intersection":
      return node.members.map(group).join(" & ");
    case "literal":
      return node.text;
    case "tuple":
      return `${input ? "readonly " : ""}[${node.items.map(inner).join(", ")}]`;
    case "object":
      return `{ ${node.entries.map((e) => `${e.key}${e.optional ? "?" : ""}: ${inner(e.type)}`).join("; ")} }`;
    case "ref": {
      const args = node.args;
      switch (node.name) {
        case "object":
          return "object";
        case "Object":
          return args.length === 2 ? `Record<${inner(args[0])}, ${inner(args[1])}>` : "object";
        case "Array": {
          const element = args.length ? group(args[0]) : "unknown";
          return `${input ? "readonly " : ""}${element}[]`;
        }
        case "Set":
          return `${input ? "ReadonlySet" : "Set"}<${args.map(inner).join(", ")}>`;
        case "Map":
          return `${input ? "ReadonlyMap" : "Map"}<${args.map(inner).join(", ")}>`;
        case "Promise":
          return `Promise<${args.map((a) => printType(a)).join(", ")}>`;
        default:
          return args.length ? `${node.name}<${args.map(inner).join(", ")}>` : node.name;
      }
    }
    default:
      throw new Error(`no TypeScript for a ${node.kind}`);
  }
}

const tsType = (text, settings) => printType(parseType(text), settings);

// Whether a JSDoc type names `object` among the members of its union.
function namesObject(text) {
  const node = parseType(text);
  const members = node.kind === "union" ? node.members : [node];
  return members.some((m) => m.kind === "ref" && m.name === "object" && !m.args.length);
}

// The same type with `object` taken out of its union.
function withoutObject(text) {
  const node = parseType(text);
  const members = (node.kind === "union" ? node.members : [node])
    .filter((m) => !(m.kind === "ref" && m.name === "object" && !m.args.length));
  return members.length === 1 ? members[0] : { kind: "union", members };
}

// ---------------------------------------------------------------------------
// Printing
// ---------------------------------------------------------------------------

// A comment block, at an indent, from its lines.
function comment(lines, indent = "") {
  const body = [...lines];
  while (body.length && !body[0].trim()) body.shift();
  while (body.length && !body.at(-1).trim()) body.pop();
  if (!body.length) return "";
  return [
    `${indent}/**`,
    ...body.map((line) => (line.trim() ? `${indent} * ${line}`.trimEnd() : `${indent} *`)),
    `${indent} */`,
    "",
  ].join("\n");
}

// The block as it stands above the definition, without the `@type` tag, whose
// type the declaration now carries.
function helpLines(block) {
  const out = [];
  let skipping = false;
  for (const line of block.split("\n")) {
    if (/^@\w+/.test(line)) skipping = /^@type\b/.test(line);
    if (!skipping) out.push(line);
  }
  return out;
}

// An `@param`'s description, filled again to the width of the comment it moves
// into: its first line was shortened by the tag in front of it.
function refill(text, width) {
  const out = [];
  for (const paragraph of text.split(/\n\s*\n/)) {
    if (out.length) out.push("");
    let line = "";
    for (const word of paragraph.split(/\s+/).filter(Boolean)) {
      if (line && line.length + 1 + word.length > width) {
        out.push(line);
        line = word;
      } else {
        line = line ? `${line} ${word}` : word;
      }
    }
    if (line) out.push(line);
  }
  return out;
}

// An options object as an inline type, each key with its own help, so that the
// help for a key shows where the key is written.
function optionsType(keys, indent) {
  if (!keys.length) return "{}";
  const inside = `${indent}  `;
  const members = keys.map((key) =>
    comment(refill(key.text, 80 - inside.length - 3), inside) +
      `${inside}${key.name}${key.optional ? "?" : ""}: ${tsType(key.type, { input: true })};`);
  return `{\n${members.join("\n")}\n${indent}}`;
}

// ---------------------------------------------------------------------------
// One export
// ---------------------------------------------------------------------------

// A function's call signatures, from its block and from how it reads its
// arguments. Each is `{ params: "...", returns: "..." }`, printed at `indent`.
function signatures(name, doc, readArgs, indent) {
  const tags = doc.paramTags;
  const top = tags.filter((p) => !p.name.includes("."));
  const children = (parent) => tags
    .filter((p) => p.name.startsWith(`${parent}.`))
    .map((p) => ({ ...p, name: p.name.slice(parent.length + 1), optional: true }));
  const returns = doc.returnsType ? tsType(doc.returnsType) : "void";
  const optionsParam = top.find((p) => p.name === "options" && children("options").length);
  const optionKeys = optionsParam ? children("options") : [];
  const optionsInline = optionsParam ? optionsType(optionKeys, indent) : null;

  // The arguments as the block states them.
  const written = [];
  for (const [index, param] of top.entries()) {
    const keys = children(param.name);
    if (param.rest) {
      // Columns, and then an options object if the function has one: the
      // options are folded into the rest, since nothing can follow it.
      if (namesObject(param.type) && optionsParam) {
        const element = printType(withoutObject(param.type), { input: true });
        const each = /[|&]/.test(element) ? `(${element})` : element;
        written.push(`...${param.name}: ${each}[] | [...${param.name}: ${each}[], options: ${optionsInline}]`);
        break;
      }
      const element = tsType(param.type, { input: true });
      written.push(`...${param.name}: ${/[|&]/.test(element) ? `(${element})` : element}[]`);
      continue;
    }
    // An argument with keys of its own (`options.wrap`, `props.color`) is an
    // object of those keys; `object` beside other types is the options object.
    const own = keys.length ? optionsType(keys, indent) : null;
    const type = own && param.type.trim() === "object"
      ? own
      : tsType(param.type, { input: true, options: own ?? optionsInline });
    // A later required argument keeps an earlier one required in TypeScript.
    const laterRequired = top.slice(index + 1).some((p) => !p.optional && !p.rest);
    written.push(`${param.name}${param.optional && !laterRequired ? "?" : ""}: ${type}`);
  }
  const out = [{ params: written.join(", "), returns }];

  if (readArgs) {
    const positional = top.filter((p) => p.name !== "options");
    positional.forEach((p, k) => {
      if (readArgs[k] !== p.name) {
        throw new Error(`\`${name}()\`: its block names \`${p.name}\` in place ${k + 1}, and \`readArgs\` reads \`${readArgs[k]}\` there`);
      }
    });
    for (const key of optionKeys) {
      if (!readArgs.includes(key.name)) {
        throw new Error(`\`${name}()\`: its block names \`options.${key.name}\`, and \`readArgs\` does not read it`);
      }
    }
    // Every argument optional: the options object may stand alone, and carry
    // the arguments by name too.
    if (positional.length && positional.every((p) => p.optional)) {
      const keys = [...positional.map((p) => ({ ...p, optional: true })), ...optionKeys];
      out.push({ params: `options: ${optionsType(keys, indent)}`, returns });
    }
    // Options and no argument: the options may also be written in order.
    if (!positional.length && optionKeys.length) {
      const byName = new Map(optionKeys.map((k) => [k.name, k]));
      const ordered = readArgs.map((n) => byName.get(n));
      if (ordered.every(Boolean)) {
        out.push({
          params: ordered.map((k) => `${k.name}?: ${tsType(k.type, { input: true })}`).join(", "),
          returns,
        });
      }
    }
  }
  return out;
}

// How the definition is built, which decides the shape of its declaration.
function shapeOf(def) {
  if (def.kind === "class") return "class";
  if (def.kind === "function") return "function";
  const init = def.text.slice(def.text.indexOf("=") + 1).trimStart();
  if (/^bareAtom\(/.test(init)) return "bare";
  if (/^callableAtom\(/.test(init)) return "callable";
  return "value";
}

function declareClass(name, def, doc, help) {
  const text = def.text;
  const heading = text.match(new RegExp(`class\\s+${name}(?:\\s+extends\\s+([\\w$.]+))?\\s*\\{`));
  const base = heading?.[1];
  const ctorAt = text.search(/\bconstructor\s*\(/);
  const ctorParams = signatures(name, doc, null, "  ")[0].params;
  // The private `brand` exists only in this file. It makes the class nominal,
  // so an object that merely has a `toString()` is not an `Atom`, and a `Plot`
  // and an `Arrangement` stay two types although they hold the same members.
  // It is a `private` member rather than `#private`, which a project that
  // targets ES5 refuses.
  const lines = [`export declare class ${name}${base ? ` extends ${base}` : ""} {`, "  private readonly brand;"];
  if (ctorAt >= 0) lines.push(`  constructor(${ctorParams});`);
  // A member is public when the help names it: `spec` and `frames` on a plot,
  // `name` on a column. What the constructor stores and the help does not name
  // is the package's own business.
  const named = new Set([...doc.description.matchAll(/`([\w$]+)`/g)].map((m) => m[1]));
  for (const m of text.matchAll(/^\s*this\.([\w$]+)\s*=\s*([\w$]+)\s*;/gm)) {
    const [, member, value] = m;
    const param = doc.paramTags.find((p) => p.name === value);
    if (!param || !named.has(member)) continue;
    lines.push(comment(refill(param.text, 80 - 5), "  ") +
      `  readonly ${member}: ${tsType(param.type)};`);
  }
  for (const m of text.matchAll(/^ {2}([\w$]+)\s*\(([^)]*)\)\s*\{/gm)) {
    const method = m[1];
    if (method === "constructor") continue;
    if (method !== "toString") {
      throw new Error(`\`${name}.${method}()\` has no known type; declare it in tools/types.mjs`);
    }
    lines.push("  toString(): string;");
  }
  lines.push("}");
  return comment(help) + lines.join("\n");
}

function declare(name, def, src) {
  const block = docBlock(def);
  if (!block) throw new Error(`\`${name}\` has no JSDoc block above its definition`);
  const doc = parseBlock(block);
  const help = helpLines(block);
  const shape = shapeOf(def);

  if (doc.type) return comment(help) + `export declare const ${name}: ${tsType(doc.type)};`;
  if (shape === "class") return declareClass(name, def, doc, help);
  if (shape === "bare") return comment(help) + `export declare const ${name}: Atom;`;
  if (shape === "value") {
    // A value with no `@type` is a function: a channel made by a helper, or an
    // arrow function such as `play`.
    if (typeof gog[name] !== "function") {
      throw new Error(`\`${name}\` is a value, and its block gives no \`@type\` for it`);
    }
  }

  const readArgs = src.readArgsList(def);
  const firstRequired = doc.paramTags.find((p) => !p.name.includes(".") && p.name !== "options")?.optional === false;
  if (shape === "callable" && !firstRequired) {
    const sigs = signatures(name, doc, readArgs, "  ");
    if (sigs.length === 1) {
      return comment(help) +
        `export declare const ${name}: Atom & ((${sigs[0].params}) => ${sigs[0].returns});`;
    }
    const members = sigs.map((s) => `  (${s.params}): ${s.returns};`).join("\n");
    return comment(help) + `export declare const ${name}: Atom & {\n${members}\n};`;
  }

  const sigs = signatures(name, doc, readArgs, "");
  return sigs
    .map((s) => comment(help) + `export declare function ${name}(${s.params}): ${s.returns};`)
    .join("\n\n");
}

// ---------------------------------------------------------------------------
// The file
// ---------------------------------------------------------------------------

// `Set` and `Map` are ES2015's, so they are asked for by name: a project whose
// `lib` is ES5, as `tsc` run on a file with no settings is, would not have them.
const HEADER = `// Type declarations for grammar-of-graphics.
//
// Generated from the JSDoc in src/*.js by tools/types.mjs. Do not edit this file:
// change the help in src/*.js and run \`${COMMAND}\` from the repository root.

/// <reference lib="es2015.collection" />
`;

export function generate() {
  const order = exportOrder();
  const runtime = Object.keys(gog).sort();
  const listed = [...order].sort();
  if (JSON.stringify(listed) !== JSON.stringify(runtime)) {
    const missing = runtime.filter((n) => !order.includes(n));
    const extra = order.filter((n) => !runtime.includes(n));
    throw new Error(
      `src/index.js lists its exports in a way this cannot read: ` +
        `missing ${missing.join(", ") || "none"}, extra ${extra.join(", ") || "none"}`
    );
  }
  const src = sources(runtime);
  const parts = order.map((name) => {
    const def = src.definitionOf(name);
    if (!def) throw new Error(`\`${name}\` has no \`export\` definition in src/*.js`);
    return declare(name, def, src);
  });
  return `${HEADER}\n${parts.join("\n\n")}\n`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const text = generate();
  fs.writeFileSync(OUTPUT, text);
  console.log(`wrote ${path.relative(process.cwd(), OUTPUT)}: ${Object.keys(gog).length} exports`);
}
