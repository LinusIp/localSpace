// The inference engine a package carries: the upstream llama.cpp release
// named in scripts/engine.json, verified by size and SHA-256 before a byte of
// it is unpacked. A mismatch fails the build (docs/DECISIONS.md, 2026-09-18).
// Only the files the engine needs are kept: fewer executables to sign, and
// no RPC backend in a product whose only socket is the gateway.
//
//   node scripts/fetch-engine.mjs                       this platform, into dist/package/engine
//   node scripts/fetch-engine.mjs --platform windows-x64 --out <dir>
//   node scripts/fetch-engine.mjs --from <archive>      an archive already on disk, verified the same way
//
// The download is a build step on a connected machine; the product itself
// never downloads an executable.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream, createWriteStream, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const value = (flag) => {
  const at = args.indexOf(flag);
  return at >= 0 ? args[at + 1] : undefined;
};

const pin = JSON.parse(readFileSync(join(root, "scripts/engine.json"), "utf8"));
const platform = value("--platform") ?? `${process.platform === "win32" ? "windows" : process.platform}-${process.arch}`;
const asset = pin.assets[platform];
if (!asset) fail(`scripts/engine.json pins no engine for ${platform} (it has: ${Object.keys(pin.assets).join(", ")})`);
const out = resolve(root, value("--out") ?? "dist/package/engine");
const cache = resolve(root, "dist/engine-cache");
const archive = value("--from") ? resolve(value("--from")) : join(cache, asset.file);

function fail(message) {
  console.error(`fetch-engine: ${message}`);
  process.exit(1);
}

async function sha256(file) {
  const hash = createHash("sha256");
  await pipeline(createReadStream(file), hash);
  return hash.digest("hex");
}

/** Why the archive is not the pinned one, or nothing when it is. */
async function mismatch(file) {
  const bytes = statSync(file).size;
  if (bytes !== asset.bytes) return `${bytes} bytes, the pin says ${asset.bytes}`;
  const digest = await sha256(file);
  if (digest !== asset.sha256) return `SHA-256 ${digest}, the pin says ${asset.sha256}`;
  return undefined;
}

async function download() {
  const url = pin.source + asset.file;
  console.log(`fetching ${url}`);
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok || !response.body) fail(`${url} answered ${response.status}`);
  mkdirSync(cache, { recursive: true });
  await pipeline(Readable.fromWeb(response.body), createWriteStream(archive));
}

function unpack() {
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  // Windows' own tar reads zip archives; the one Git for Windows puts first
  // on PATH does not, so it is named in full.
  const tar = process.platform === "win32" ? join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe") : "tar";
  execFileSync(tar, ["-xf", archive, "-C", out], { stdio: "inherit" });
}

/** Keep what the pin lists (a `*` matches within a name) and say what is missing. */
function prune() {
  const patterns = asset.keep.map((k) => new RegExp(`^${k.split("*").map((s) => s.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*")}$`));
  const present = readdirSync(out);
  for (const name of present) {
    if (!patterns.some((p) => p.test(name))) rmSync(join(out, name), { recursive: true, force: true });
  }
  const missing = asset.keep.filter((k, i) => !present.some((name) => patterns[i].test(name)));
  if (missing.length) fail(`release ${pin.release} has no ${missing.join(", ")}: its layout changed, so the keep list in scripts/engine.json needs reading again`);
  if (!existsSync(join(out, asset.binary))) fail(`${asset.binary} is not in ${out}`);
}

if (value("--from")) {
  if (!existsSync(archive)) fail(`${archive} does not exist`);
} else if (!existsSync(archive) || (await mismatch(archive))) {
  await download();
}
const wrong = await mismatch(archive);
if (wrong) {
  if (!value("--from")) rmSync(archive, { force: true });
  fail(`${asset.file} is not the pinned release ${pin.release}: ${wrong}. Nothing was unpacked.`);
}
unpack();
prune();
const kept = readdirSync(out);
const size = kept.reduce((sum, name) => sum + statSync(join(out, name)).size, 0);
console.log(`engine ${pin.release} (${platform}): ${kept.length} files, ${(size / 1e6).toFixed(1)} MB, in ${out}`);
