// A text file attached in the composer, in a real browser (docs/DECISIONS.md,
// 2026-10-26; plan 38, F1 and F4), with a model that cannot read pictures:
// a file that is not text is refused, saying why; a Windows-1251 file is
// taken and sent, the engine is sent it as words, and it is saved again as
// UTF-8; a file too long for the chat says how much the model read; and
// deleting the chat deletes the files, on the server and on the disk. Runs
// against a `localspace serve` that is already up, with
// `LOCALSPACE_LLAMA_SERVER` set to the test engine; needs Edge or Chrome on
// the machine.
//
//   node e2e/text-files.mjs http://127.0.0.1:8447 ci-token maths <data dir>

import { chromium } from "playwright-core";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const [origin, token, model, data] = process.argv.slice(2);
if (!origin || !token || !model || !data) throw new Error("usage: text-files.mjs <origin> <token> <model> <data dir>");
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
      else if (e.name.endsWith(".txt")) out.push(p);
    }
  };
  walk(root);
  return out;
};

// Windows-1251 for the Russian letters, ё and №; everything else here is ASCII.
const cp1251 = (text) =>
  Buffer.from(
    [...text].map((c) => {
      const code = c.codePointAt(0);
      if (code < 0x80) return code;
      if (code >= 0x410 && code <= 0x44f) return code - 0x410 + 0xc0;
      if (c === "ё") return 0xb8;
      if (c === "№") return 0xb9;
      throw new Error(`not in this walk's Windows-1251: ${c}`);
    }),
  );
const words = "Счёт № 7\nИтого: 1250 сум за услуги связи.\n";
const files = fs.mkdtempSync(path.join(os.tmpdir(), "localspace-text-files-"));
const invoice = path.join(files, "счёт.txt");
fs.writeFileSync(invoice, cp1251(words));
const long = path.join(files, "long.md");
fs.writeFileSync(long, Array.from({ length: 30000 }, (_, i) => `Line ${i + 1} of a long report, kept for the record.`).join("\n"));
const binary = path.join(files, "data.txt");
fs.writeFileSync(binary, Buffer.from([0x50, 0x4b, 0x03, 0x04, 0x00, 0x00, 0x08, 0x00, 0xff, 0x00, 0x01, 0x02]));

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
  step(`loading ${model}, a test engine that cannot read pictures`);
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
  const chooser = page.locator("input[type=file]");
  const said = page.locator(".composer-said");

  // 1. A file named .txt that is not text: Core refuses it, and the person is told why.
  await chooser.setInputFiles(binary);
  await until("the refusal", async () => /Only pictures .* and text files/.test((await said.allInnerTexts()).join(" ")), 15000);
  if ((await page.locator(".composer-file").count()) !== 0 || kept().length !== 0) throw new Error("a file that is not text was kept");
  step("refused: a .txt that is not text, with the reason in the composer");

  // 2. A Windows-1251 file, attached through the file chooser and sent with
  //    words to a model that cannot read pictures: nothing warns of pictures,
  //    and the engine is sent the file as words, in UTF-8.
  await chooser.setInputFiles(invoice);
  await until("the file taken", async () => (await page.locator(".composer-file:not(.uploading)").count()) === 1, 15000);
  if (kept().length !== 1) throw new Error(`expected one kept file, found ${kept().length}`);
  if (/cannot read pictures|can't read pictures/i.test((await said.allInnerTexts()).join(" "))) throw new Error("a text file was taken for a picture");
  await box.fill("What is the total?");
  await box.press("Enter");
  const sent = page.locator(".message-file");
  await until("the file in the sent message", async () => (await sent.count()) === 1, 30000);
  const engineLog = () => fs.readFileSync(path.join(data, "engines", `${model}.log`), "utf8");
  await until("the engine to be sent the file", async () => engineLog().includes("fake llama-server: --- File: счёт.txt --- | Счёт № 7"), 15000);
  if ((await sent.locator(".message-file-note").count()) !== 0) throw new Error("a short file says it was read only in part");
  const href = await sent.locator("a").getAttribute("href");
  const served = await fetch(`${origin}${href}`, { headers });
  if (served.status !== 200 || served.headers.get("content-type") !== "text/plain; charset=utf-8") {
    throw new Error(`the sent file was not served as UTF-8 text: ${served.status} ${served.headers.get("content-type")}`);
  }
  if (served.headers.get("x-content-type-options") !== "nosniff") throw new Error("the file is served without nosniff");
  if (!/^attachment/.test(served.headers.get("content-disposition") ?? "")) throw new Error("the file is not served as a download");
  if ((await served.text()) !== words) throw new Error("the file saved again is not the words it was sent with");
  step("sent: the engine had the Windows-1251 file as words; saved again, it is the same words in UTF-8");

  // 3. A file too long for the chat: the model reads the start, and the
  //    message says how much (never cut silently).
  await until("the answer before the next message", async () => (await box.isEnabled()) && (await page.getByRole("button", { name: "Stop" }).count()) === 0, 30000);
  await chooser.setInputFiles(long);
  await until("the long file taken", async () => (await page.locator(".composer-file:not(.uploading)").count()) === 1, 15000);
  await box.fill("Summarise this report.");
  await box.press("Enter");
  await until("the long file in the sent message", async () => (await sent.count()) === 2, 30000);
  const note = await until("how much was read", async () => {
    const n = sent.nth(1).locator(".message-file-note");
    return (await n.count()) === 1 ? n.innerText() : null;
  });
  if (!/the model read the first \d+% \(about [\d,]+ of [\d,]+ words\)/.test(note)) throw new Error(`the part read is not said plainly: ${note}`);
  await until("the engine to be sent the start of it", async () => /--- File: long\.md \([^)]*\) --- \| Line 1 of a long report/.test(engineLog()), 15000);
  step(`read in part: "${note}"`);

  // 4. Deleted with the chat, on the server and on the disk.
  const chat = (await request("list_conversations")).conversations.current;
  await request({ delete_conversation: { id: chat } });
  const after = await fetch(`${origin}${href}`, { headers });
  if (after.status !== 404) throw new Error(`the file is still served after its chat was deleted: ${after.status}`);
  if (kept().length !== 0) throw new Error(`files left on the disk after the chat was deleted: ${kept().join(", ")}`);
  step("deleted with its chat: no longer served, and gone from the disk");
  console.log("text files walk: ok");
} catch (error) {
  if (page) {
    const shownText = await page.evaluate(() => document.body.innerText).catch(() => "(the page could not be read)");
    console.log(`  the page at ${page.url()} showed: ${JSON.stringify(shownText.slice(0, 600))}`);
  }
  throw error;
} finally {
  await browser.close();
  fs.rmSync(files, { recursive: true, force: true });
}
