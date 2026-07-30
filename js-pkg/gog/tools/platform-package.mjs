// Build one platform package: a package.json, the engine, and the license.
//
//   node tools/platform-package.mjs darwin-arm64 ../../target/release/gog-cli
//
// The engine ships as one npm package per platform, named
// `grammar-of-graphics-<platform>-<arch>` and listed under the main package's
// `optionalDependencies`. npm's `os`/`cpu` fields make it install exactly one of
// them, so a user downloads a single 2 MB engine rather than five. `render.js`
// resolves it by name. There is no install script anywhere in the chain, which
// is why `--ignore-scripts` cannot leave someone without an engine.
//
// **These packages are generated, never committed.** That is the point of the
// script rather than a tidiness preference: five checked-in manifests would be
// five more version numbers to move in step with the other six, and "one
// grammar, one number" is an invariant a test enforces. Generating them from the
// main `package.json` means the number exists once and is copied, so it cannot
// drift.
//
// The license is copied into every one of them. Apache 2.0 §4(a) binds whoever
// hands out a copy to hand out the License with it, and each of these is a
// separate artifact on a public registry — the repository's `LICENSE` sits far
// above where an installer of `grammar-of-graphics-linux-x64` can reach.

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const PKG = path.join(HERE, "..");

function die(message) {
  process.stderr.write(`platform-package: ${message}\n`);
  process.exit(1);
}

const [target, enginePath, outArg] = process.argv.slice(2);
if (!target || !enginePath) {
  die("usage: platform-package.mjs <platform>-<arch> <path-to-gog-cli> [outdir]");
}

const manifest = JSON.parse(fs.readFileSync(path.join(PKG, "package.json"), "utf8"));
const name = `${manifest.name}-${target}`;

// The main package's `optionalDependencies` is the list of platforms that exist,
// and `render.js`'s `ENGINE_PLATFORMS` is the same list a second time. Refusing
// an unlisted target here is what keeps a release from quietly publishing a
// sixth package nothing resolves — the engine would be on the registry and no
// install would ever ask for it.
if (!Object.hasOwn(manifest.optionalDependencies ?? {}, name)) {
  die(
    `\`${target}\` is not a built platform. \`package.json\` lists:\n` +
      Object.keys(manifest.optionalDependencies ?? {})
        .map((key) => `  ${key.slice(manifest.name.length + 1)}`)
        .join("\n") +
      "\nAdd it there and to `ENGINE_PLATFORMS` in src/render.js first."
  );
}

// A pin that is not this version would publish a package the main one never
// resolves, which npm reports as nothing at all.
const pinned = manifest.optionalDependencies[name];
if (pinned !== manifest.version) {
  die(`\`${name}\` is pinned at ${pinned} but the package is ${manifest.version}`);
}

if (!fs.existsSync(enginePath)) die(`no engine at ${enginePath}`);

const [platform, arch] = target.split("-");
const exe = platform === "win32" ? "gog-cli.exe" : "gog-cli";

const out = path.resolve(outArg ?? path.join(PKG, "dist"), name);
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(path.join(out, "bin"), { recursive: true });

fs.writeFileSync(
  path.join(out, "package.json"),
  `${JSON.stringify(
    {
      name,
      version: manifest.version,
      description: `The gog engine for ${target}. Installed automatically by \`${manifest.name}\`; nothing imports it directly.`,
      homepage: manifest.homepage,
      repository: manifest.repository,
      license: manifest.license,
      author: manifest.author,
      // What makes npm skip the other four. A package whose `os`/`cpu` do not
      // match is not downloaded at all, rather than downloaded and ignored.
      os: [platform],
      cpu: [arch],
      // Yarn Berry zips a package by default and cannot execute a file inside a
      // zip. This asks it to unpack — npm and pnpm always do.
      preferUnplugged: true,
      files: ["bin", "LICENSE", "NOTICE"],
    },
    null,
    2
  )}\n`
);

const engine = path.join(out, "bin", exe);
fs.copyFileSync(enginePath, engine);
// npm records a file's mode in the tarball, so the bit set here is the bit the
// user gets. Windows has no such bit and does not need one.
if (platform !== "win32") fs.chmodSync(engine, 0o755);

for (const file of ["LICENSE", "NOTICE"]) {
  fs.copyFileSync(path.join(PKG, file), path.join(out, file));
}

fs.writeFileSync(
  path.join(out, "README.md"),
  `# ${name}

The [gog](${manifest.homepage}) graphics engine, compiled for \`${target}\`.

This package holds one executable and no JavaScript. It is one of five platform
builds that [\`${manifest.name}\`](https://www.npmjs.com/package/${manifest.name})
lists as optional dependencies, so installing that package brings the right
engine for your machine and skips the other four. Install it directly only to
repair an install that skipped it.

Apache License 2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
`
);

process.stdout.write(`${name} → ${out}\n`);
