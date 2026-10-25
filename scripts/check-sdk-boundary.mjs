#!/usr/bin/env node
// The plugin SDK's folder is Apache 2.0 while the rest of the repository is
// not yet (docs/DECISIONS.md, 2026-10-25), so nothing in `sdk/` may come from
// outside it: no path that leaves the folder, and no dependency on the
// app's crates or packages, which still hold the organisation code
// (internal plan 42 §1). This check fails the build when one does.
//
//   node scripts/check-sdk-boundary.mjs

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sdk = join(root, "sdk");

/** The app's own crates and packages: an SDK file may name none of them. */
const APP_CRATES = [
  "localspace-proto",
  "localspace-core",
  "localspace-server",
  "localspace-cli",
  "localspace-shell",
  "localspace-desktop",
  "localspace-client",
];
const APP_PACKAGES = /^@localspace\/(ui|canvas|libs|web)\b/;
const TEXT = /\.(rs|toml|ts|tsx|js|mjs|cjs|json|wit|md|css|html)$/;

const problems = [];
const inside = (path) => {
  const rel = relative(sdk, path);
  return rel === "" || (!rel.startsWith("..") && !rel.includes(`..${sep}`));
};

function walk(dir) {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "target") continue;
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path);
    else if (TEXT.test(name)) check(path);
  }
}

function check(file) {
  const text = readFileSync(file, "utf8");
  const where = relative(root, file);
  // Relative paths in imports, includes, `path =` dependencies and
  // `#[path]` attributes must stay inside the folder.
  for (const match of text.matchAll(/["'`]((?:\.\.?\/)+[^"'`\s]*)["'`]/g)) {
    if (!inside(resolve(dirname(file), match[1]))) problems.push(`${where}: "${match[1]}" leaves sdk/`);
  }
  // Prose may name the crates to say they are off limits; code may not.
  for (const crate of file.endsWith(".md") ? [] : APP_CRATES) {
    if (new RegExp(`\\b${crate}\\b`).test(text)) problems.push(`${where}: names the app's crate ${crate}`);
  }
  if (file.endsWith("package.json")) {
    const pj = JSON.parse(text);
    for (const kind of ["dependencies", "devDependencies", "peerDependencies"]) {
      for (const name of Object.keys(pj[kind] ?? {})) {
        if (APP_PACKAGES.test(name)) problems.push(`${where}: depends on the app's package ${name}`);
      }
    }
  }
}

if (!existsSync(join(sdk, "LICENSE"))) problems.push("sdk/LICENSE is missing");
if (existsSync(sdk)) walk(sdk);

if (problems.length) {
  console.error(`the SDK boundary: ${problems.length} problem(s):`);
  for (const p of problems) console.error(`  - ${p}`);
  process.exit(1);
}
console.log("the SDK boundary: nothing in sdk/ comes from outside it");
