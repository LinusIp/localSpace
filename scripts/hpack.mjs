// A harness's package layout: the contents of its `.hpack` (architecture v2.1
// §7; plugin spec §12) — `harness.toml`, `logic.wasm`, `ui/`, `evals.json`
// and every other file the manifest names — built from wherever their
// sources live in this repository. Where a source lives is recorded in
// scripts/harnesses.json, a repository detail; the layout is the package's.
// Zipping and signing a layout into an `.hpack` is the packaging step's
// (architecture §13 step 10).
//
//   node scripts/hpack.mjs                  every harness, into dist/hpack/<id>-<version>/
//   node scripts/hpack.mjs <id> --out <dir> one harness, somewhere else
//   node scripts/hpack.mjs --in-place       also put the built files into the manifest's own
//                                           directory, which `--harnesses`, `--registry` and
//                                           the tests load
//   node scripts/hpack.mjs --no-build       assemble from what is already built
//
// Needs cargo with the wasm32-wasip2 and wasm32-unknown-unknown targets, and
// npm for a web surface. Honours CARGO_TARGET_DIR.

import { execFileSync, execSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const outAt = args.indexOf("--out");
const outRoot = resolve(root, outAt >= 0 && args[outAt + 1] ? args[outAt + 1] : "dist/hpack");
const build = !args.includes("--no-build");
const inPlace = args.includes("--in-place");
const { harnesses } = JSON.parse(readFileSync(join(root, "scripts/harnesses.json"), "utf8"));
const wanted = args.filter((a, i) => !a.startsWith("--") && i !== outAt + 1);
const ids = wanted.length ? wanted : Object.keys(harnesses);

function run(command, argv, cwd) {
  console.log(`$ ${command} ${argv.join(" ")}   (in ${relative(root, cwd) || "."})`);
  // npm is a script on Windows, found only through a shell. Its arguments
  // come from scripts/harnesses.json, so one command line is safe to hand it.
  if (command === "npm" && process.platform === "win32") execSync(`npm ${argv.join(" ")}`, { cwd, stdio: "inherit" });
  else execFileSync(command, argv, { cwd, stdio: "inherit" });
}

const targetDir = (crate) => (process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR) : join(root, crate, "target"));

/** A wasm artefact built by cargo for a target, from its crate. */
function cargoWasm({ crate, target, artifact }) {
  if (build) run("cargo", ["build", "--release", "--target", target], join(root, crate));
  const file = join(targetDir(crate), target, "release", artifact);
  if (!existsSync(file)) throw new Error(`${artifact} is not built at ${file}${build ? "" : "; run without --no-build"}`);
  return file;
}

/** A web surface built by an npm script in web/, into its output directory. */
function webSurface({ script, out }) {
  if (build) run("npm", ["run", script], join(root, "web"));
  const dir = join(root, out);
  if (!existsSync(join(dir, "index.js"))) throw new Error(`the web surface is not built at ${dir}${build ? "" : "; run without --no-build"}`);
  return dir;
}

/** What a manifest names, from the lines that name it: its version, logic, tools and view modules. */
function named(manifest) {
  const text = readFileSync(manifest, "utf8");
  const values = (key) => [...text.matchAll(new RegExp(`\\b${key}\\s*=\\s*"([^"]+)"`, "g"))].map((m) => m[1]);
  return { version: values("version")[0], logic: values("logic")[0], tools: values("tools")[0], modules: values("module") };
}

function tree(dir, base = dir) {
  const lines = [];
  for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) lines.push(...tree(path, base));
    else lines.push(`${relative(base, path).replaceAll("\\", "/")}  ${statSync(path).size} B`);
  }
  return lines;
}

function copy(from, to) {
  mkdirSync(dirname(to), { recursive: true });
  cpSync(from, to, { recursive: true });
}

for (const id of ids) {
  const spec = harnesses[id];
  if (!spec) throw new Error(`no harness ${id} in scripts/harnesses.json`);
  const manifestDir = join(root, spec.manifest);
  const names = named(join(manifestDir, "harness.toml"));
  if (!names.version) throw new Error(`${id}: the manifest has no version`);
  const out = join(outRoot, `${id}-${names.version}`);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });

  // What the manifest's directory holds as source: the manifest and its data.
  for (const file of ["harness.toml", names.tools, "evals.json", "icon.svg"]) {
    if (file && existsSync(join(manifestDir, file))) copy(join(manifestDir, file), join(out, file));
  }
  // What is built, from wherever its source lives.
  const built = [];
  if (spec.logic) built.push([names.logic ?? "logic.wasm", cargoWasm(spec.logic)]);
  for (const [path, source] of Object.entries(spec.files ?? {})) built.push([path, source.crate ? cargoWasm(source) : webSurface(source)]);
  for (const [path, from] of built) {
    copy(from, join(out, path));
    if (inPlace && resolve(from) !== resolve(manifestDir, path)) copy(from, join(manifestDir, path));
  }

  // Every file the manifest names is in the layout.
  const missing = [names.logic, names.tools, ...names.modules].filter((p) => p && !existsSync(join(out, p)));
  if (missing.length) throw new Error(`${id}: the layout lacks ${missing.join(", ")}`);
  console.log(`${id} ${names.version} -> ${relative(root, out).replaceAll("\\", "/")}`);
  for (const line of tree(out)) console.log(`  ${line}`);
}
