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
// Signing. LOCALSPACE_SIGN_COMMAND is what signs one file, as JSON in the form
// tauri's `bundle.windows.signCommand` takes: {"cmd": "…", "args": ["…", "%1"]},
// with %1 where the file goes. When it is set, **every program file of the
// package is signed**, the engine's and the command line too and not only
// the installer: Smart App Control judges each executable and each library
// a program loads. tauri signs the app, the installer and the uninstaller
// with the same command. Without it nothing is signed, which is the test
// build (docs/DECISIONS.md, 2026-09-18 and 2026-09-19).
//
// Before it: `npm run build` in web/ and `cargo build --release -p
// localspace-cli`; the harness packages it builds itself, through
// scripts/hpack.mjs (cargo with wasm32-wasip2, and npm). The installer step needs
// tauri-cli (`cargo install tauri-cli --version 2.11.4 --locked`). Honours
// CARGO_TARGET_DIR and LOCALSPACE_BUILD_ID.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
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

// --- signing ------------------------------------------------------------------

const signing = (() => {
  const raw = process.env.LOCALSPACE_SIGN_COMMAND;
  if (!raw) return undefined;
  let command;
  try {
    command = JSON.parse(raw);
  } catch {
    fail('LOCALSPACE_SIGN_COMMAND is not JSON; it reads {"cmd": "…", "args": ["…", "%1"]}');
  }
  if (typeof command.cmd !== "string" || !Array.isArray(command.args) || !command.args.includes("%1")) {
    fail('LOCALSPACE_SIGN_COMMAND needs "cmd" and "args", with "%1" where the file goes');
  }
  return command;
})();

/** Every executable and library under `dir`. */
function programFiles(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return programFiles(path);
    return /\.(exe|dll)$/i.test(name) ? [path] : [];
  });
}

/** The command's arguments may hold a secret: the file is named, the command is not. */
function sign(file) {
  console.log(`signing ${file}`);
  execFileSync(signing.cmd, signing.args.map((arg) => (arg === "%1" ? file : arg)), { cwd: root, stdio: ["ignore", "inherit", "inherit"] });
}

function isSigned(file) {
  const out = execFileSync("powershell", ["-NoProfile", "-NonInteractive", "-Command", `[bool](Get-AuthenticodeSignature -LiteralPath '${file.replaceAll("'", "''")}').SignerCertificate`], { encoding: "utf8" });
  return out.trim() === "True";
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
if (signing) {
  const files = programFiles(stage);
  for (const file of files) sign(file);
  console.log(`signed ${files.length} program files of the stage`);
}
if (stageOnly) process.exit(0);

// --- the installer ----------------------------------------------------------

// With a signing command, tauri is given it too: it signs the app, the
// installer and the uninstaller. The settings file it reads is written for
// this build and taken away after it.
let bundleConfig = "tauri.bundle.conf.json";
if (signing) {
  const settings = JSON.parse(readFileSync(join(shell, bundleConfig), "utf8"));
  settings.bundle.windows.signCommand = signing;
  bundleConfig = "tauri.bundle.signed.conf.json";
  writeFileSync(join(shell, bundleConfig), JSON.stringify(settings, null, 2));
}
try {
  run("cargo", ["tauri", "build", "--config", bundleConfig], shell);
} finally {
  if (signing) rmSync(join(shell, bundleConfig), { force: true });
}
const nsis = join(target, "release/bundle/nsis");
const setups = existsSync(nsis) ? readdirSync(nsis).filter((name) => name.endsWith("-setup.exe")) : [];
if (setups.length !== 1) fail(`expected one installer in ${nsis}, found ${setups.length}`);
const out = join(root, "dist/windows");
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const stem = `localSpace-${version}-${build}-windows-x64`;
const installer = join(out, `${stem}-setup.exe`);
cpSync(join(nsis, setups[0]), installer);
if (signing && !isSigned(installer)) sign(installer);

// --- the portable copy: the same files, no installer ------------------------

const app = join(target, "release/localspace-app.exe");
need(app, "the installer step builds it");
if (signing && !isSigned(app)) sign(app);
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
