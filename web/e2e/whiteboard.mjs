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

let page;
try {
  page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
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
      // Core answers a message when its turn is over, however it ended: with
      // the assistant's reply, or stopped at Core's cap on tool calls, which a
      // small model can reach. So the user's edit below never interleaves
      // with the agent's commits.
      const answer = await request({ send_message: { text: "Add a yellow sticky that says \"Plan: design, build, launch\"." } });
      if (answer && typeof answer === "object" && "error" in answer) step(`the turn ended with an error: ${JSON.stringify(answer.error).slice(0, 200)}`);
      const settled = await doc();
      agentAdded = settled.shapes.length - shapesBefore;
      await until("the frame to show the document as it is", async () => (await state()).shapes === settled.shapes.length + settled.frames.length, 15000);
      if (agentAdded > 0) step(`the agent added ${agentAdded} shape(s); the frame shows them`);
      else if (agentAdded < 0) step(`the agent removed ${-agentAdded} shape(s) instead (a small model); the frame shows the document as it is`);
      else step(`the agent added nothing this run (a small model); the frame shows the document as it is`);
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
  // The new note is the selection; every check below is about this one
  // note by id, because the document persists between runs and an earlier
  // run's note may still carry the same text.
  const noteId = await frame.evaluate(() => [...window.__localspace.editor.selection][0] ?? null);
  if (!noteId) throw new Error("the click did not create a note");
  const NOTE = "edited live on the own canvas";
  await page.keyboard.type(NOTE);
  await page.keyboard.press("Escape");
  // The note's text in Core: null when the note is not in the document.
  const noteInCore = async () => (await doc()).shapes.find((s) => s.id === noteId)?.text ?? null;
  const noteInFrame = () => frame.evaluate((id) => window.__localspace.editor.scene.get(id)?.text ?? null, noteId);
  const written = await until(
    "Core to hold the note",
    async () => {
      const d = await doc();
      return d.shapes.find((s) => s.id === noteId)?.text === NOTE ? d : null;
    },
    20000,
  );
  const commits = await history();
  const head = commits[0];
  step(`Core has the note (${written.shapes.length} shapes); head commit: ${head.tool} by ${head.author}: ${head.diff_summary}`);
  if (head.tool !== "surface:sync" || head.author !== "user") throw new Error("the edit did not land as the user's commit from the replica");
  if (head.id === headBefore) throw new Error("no commit was made");
  // Typing is one commit however many keys it takes: the note is its
  // creation and its text, nothing per keystroke.
  const made = commits.findIndex((c) => c.id === headBefore);
  if (made !== 2) throw new Error(`the note made ${made < 0 ? "more than fifty" : made} commit(s); expected two, its creation and its text`);
  step(`the note is two commits: its creation, and its ${NOTE.length} characters of text in one`);

  // 5. Undo through the DAG, from inside the frame: the note was two commits,
  // its creation and its text, so the first Ctrl+Z takes the text and the
  // second the note; then redo both.
  const total = written.shapes.length + written.frames.length;
  await host.click({ position: { x: box.width * 0.15, y: box.height * 0.15 } });
  await page.keyboard.press("Control+z");
  await until("the undo to take the text away", async () => (await noteInCore()) === "", 20000);
  await until("the frame to follow the undo", async () => (await state()).shapes === total && (await noteInFrame()) === "", 20000);
  await page.keyboard.press("Control+z");
  await until("the second undo to take the note away", async () => (await noteInCore()) === null, 20000);
  await until("the frame to follow the second undo", async () => (await state()).shapes === total - 1 && (await noteInFrame()) === null, 20000);
  step("Ctrl+Z inside the frame undid the text, then the note, through Core's history; the frame followed both");
  await page.keyboard.press("Control+Shift+z");
  await until("the redo to bring the note back", async () => (await noteInCore()) === "", 20000);
  await page.keyboard.press("Control+Shift+z");
  await until("the redo to bring the text back", async () => (await noteInCore()) === NOTE, 20000);
  await until("the frame to follow the redo", async () => (await noteInFrame()) === NOTE, 20000);
  step("Ctrl+Shift+Z twice redid the note and its text; the frame followed");

  // 6. The document's selection is the agent's: canvas.select, here run by
  // hand through the API, moves the frame's selection.
  await request({ call_tool: { tool: "canvas.select", params: { ids: [noteId] } } });
  await until("the frame to select the note", async () => (await frame.evaluate(() => [...window.__localspace.editor.selection])).join() === noteId, 15000);
  step("canvas.select through the API selected the note in the frame");

  // 7. Snapping, on an empty stretch of the board: a note dropped with its
  // left edge a few pixels from another's comes to rest on it, with a guide
  // drawn while it is held; with Alt held it goes exactly where it is put.
  // Both notes are deleted afterwards.
  const far = 1_000_000 + Math.round(Math.random() * 1_000_000);
  await frame.evaluate((x) => window.__localspace.editor.setCamera({ x, y: x, z: 1 }), far);
  const hostBox = await host.boundingBox();
  const makeNote = async (sx, sy) => {
    await frame.getByRole("button", { name: /Sticky note/ }).click();
    await page.mouse.click(hostBox.x + sx, hostBox.y + sy);
    await sleep(200);
    await page.keyboard.press("Escape");
    return frame.evaluate(() => [...window.__localspace.editor.selection][0]);
  };
  const nodeOf = (id) =>
    frame.evaluate((i) => {
      const n = window.__localspace.editor.scene.get(i);
      return n ? { x: n.x, y: n.y, w: n.w, h: n.h } : null;
    }, id);
  const coreX = async (id) => (await doc()).shapes.find((s) => s.id === id)?.x;
  const noteA = await makeNote(300, 150);
  const noteB = await makeNote(420, 420);
  const a = await nodeOf(noteA);
  let b = await nodeOf(noteB);
  // The editor keeps positions to a hundredth of a unit.
  const near = (v, want) => typeof v === "number" && Math.abs(v - want) <= 0.006;
  const grab = async (n) => {
    await page.mouse.move(hostBox.x + n.x + n.w / 2 - far, hostBox.y + n.y + n.h / 2 - far);
    await page.mouse.down();
  };
  await grab(b);
  await page.mouse.move(hostBox.x + a.x + 5 + b.w / 2 - far, hostBox.y + b.y + b.h / 2 - far, { steps: 12 });
  await sleep(150);
  const guide = await frame.evaluate(
    ([x, y]) => {
      const c = document.querySelector(".board-host canvas");
      const d = c.getContext("2d").getImageData(x * devicePixelRatio, y * devicePixelRatio, 1, 1).data;
      return [d[0], d[1], d[2]];
    },
    [Math.round(a.x - far), Math.round((a.y + a.h + b.y) / 2 - far)],
  );
  await page.mouse.up();
  if (!(guide[0] > 180 && guide[1] < 110)) throw new Error(`no guide drawn at the other note's left edge while the drag was held: rgb(${guide.join(", ")})`);
  await until("the dropped note to rest on the other's left edge", async () => near(await coreX(noteB), a.x), 20000);
  b = await nodeOf(noteB);
  await grab(b);
  await page.keyboard.down("Alt");
  await page.mouse.move(hostBox.x + b.x + 5 + b.w / 2 - far, hostBox.y + b.y + b.h / 2 - far, { steps: 6 });
  await page.mouse.up();
  await page.keyboard.up("Alt");
  await until("the Alt-dropped note to stay where it was put", async () => near(await coreX(noteB), a.x + 5), 20000);
  await frame.evaluate(([x, y]) => {
    const e = window.__localspace.editor;
    e.select([x, y]);
    e.deleteSelection();
  }, [noteA, noteB]);
  await until("the two notes to go", async () => !(await doc()).shapes.some((s) => s.id === noteA || s.id === noteB), 20000);
  step("a note dropped 5 px off another's left edge came to rest on it, with a guide drawn; with Alt held it stayed 5 px off");

  // 8. Zoom from the shell's top bar reaches the frame, and the frame reports back.
  const z0 = (await state()).zoom;
  await page.getByRole("button", { name: "Zoom in" }).click();
  await until("the frame to zoom", async () => Math.abs((await state()).zoom - z0) > 0.01);
  const label = await page.locator("button", { hasText: /^\d+%$/ }).first().innerText();
  step(`zoom from the top bar: ${label} in the shell, ${((await state()).zoom * 100).toFixed(0)}% in the frame`);

  console.log(`PASS${agentAdded ? ` (agent added ${agentAdded})` : ""}`);
} catch (err) {
  // What the shell and Core say at the moment of a failure, so a run that
  // fails explains itself.
  if (page) {
    const toasts = await page.locator(".ls-toast, [role=alert], [role=status]").allInnerTexts().catch(() => []);
    console.log(`  shell toasts: ${JSON.stringify(toasts)}`);
  }
  for (const c of (await history().catch(() => [])).slice(0, 8)) console.log(`  commit ${c.id} <- ${c.parent} ${c.tool} ${c.author} ${JSON.stringify(c.diff_summary)}`);
  const d = await doc().catch(() => null);
  if (d) console.log(`  Core: ${d.shapes.length} shapes; with the note text: ${d.shapes.filter((s) => (s.text ?? "").includes("edited live on the own canvas")).map((s) => s.id).join(", ") || "none"}`);
  throw err;
} finally {
  await browser.close();
}
