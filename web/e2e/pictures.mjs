// A picture attached in the composer, in a real browser (docs/DECISIONS.md,
// 2026-10-18; plan 38, P3): attached through the file chooser and sent with
// a message; the engine is sent it as a picture with the words; the sent
// message shows it from its own address; and deleting the chat deletes it,
// on the server and on the disk. Runs against a `localspace serve` that is
// already up, with `LOCALSPACE_LLAMA_SERVER` set to the test engine and a
// model with a vision file; needs Edge or Chrome on the machine.
//
//   node e2e/pictures.mjs http://127.0.0.1:8448 ci-token seer <data dir> <picture>

import { chromium } from "playwright-core";
import fs from "node:fs";
import path from "node:path";

const [origin, token, model, data, picture] = process.argv.slice(2);
if (!origin || !token || !model || !data || !picture) throw new Error("usage: pictures.mjs <origin> <token> <model> <data dir> <picture>");
const headers = { Authorization: `Bearer ${token}`, "content-type": "application/json" };
const api = async (p) => (await fetch(`${origin}/api/v1${p}`, { headers })).json();
const request = async (body) => (await fetch(`${origin}/api/v1/request`, { method: "POST", headers, body: JSON.stringify(body) })).json();
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
const kept = () => {
  const root = path.join(data, "attachments");
  const out = [];
  const walk = (d) => {
    if (!fs.existsSync(d)) return;
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (/\.(png|jpg)$/.test(e.name)) out.push(p);
    }
  };
  walk(root);
  return out;
};

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
  step(`loading ${model}, a test engine with a picture file`);
  await request({ load_model: { id: model } });
  await until("the engine", async () => (await api("/environment")).environment.engine.running, 60000);

  page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  page.on("pageerror", (e) => console.log(`  [pageerror] ${e.message.slice(0, 200)}`));
  await page.goto(`${origin}/?token=${token}`);
  const notNow = page.getByRole("button", { name: "Not now", exact: true });
  const box = page.getByRole("textbox", { name: "Message" });
  await until("the chat or the first run", async () => (await box.count()) + (await notNow.count()) > 0, 60000);
  if (await notNow.count()) await notNow.click();
  await until("the composer to take a message", async () => (await box.count()) > 0 && (await box.isEnabled()), 15000);

  // 1. Attached through the file chooser: a thumbnail, taken by Core.
  await page.locator("input[type=file]").setInputFiles(picture);
  await until("the picture taken", async () => (await page.locator(".composer-picture:not(.uploading)").count()) === 1, 15000);
  if (kept().length !== 1) throw new Error(`expected one kept picture, found ${kept().length}`);
  step("attached: one picture in the composer, kept by Core until it is sent");

  // 2. Sent with words: the engine is sent the picture with them.
  await box.fill("What is in this picture?");
  await box.press("Enter");
  const shown = page.locator(".message-picture img");
  await until("the picture in the sent message", async () => (await shown.count()) === 1, 30000);
  const engineLog = fs.readFileSync(path.join(data, "engines", `${model}.log`), "utf8");
  if (!engineLog.includes("text, image_url")) throw new Error("the engine was not sent the picture with the words");
  const src = await shown.getAttribute("src");
  const served = await fetch(`${origin}${src}`, { headers });
  if (served.status !== 200 || !/^image\/(png|jpeg)$/.test(served.headers.get("content-type") ?? "")) {
    throw new Error(`the sent picture was not served: ${served.status} ${served.headers.get("content-type")}`);
  }
  if (served.headers.get("x-content-type-options") !== "nosniff") throw new Error("the picture is served without nosniff");
  step(`sent: the engine had it as a picture with the words; the message shows it from ${src}`);

  // 3. Deleted with the chat, on the server and on the disk.
  const chat = (await request("list_conversations")).conversations.current;
  await request({ delete_conversation: { id: chat } });
  const after = await fetch(`${origin}${src}`, { headers });
  if (after.status !== 404) throw new Error(`the picture is still served after its chat was deleted: ${after.status}`);
  if (kept().length !== 0) throw new Error(`pictures left on the disk after the chat was deleted: ${kept().join(", ")}`);
  step("deleted with its chat: no longer served, and gone from the disk");
  console.log("pictures walk: ok");
} catch (error) {
  if (page) {
    const shownText = await page.evaluate(() => document.body.innerText).catch(() => "(the page could not be read)");
    console.log(`  the page at ${page.url()} showed: ${JSON.stringify(shownText.slice(0, 600))}`);
  }
  throw error;
} finally {
  await browser.close();
}
