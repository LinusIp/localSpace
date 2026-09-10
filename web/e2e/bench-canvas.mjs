// The canvas benchmark, headless, against a baseline (docs/DECISIONS.md,
// 2026-09-10: the gate number comes from the W32 machine; CI keeps a
// frame-time regression check without asserting 60 fps).
//
//   node e2e/bench-canvas.mjs                 run, compare with the baseline for this profile
//   node e2e/bench-canvas.mjs --record        run, write the baseline
//   node e2e/bench-canvas.mjs --profile w32   name the machine class (default: ci)
//   node e2e/bench-canvas.mjs --shapes 5000 --frames 240
//
// Serves the page with Vite's own dev server on a free port and drives Edge
// or Chrome already on the machine through playwright-core; nothing is
// downloaded. A run is a regression when its p95 frame time exceeds the
// baseline's by more than the tolerance (25 %), and the exit code says so.

import { createServer } from "vite";
import { chromium } from "playwright-core";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";

const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] && !args[i + 1].startsWith("--") ? args[i + 1] : fallback;
};
const record = args.includes("--record");
const profile = flag("profile", "ci");
const shapes = Number(flag("shapes", "5000"));
const frames = Number(flag("frames", "240"));
const tolerance = Number(flag("tolerance", "0.25"));
const baselinePath = resolve(`packages/canvas/bench/baseline.${profile}.json`);

const server = await createServer({ configFile: false, root: process.cwd(), server: { port: 0, strictPort: false }, logLevel: "silent" });
await server.listen();
const origin = server.resolvedUrls?.local[0]?.replace(/\/$/, "") ?? "http://127.0.0.1:5173";

let browser = null;
for (const channel of ["msedge", "chrome"]) {
  try {
    browser = await chromium.launch({ channel, headless: true });
    break;
  } catch {
    // try the next one
  }
}
if (!browser) {
  await server.close();
  throw new Error("neither Edge nor Chrome could be launched");
}

try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
  page.on("pageerror", (e) => console.error(`[page] ${e.message}`));
  await page.goto(`${origin}/packages/canvas/bench/index.html?shapes=${shapes}&frames=${frames}`);
  await page.waitForFunction(() => Boolean(window.__bench), null, { timeout: 120000 });
  const result = await page.evaluate(() => window.__bench);
  const line = (label, s) => `${label}  mean ${s.mean.toFixed(2)} ms  p95 ${s.p95.toFixed(2)} ms  max ${s.max.toFixed(2)} ms`;
  console.log(`${result.shapes} shapes, ${result.drawnPerFrame} drawn per frame, ${result.viewport.w}×${result.viewport.h} @${result.dpr}x, ${result.frames} frames`);
  console.log(line("pan ", result.panMs));
  console.log(line("drag", result.dragMs));
  console.log(`frame rate at p95: ${result.fpsAtP95.toFixed(1)} fps`);

  const summary = { profile, shapes: result.shapes, drawnPerFrame: result.drawnPerFrame, panP95: result.panMs.p95, dragP95: result.dragMs.p95, fpsAtP95: result.fpsAtP95, recorded: new Date().toISOString().slice(0, 10) };
  if (record || !existsSync(baselinePath)) {
    writeFileSync(baselinePath, JSON.stringify(summary, null, 2) + "\n");
    console.log(`baseline ${record ? "written" : "created"}: ${baselinePath}`);
  } else {
    const baseline = JSON.parse(readFileSync(baselinePath, "utf8"));
    const worse = (now, then) => now > then * (1 + tolerance);
    const regressions = [];
    if (worse(summary.panP95, baseline.panP95)) regressions.push(`pan p95 ${summary.panP95.toFixed(2)} ms against ${baseline.panP95.toFixed(2)} ms`);
    if (worse(summary.dragP95, baseline.dragP95)) regressions.push(`drag p95 ${summary.dragP95.toFixed(2)} ms against ${baseline.dragP95.toFixed(2)} ms`);
    if (regressions.length) {
      console.error(`REGRESSION against ${baselinePath} (${baseline.recorded}): ${regressions.join("; ")}`);
      process.exitCode = 1;
    } else {
      console.log(`within ${tolerance * 100}% of the ${profile} baseline of ${baseline.recorded}`);
    }
  }
} finally {
  await browser.close();
  await server.close();
}
