// Maths in a chat answer, in a real browser (docs/DECISIONS.md, 2026-10-18):
// the test engine answers with a display formula and an inline one, and the
// answer must show both rendered by KaTeX, with none of the dollar marks
// left, and KaTeX's fonts fetched as woff2 only. Runs against a
// `localspace serve` that is already up, with `LOCALSPACE_LLAMA_SERVER` set
// to the test engine and a model whose stub says `writes maths`; needs Edge
// or Chrome on the machine, nothing downloaded.
//
//   node e2e/maths.mjs http://127.0.0.1:8447 ci-token maths

import { chromium } from "playwright-core";

const [origin = "http://127.0.0.1:8447", token = "ci-token", model = "maths"] = process.argv.slice(2);
const headers = { Authorization: `Bearer ${token}`, "content-type": "application/json" };
const api = async (path) => (await fetch(`${origin}/api/v1${path}`, { headers })).json();
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
  step(`loading ${model}, the test engine that writes maths`);
  await request({ load_model: { id: model } });
  await until("the engine", async () => (await api("/environment")).environment.engine.running, 60000);

  const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  const fonts = [];
  page.on("request", (r) => {
    if (r.resourceType() === "font") fonts.push(new URL(r.url()).pathname);
  });
  page.on("pageerror", (e) => console.log(`  [pageerror] ${e.message.slice(0, 200)}`));
  await page.goto(`${origin}/?token=${token}`);

  // With a model here the shell opens directly; the first run is passed if it shows.
  const notNow = page.getByRole("button", { name: "Not now", exact: true });
  const chat = page.getByRole("button", { name: "Chat", exact: true });
  await until("the shell or the first run", async () => (await chat.count()) + (await notNow.count()) > 0, 60000);
  if (await notNow.count()) await notNow.click();
  await chat.first().click();

  const box = page.getByRole("textbox", { name: "Message" });
  await until("the composer to take a message", async () => (await box.count()) > 0 && (await box.isEnabled()), 15000);
  await box.fill("Show me a formula.");
  await box.press("Enter");
  step("asked for a formula");

  const answer = page.locator(".prose-chat").last();
  await until("both formulas rendered", async () => (await answer.locator(".katex").count()) >= 2, 30000);
  const displayed = await answer.locator(".maths-display .katex-display").count();
  if (displayed !== 1) throw new Error(`expected one display formula, found ${displayed}`);
  const inline = await answer.locator(".maths:not(.maths-display) .katex").count();
  if (inline !== 1) throw new Error(`expected one inline formula, found ${inline}`);
  const source = await answer.locator(".maths-source").count();
  if (source) throw new Error(`${source} formula(s) still shown as their source`);
  const shown = await answer.innerText();
  if (shown.includes("$")) throw new Error(`a dollar mark is left in the answer: ${JSON.stringify(shown.slice(0, 200))}`);
  if (!/area under the curve/.test(shown) || !/right triangle/.test(shown)) throw new Error(`the words around the formulas are missing: ${JSON.stringify(shown.slice(0, 200))}`);
  step(`rendered: one display formula, one inline; the words around them kept`);

  await until("KaTeX's fonts", async () => fonts.length > 0, 10000);
  const other = fonts.filter((f) => !f.endsWith(".woff2"));
  if (other.length) throw new Error(`fonts other than woff2 were fetched: ${other.join(", ")}`);
  step(`fonts: ${fonts.length} fetched, all woff2`);
  console.log("maths walk: ok");
} finally {
  await browser.close();
}
