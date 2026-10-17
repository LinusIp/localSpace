// The version, set in two places, must say one thing: the workspace's
// Cargo.toml (what the app and its API report, the About screen included) and
// the shell's tauri.conf.json (what the installer and the package's file names
// carry). Given a tag, the tag must name that version: v0.2.0 for 0.2.0, or
// v0.2.0-rc.N for one of its release candidates (docs/DECISIONS.md,
// 2026-10-18). Run by `ci` on every push, and by `package` before a tag's
// package is built and again before it is published.
//
//   node scripts/check-version.mjs            # the two files agree
//   node scripts/check-version.mjs v0.2.0     # and the tag names their version
import { readFileSync } from "node:fs";

const fail = (why) => {
  console.error(`::error::${why}`);
  process.exit(1);
};

const cargo = readFileSync("Cargo.toml", "utf8").replace(/\r\n/g, "\n");
const section = cargo.split(/^\[/m).find((s) => s.startsWith("workspace.package]"));
const fromCargo = section?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!fromCargo) fail("no version in Cargo.toml's [workspace.package]");

const fromTauri = JSON.parse(readFileSync("crates/localspace-shell/tauri.conf.json", "utf8")).version;
if (fromTauri !== fromCargo) {
  fail(`Cargo.toml says ${fromCargo} and crates/localspace-shell/tauri.conf.json says ${fromTauri}: they must agree`);
}

const tag = process.argv[2];
if (tag !== undefined) {
  const named = tag.match(/^v(\d+\.\d+\.\d+)(?:-rc\.\d+)?$/)?.[1];
  if (!named) fail(`${tag} is not a release tag (v0.2.0) or a release candidate's (v0.2.0-rc.1)`);
  if (named !== fromCargo) fail(`the tag ${tag} names ${named}, and the code's version is ${fromCargo}`);
}

console.log(tag ? `the version is ${fromCargo}, and ${tag} names it` : `the version is ${fromCargo} in both places`);
