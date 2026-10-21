#!/usr/bin/env node
// The notices of everything the Windows package ships that is not ours
// (internal plan 42 §3; docs/DECISIONS.md, 2026-10-21): the Rust crates
// linked into `localspace-app.exe` and `localspace.exe`, the JavaScript in
// the web client, the KaTeX fonts, and the components listed by hand in
// `packaging/licences/notices.json`. Each licence text is written once, with
// every component it covers. MIT, BSD, ISC and the others require their
// notices to travel with a binary, whatever our own licence is.
//
//   node scripts/third-party-notices.mjs [--out <file>] [--check]
//
// --out writes the file (default dist/package/licences/THIRD-PARTY-NOTICES.txt).
// --check writes nothing and fails when a shipped component has no licence
// text, so that a new dependency cannot ship without its notice.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const check = args.includes("--check");
const outAt = args.indexOf("--out");
const out = outAt >= 0 ? resolve(args[outAt + 1]) : join(root, "dist/package/licences/THIRD-PARTY-NOTICES.txt");

/** The two programs the package ships, for the platform it is built for. */
const ROOTS = ["localspace-shell", "localspace-cli"];
const PLATFORM = "x86_64-pc-windows-msvc";
const LICENCE_FILE = /^(licen[cs]e|copying|copyright|unlicense|notice)/i;

const texts = new Map(); // sha -> { text, users: Set<string> }
const missing = [];

function addText(text, user) {
  const clean = text.replace(/\r\n/g, "\n").trim();
  const sha = createHash("sha256").update(clean).digest("hex");
  if (!texts.has(sha)) texts.set(sha, { text: clean, users: new Set() });
  texts.get(sha).users.add(user);
}

function licenceFiles(dir) {
  let names;
  try {
    names = readdirSync(dir);
  } catch {
    return [];
  }
  const files = [];
  for (const name of names.sort()) {
    const path = join(dir, name);
    const isDir = statSync(path).isDirectory();
    if (!isDir && LICENCE_FILE.test(name)) files.push(path);
    // A LICENSES/ folder of SPDX texts, as some crates carry.
    if (isDir && /^licen[cs]es?$/i.test(name)) {
      for (const inner of readdirSync(path).sort()) files.push(join(path, inner));
    }
  }
  return files;
}

/** A licence text kept in the repository, for components that ship none of their own. */
function keptText(spdx) {
  const path = join(root, "packaging/licences/texts", `${spdx}.txt`);
  return existsSync(path) ? readFileSync(path, "utf8") : null;
}

// --- Rust: the crates linked into the shipped programs ----------------------

const cargo = process.env.CARGO ?? "cargo";
const metadata = JSON.parse(
  execFileSync(cargo, ["metadata", "--format-version", "1", "--locked", "--filter-platform", PLATFORM], {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
  }),
);
const packages = new Map(metadata.packages.map((p) => [p.id, p]));
const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));
const members = new Set(metadata.workspace_members);
const procMacro = (id) => packages.get(id).targets.some((t) => t.kind.includes("proc-macro"));
// Normal edges only, and never through a proc-macro: what runs at build time
// is not in the programs.
const linked = new Set();
const stack = metadata.workspace_members.filter((id) => ROOTS.includes(packages.get(id).name));
for (const id of stack) linked.add(id);
while (stack.length) {
  const id = stack.pop();
  for (const dep of nodes.get(id).deps) {
    if (!dep.dep_kinds.some((k) => (k.kind ?? "normal") === "normal")) continue;
    if (procMacro(dep.pkg) || linked.has(dep.pkg)) continue;
    linked.add(dep.pkg);
    stack.push(dep.pkg);
  }
}
let crates = 0;
for (const id of linked) {
  if (members.has(id)) continue;
  const p = packages.get(id);
  // MPL-2.0 asks a binary to say where the source is.
  const source = /MPL/.test(p.license ?? "") ? `, source: https://crates.io/crates/${p.name}/${p.version}` : "";
  const label = `${p.name} ${p.version} (${p.license ?? "see its licence file"}${source})`;
  crates++;
  const files = licenceFiles(dirname(p.manifest_path));
  for (const file of files) addText(readFileSync(file, "utf8"), label);
  if (files.length === 0) {
    // A crate that ships no licence file: the standard text of the licence
    // it declares (of an "OR", the permissive one we keep), its authors as
    // the holders.
    const standard = p.license && standardText(p.license, p.authors ?? [], p.name);
    if (standard) addText(standard, label);
    else missing.push(`Rust crate ${label}: no licence file`);
  }
}

/** The standard text of `expression`'s first option we keep, preferring MIT, then Apache, then BSD, then MPL. */
function standardText(expression, authors, name) {
  const options = expression.split(/\s+OR\s+|\//).map((o) => o.replace(/[()]/g, "").trim());
  const holders = authors.map((a) => a.replace(/\s*<[^>]*>/, "")).join(", ") || `the ${name} authors`;
  for (const choice of ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-3-Clause", "MPL-2.0"]) {
    if (!options.includes(choice)) continue;
    if (choice === "MIT") return `MIT License\n\nCopyright (c) ${holders}\n\n${keptText("MIT.body")}`;
    if (choice === "BSD-3-Clause") return `Copyright (c) ${holders}. All rights reserved.\n${keptText("BSD-3-Clause.body")}`;
    return keptText(choice);
  }
  return null;
}

// --- JavaScript: what the web client ships -----------------------------------

const web = join(root, "web");
const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));
function resolvePackage(from, name) {
  for (let dir = from; dir.startsWith(web); dir = dirname(dir)) {
    const candidate = join(dir, "node_modules", name);
    if (existsSync(join(candidate, "package.json"))) return candidate;
    if (dirname(dir) === dir) break;
  }
  return null;
}
const jsRoots = [web, ...readdirSync(join(web, "packages")).map((p) => join(web, "packages", p))].filter((d) =>
  existsSync(join(d, "package.json")),
);
const seen = new Set();
const todo = [];
for (const dir of jsRoots) for (const name of Object.keys(readJson(join(dir, "package.json")).dependencies ?? {})) todo.push([dir, name]);
let scripts = 0;
while (todo.length) {
  const [from, name] = todo.pop();
  if (name.startsWith("@localspace/")) continue;
  const dir = resolvePackage(from, name);
  if (!dir) {
    missing.push(`JavaScript package ${name}: not installed (run npm ci in web/)`);
    continue;
  }
  if (seen.has(dir)) continue;
  seen.add(dir);
  const pj = readJson(join(dir, "package.json"));
  const label = `${pj.name} ${pj.version} (${typeof pj.license === "string" ? pj.license : "see its licence file"})`;
  scripts++;
  const files = licenceFiles(dir);
  for (const file of files) addText(readFileSync(file, "utf8"), label);
  if (files.length === 0) missing.push(`JavaScript package ${label}: no licence file`);
  for (const dep of Object.keys(pj.dependencies ?? {})) todo.push([dir, dep]);
}

// --- The KaTeX fonts: SIL OFL 1.1, not the package's MIT ---------------------

const katexFonts = join(web, "node_modules/katex/dist/fonts");
if (existsSync(katexFonts)) {
  const families = [...new Set(readdirSync(katexFonts).map((f) => f.replace(/-[^-]+\.(woff2?|ttf)$/, "")))].sort();
  addText(
    [
      "Copyright (c) 2009-2010, Design Science, Inc. (<www.mathjax.org>)",
      "Copyright (c) 2014-2018 Khan Academy (<www.khanacademy.org>),",
      `with Reserved Font Names ${families.join(", ")}.`,
      "",
      "This Font Software is licensed under the SIL Open Font License, Version 1.1.",
      "",
      keptText("OFL-1.1"),
    ].join("\n"),
    `the KaTeX fonts (${families.length} families, in the web client)`,
  );
} else {
  missing.push("the KaTeX fonts: web/node_modules/katex is not installed");
}

// --- Components listed by hand --------------------------------------------

const listed = readJson(join(root, "packaging/licences/notices.json")).components;
for (const c of listed) {
  const path = join(root, "packaging/licences", c.text);
  if (!existsSync(path)) {
    missing.push(`${c.name}: its text ${relative(root, path)} is missing`);
    continue;
  }
  addText(readFileSync(path, "utf8"), `${c.name} (${c.licence}; ${c.where})`);
}

// --- Out --------------------------------------------------------------------

if (missing.length) {
  console.error(`third-party notices: ${missing.length} component(s) without a licence text:`);
  for (const m of missing) console.error(`  - ${m}`);
  process.exit(1);
}
console.log(
  `third-party notices: ${crates} Rust crates, ${scripts} JavaScript packages and ${listed.length + 1} other components; ${texts.size} distinct texts`,
);
if (check) process.exit(0);

const rule = "=".repeat(78);
const body = [...texts.values()]
  .map(({ text, users }) => [rule, "Used by:", ...[...users].sort().map((u) => `  - ${u}`), rule, "", text, ""].join("\n"))
  .join("\n");
mkdirSync(dirname(out), { recursive: true });
writeFileSync(
  out,
  [
    "localSpace: notices of third-party software",
    "",
    "localSpace includes the software below, each under its own licence. Each",
    "licence text is given once, with the components it covers. The models you",
    "download in the app are not part of localSpace and keep their own licences,",
    "shown in the app beside each model.",
    "",
    body,
  ].join("\n"),
);
console.log(`written to ${relative(root, out)}`);
