// The whiteboard gate on the v2.1 stack, driven end to end in a real browser
// (architecture §13 step 5): install from the catalog, open the board, the
// agent puts a plan on it, the user edits it live inside the harness's
// frame, undo through the DAG from inside the frame. Runs against a
// `localspace serve` that is already up; needs Edge or Chrome on the
// machine, nothing downloaded.
//
//   node e2e/whiteboard.mjs http://127.0.0.1:8443 devtoken-1 [--no-agent]

import { chromium } from "playwright-core";

const args = process.argv.slice(2);
const positional = args.filter((a) => !a.startsWith("--"));
const [origin = "http://127.0.0.1:8443", token = "devtoken-1"] = positional;
const withAgent = !args.includes("--no-agent");
const headers = { Authorization: `Bearer ${token}`, "content-type": "application/json" };
const api = async (path) => (await fetch(`${origin}/api/v1${path}`, { headers })).json();
const request = async (body) => (await fetch(`${origin}/api/v1/request`, { method: "POST", headers, body: JSON.stringify(body) })).json();
const history = async () => (await api("/history?limit=50")).history.commits;
const doc = async () => (await api("/docs/io.localspace.whiteboard")).doc_json.json;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const until = async (what, check, ms = 15000) => {
  const start = Date.now();
  for (;;) {
    const value = await check();
    if (value) return value;
    if (Date.now() - start > ms) throw new Error(`timed out waiting for ${what}`);
    await sleep(250);
  }
};
const step = (text) => console.log(`· ${text}`);

let browser;
for (const channel of ["msedge", "chrome"]) {
  try {
    browser = await chromium.launch({ channel, headless: true });
    step(`browser: ${channel}`);
    break;
  } catch {
    // try the next one
  }
}
if (!browser) throw new Error("neither Edge nor Chrome could be launched");

try {
  const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
  page.on("console", (m) => {
    if (m.type() === "error" && !/ws\/json/.test(m.text())) console.log(`  [console] ${m.text().slice(0, 200)}`);
  });
  page.on("pageerror", (e) => console.log(`  [pageerror] ${e.message.slice(0, 200)}`));
  await page.goto(`${origin}/?token=${token}`);
  await page.getByRole("button", { name: "Library", exact: true }).click();

  // 1. Install from the catalog, unless it is installed already.
  const installed = (await api("/environment")).environment.harnesses.some((h) => h.id === "io.localspace.whiteboard");
  if (!installed) {
    step("installing the whiteboard from the catalog");
    const row = page.locator("li", { hasText: "io.localspace.whiteboard" }).first();
    await row.getByRole("button", { name: /Install/ }).click();
    await until("the install", async () => (await api("/environment")).environment.harnesses.some((h) => h.id === "io.localspace.whiteboard"));
  } else {
    step("the whiteboard is already installed");
  }

  // 2. Open the board as a panel beside the chat.
  await page.getByRole("button", { name: "Tools", exact: true }).click();
  await page.getByTitle(/Open the web view "web"/).click();
  const frameEl = page.locator("iframe[title='Board']");
  await frameEl.waitFor({ state: "visible", timeout: 15000 });
  const src = await frameEl.getAttribute("src");
  if (!/^http:\/\/h-io-localspace-whiteboard\.localhost:\d+\/s\/[0-9a-f]+\/$/.test(src)) throw new Error(`unexpected frame url ${src}`);
  step(`the frame is on its own origin: ${new URL(src).origin}`);
  const frame = await (await frameEl.elementHandle()).contentFrame();
  const host = frame.locator(".board-host");
  await host.waitFor({ state: "visible", timeout: 30000 });
  await frame.waitForFunction(() => Boolean(window.__localspace), null, { timeout: 30000 });
  const state = () => frame.evaluate(() => ({ shapes: window.__localspace.editor.scene.size, replica: window.__localspace.replica?.size ?? -1, zoom: window.__localspace.editor.camera.z }));
  const before = await doc();
  const s0 = await state();
  step(`the board shows ${s0.shapes} shape(s); the replica holds ${s0.replica}; Core's document has ${before.shapes.length + before.frames.length}`);
  if (s0.shapes !== before.shapes.length + before.frames.length) throw new Error("the board does not show what the document holds");

  // 3. The agent puts a plan on the board, when a model is there to run it.
  let agentAdded = 0;
  if (withAgent) {
    const env = (await api("/environment")).environment;
    if (!env.model) {
      const catalog = (await request("list_model_catalog")).model_catalog.entries;
      const local = catalog.find((m) => m.installed && m.verdict !== "does not fit");
      if (local) {
        step(`loading ${local.id} for the agent`);
        await request({ load_model: { id: local.id } });
        await until("the engine", async () => (await api("/environment")).environment.engine.running, 180000);
      }
    }
    if ((await api("/environment")).environment.model) {
      const shapesBefore = (await doc()).shapes.length;
      await request("new_conversation");
      step("asking the agent for a plan on the board, in a new conversation");
      await request({ send_message: { text: "Add a yellow sticky that says \"Plan: design, build, launch\"." } });
      const after = await until("the agent's notes", async () => {
        const d = await doc();
        return d.shapes.length > shapesBefore ? d : null;
      }, 20000).catch(() => null);
      if (after) {
        agentAdded = after.shapes.length - shapesBefore;
        await until("the frame to show them", async () => (await state()).shapes >= after.shapes.length + after.frames.length, 15000);
        step(`the agent added ${agentAdded} shape(s); the frame shows them`);
      } else {
        step("the agent added nothing this run (a small model); the rest of the gate goes on");
      }
    } else {
      step("no model to run the agent with; the rest of the gate goes on");
    }
  }

  // 4. Edit live: a sticky note, typed into, from inside the frame.
  const headBefore = (await history())[0]?.id;
  const box = await host.boundingBox();
  await frame.getByRole("button", { name: /Sticky note/ }).click();
  await host.click({ position: { x: box.width * 0.55, y: box.height * 0.7 } });
  await sleep(300);
  await page.keyboard.type("edited live on the own canvas");
  await page.keyboard.press("Escape");
  const written = await until(
    "Core to hold the note",
    async () => {
      const d = await doc();
      return d.shapes.find((s) => s.kind === "sticky" && (s.text ?? "").includes("edited live on the own canvas")) ? d : null;
    },
    20000,
  );
  const commits = await history();
  const head = commits[0];
  step(`Core has the note (${written.shapes.length} shapes); head commit: ${head.tool} by ${head.author}: ${head.diff_summary}`);
  if (head.tool !== "surface:sync" || head.author !== "user") throw new Error("the edit did not land as the user's commit from the replica");
  if (head.id === headBefore) throw new Error("no commit was made");

  // 5. Undo through the DAG, from inside the frame: the note was two commits,
  // its creation and its text, so the first Ctrl+Z takes the text and the
  // second the note; then redo both.
  const noteInCore = async () => !!(await doc()).shapes.find((s) => (s.text ?? "").includes("edited live on the own canvas"));
  const noteInFrame = () => frame.evaluate(() => window.__localspace.editor.scene.all().some((n) => n.text.includes("edited live on the own canvas")));
  const total = written.shapes.length + written.frames.length;
  await host.click({ position: { x: box.width * 0.15, y: box.height * 0.15 } });
  await page.keyboard.press("Control+z");
  await until("the undo to take the text away", async () => !(await noteInCore()), 20000);
  await until("the frame to follow the undo", async () => (await state()).shapes === total && !(await noteInFrame()), 20000);
  await page.keyboard.press("Control+z");
  await until("the second undo to take the note away", async () => (await doc()).shapes.length === written.shapes.length - 1, 20000);
  await until("the frame to follow the second undo", async () => (await state()).shapes === total - 1, 20000);
  step("Ctrl+Z inside the frame undid the text, then the note, through Core's history; the frame followed both");
  await page.keyboard.press("Control+Shift+z");
  await until("the redo to bring the note back", async () => (await doc()).shapes.length === written.shapes.length, 20000);
  await page.keyboard.press("Control+Shift+z");
  await until("the redo to bring the text back", noteInCore, 20000);
  await until("the frame to follow the redo", noteInFrame, 20000);
  step("Ctrl+Shift+Z twice redid the note and its text; the frame followed");

  // 6. Zoom from the shell's top bar reaches the frame, and the frame reports back.
  const z0 = (await state()).zoom;
  await page.getByRole("button", { name: "Zoom in" }).click();
  await until("the frame to zoom", async () => Math.abs((await state()).zoom - z0) > 0.01);
  const label = await page.locator("button", { hasText: /^\d+%$/ }).first().innerText();
  step(`zoom from the top bar: ${label} in the shell, ${((await state()).zoom * 100).toFixed(0)}% in the frame`);

  console.log(`PASS${agentAdded ? ` (agent added ${agentAdded})` : ""}`);
} finally {
  await browser.close();
}
