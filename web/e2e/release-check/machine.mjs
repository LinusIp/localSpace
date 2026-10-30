// The Windows side of the release check: installing, starting and stopping
// the app as a person's computer does, and reading what it wrote. Nothing
// here talks to the app's API; see check.mjs.

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Polls `check` until it returns something, or fails after `ms`. */
export async function until(what, check, ms = 30000, every = 500) {
  const start = Date.now();
  for (;;) {
    const value = await check();
    if (value) return value;
    if (Date.now() - start > ms) throw new Error(`timed out after ${Math.round(ms / 1000)} s waiting for ${what}`);
    await sleep(every);
  }
}

export const LOCALAPPDATA = process.env.LOCALAPPDATA ?? "";
/** Where the installer puts the program, for this user. */
export const INSTALL_DIR = path.join(LOCALAPPDATA, "Programs", "localSpace");
/** The data folder a first start makes for itself. */
export const DATA_DIR = path.join(LOCALAPPDATA, "localSpace");
export const APP_LOG = path.join(DATA_DIR, "logs", "app.log");

/** The one file in `dir` whose name ends with `suffix`. */
export function fileIn(dir, suffix) {
  const found = fs.readdirSync(dir).filter((f) => f.endsWith(suffix));
  if (found.length !== 1) throw new Error(`expected one *${suffix} in ${dir}, found ${found.length}`);
  return path.join(dir, found[0]);
}

/** Runs an installer silently, as an administrator's deployment would, and waits for it. */
export function installSilently(setup) {
  const run = spawnSync(setup, ["/S"], { stdio: "inherit", windowsHide: true });
  if (run.status !== 0) throw new Error(`${path.basename(setup)} exited with ${run.status}`);
  if (!fs.existsSync(path.join(INSTALL_DIR, "localspace-app.exe"))) throw new Error("the installer did not put localspace-app.exe in place");
}

/** The version the installed command line reports, for example "0.2.0". */
export function installedVersion() {
  const run = spawnSync(path.join(INSTALL_DIR, "localspace.exe"), ["--version"], { encoding: "utf8", windowsHide: true });
  if (run.status !== 0) throw new Error(`localspace.exe --version exited with ${run.status}`);
  return run.stdout.trim();
}

/** Lines of the app's log written from `fromLine` on. */
export function logLines(fromLine = 0) {
  if (!fs.existsSync(APP_LOG)) return [];
  return fs.readFileSync(APP_LOG, "utf8").split(/\r?\n/).slice(fromLine);
}

export function logLength() {
  return logLines().length;
}

/**
 * Starts the installed app as the Start menu would, and waits until its
 * server answers and its window is open. Returns the process, the client's
 * address and the log line the start began at.
 */
export async function startApp() {
  const from = logLength();
  const app = spawn(path.join(INSTALL_DIR, "localspace-app.exe"), [], { detached: true, stdio: "ignore" });
  let exited = false;
  app.on("exit", () => {
    exited = true;
  });
  const url = await until(
    "the app's server and window",
    () => {
      if (exited) throw new Error("the app exited while starting");
      const lines = logLines(from);
      const at = lines.map((l) => l.match(/web client at (http:\/\/[0-9.:]+\/)/)).find(Boolean);
      const open = lines.some((l) => l.includes("the window is open"));
      return at && open ? at[1] : null;
    },
    120000,
  );
  await until("the server's health", async () => (await fetch(`${url}healthz`).catch(() => null))?.ok, 30000);
  return { app, url, from };
}

/** Ends the app and everything it started (the engine ends with it). */
export async function stopApp(app) {
  if (app.exitCode === null) {
    spawnSync("taskkill", ["/PID", String(app.pid), "/T", "/F"], { windowsHide: true });
  }
  await until("the app to end", () => app.exitCode !== null || app.signalCode !== null, 30000);
  // The engine is ended by the job object; make sure no copy is left.
  await until("no engine left running", () => !spawnSync("tasklist", ["/FI", "IMAGENAME eq llama-server.exe"], { encoding: "utf8" }).stdout.includes("llama-server.exe"), 30000);
}

/** Uninstalls silently without "delete the application data": the program goes, the data stays. */
export async function uninstallKeepingData() {
  spawnSync(path.join(INSTALL_DIR, "uninstall.exe"), ["/S"], { windowsHide: true });
  await until("the program to be removed", () => !fs.existsSync(path.join(INSTALL_DIR, "localspace-app.exe")), 120000);
  return fs.existsSync(path.join(DATA_DIR, "db"));
}

/** The SHA-256 of a file, as Windows computes it. */
export function sha256(file) {
  const run = spawnSync("powershell", ["-NoProfile", "-Command", `(Get-FileHash -LiteralPath '${file.replace(/'/g, "''")}' -Algorithm SHA256).Hash`], { encoding: "utf8", windowsHide: true });
  if (run.status !== 0) throw new Error(`Get-FileHash failed for ${file}`);
  return run.stdout.trim().toLowerCase();
}
