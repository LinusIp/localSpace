// The Windows package a tester gets: an NSIS installer through tauri-cli and
// a portable zip of the same files (docs/DECISIONS.md, 2026-09-12 and
// 2026-09-18). What goes in: the desktop app (Core inside it), the command
// line, the web client, the Store's catalog, and the pinned llama.cpp Vulkan
// engine, verified by scripts/fetch-engine.mjs before it is unpacked.
//
//   node scripts/package.mjs                 stage, build the installer, zip the portable copy
//   node scripts/package.mjs --stage-only    lay out dist/package/ and stop
//   node scripts/package.mjs --engine-from <archive>   an engine archive already on disk
//   node scripts/package.mjs --no-harness-build        assemble the harnesses from what is already built
//
// Before it: `npm run build` in web/ and `cargo build --release -p
// localspace-cli`; the harness packages it builds itself, through
// scripts/hpack.mjs (cargo with wasm32-wasip2, and npm). The installer step needs
// tauri-cli (`cargo install tauri-cli --version 2.11.4 --locked`). Honours
// CARGO_TARGET_DIR and LOCALSPACE_BUILD_ID.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const stageOnly = args.includes("--stage-only");
const engineFrom = args.includes("--engine-from") ? args[args.indexOf("--engine-from") + 1] : undefined;

if (process.platform !== "win32") fail("this script builds the Windows package; the Linux tarball comes with the server test");

const target = resolve(root, process.env.CARGO_TARGET_DIR ?? "target");
const shell = join(root, "crates/localspace-shell");
const version = JSON.parse(readFileSync(join(shell, "tauri.conf.json"), "utf8")).version;
const build = process.env.LOCALSPACE_BUILD_ID ?? "dev";
const stage = join(root, "dist/package");

function fail(message) {
  console.error(`package: ${message}`);
  process.exit(1);
}

function need(path, how) {
  if (!existsSync(path)) fail(`${path} is missing: ${how}`);
}

function run(command, argv, cwd) {
  console.log(`$ ${command} ${argv.join(" ")}`);
  execFileSync(command, argv, { cwd, stdio: "inherit" });
}

// --- what the package is made of -------------------------------------------

const cli = join(target, "release/localspace.exe");
need(join(root, "web/dist/index.html"), "run `npm run build` in web/");
need(cli, "run `cargo build --release -p localspace-cli`");

rmSync(stage, { recursive: true, force: true });
mkdirSync(stage, { recursive: true });
cpSync(join(root, "web/dist"), join(stage, "web"), { recursive: true });
// The Store's catalog: every harness of this repository at its current
// version, laid out afresh so that no older layout rides along.
run(process.execPath, [join(root, "scripts/hpack.mjs"), "--out", join(stage, "registry"), ...(args.includes("--no-harness-build") ? ["--no-build"] : [])], root);
const packages = readdirSync(join(stage, "registry"));
cpSync(join(root, "packaging/licences"), join(stage, "licences"), { recursive: true });
cpSync(join(root, "packaging/windows/README.txt"), join(stage, "README.txt"));
cpSync(cli, join(stage, "localspace.exe"));
run(process.execPath, [join(root, "scripts/fetch-engine.mjs"), "--platform", "windows-x64", "--out", join(stage, "engine"), ...(engineFrom ? ["--from", engineFrom] : [])], root);
console.log(`staged in ${stage}: the client, ${packages.length} harness packages (${packages.join(", ")}), the engine, the command line`);
if (stageOnly) process.exit(0);

// --- the installer ----------------------------------------------------------

run("cargo", ["tauri", "build", "--config", "tauri.bundle.conf.json"], shell);
const nsis = join(target, "release/bundle/nsis");
const setups = existsSync(nsis) ? readdirSync(nsis).filter((name) => name.endsWith("-setup.exe")) : [];
if (setups.length !== 1) fail(`expected one installer in ${nsis}, found ${setups.length}`);
const out = join(root, "dist/windows");
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const stem = `localSpace-${version}-${build}-windows-x64`;
const installer = join(out, `${stem}-setup.exe`);
cpSync(join(nsis, setups[0]), installer);

// --- the portable copy: the same files, no installer ------------------------

const app = join(target, "release/localspace-app.exe");
need(app, "the installer step builds it");
const portable = join(root, "dist/portable/localSpace");
rmSync(dirname(portable), { recursive: true, force: true });
mkdirSync(dirname(portable), { recursive: true });
cpSync(stage, portable, { recursive: true });
cpSync(app, join(portable, "localspace-app.exe"));
const zip = join(out, `${stem}-portable.zip`);
// Windows' own tar writes a zip when asked for one by name.
run(join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe"), ["-a", "-cf", zip, "-C", dirname(portable), basename(portable)], root);

const sums = [installer, zip].map((file) => `${createHash("sha256").update(readFileSync(file)).digest("hex")}  ${basename(file)}`);
writeFileSync(join(out, "SHA256SUMS.txt"), sums.join("\n") + "\n");
console.log(sums.join("\n"));
console.log(`in ${out}`);
