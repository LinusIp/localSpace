// How an answer that ends early is said, in a real browser (docs/DECISIONS.md,
// 2026-10-26): a chat that runs out of room says so in the person's language
// and offers a new chat, never Continue; a request the engine refuses for want
// of room says the same, never the raw error; an answer at its own length
// limit says that, and Continue carries it on. The words come from the
// client, so the same chat reads in Uzbek, Russian and English. Runs against
// a `localspace serve` that is already up, with `LOCALSPACE_LLAMA_SERVER`
// set to the test engine and two models: `full` ("fills the context;
// refuses requests over 6000 bytes") and `limit` ("stops at the limit").
//
//   node e2e/chat-full.mjs http://127.0.0.1:8449 ci-token

import { chromium } from "playwright-core";

const [origin, token] = process.argv.slice(2);
if (!origin || !token) throw new Error("usage: chat-full.mjs <origin> <token>");
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

// The lines as the client says them (web/src/lib/strings.ts).
const SAYS = {
  uz: { chatFull: "Bu suhbat toʻldi, shuning uchun javob chala qoldi.", noRoom: "modelga javob berish uchun joy qolmadi", newChat: "Yangi suhbat" },
  ru: { chatFull: "Этот чат заполнен, поэтому ответ оборван.", noRoom: "модели не хватило места для ответа" },
  en: { chatFull: "This chat is full, so the answer was cut short.", noRoom: "the model had no room to answer", atLimit: "The answer reached its length limit.", continue: "Continue" },
};
const RAW = /\b400\b|context size|could not reach|exceed/i;

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

const load = async (id) => {
  await request({ load_model: { id } });
  await until(`the engine for ${id}`, async () => {
    const e = (await api("/environment")).environment;
    return e.engine.running && e.model?.id === id;
  }, 60000);
};

let page;
try {
  step("loading `full`, a test engine whose context runs out");
  await load("full");
  await request({ set_preference: { key: "language", value: "uz" } });

  page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  page.on("pageerror", (e) => console.log(`  [pageerror] ${e.message.slice(0, 200)}`));
  await page.goto(`${origin}/?token=${token}`);
  const box = page.getByRole("textbox", { name: "Message" });
  const notNow = page.getByRole("button", { name: "Not now", exact: true });
  await until("the chat or the first run", async () => (await box.count()) + (await notNow.count()) > 0, 60000);
  if (await notNow.count()) await notNow.click();
  await until("the composer to take a message", async () => (await box.count()) > 0 && (await box.isEnabled()), 15000);
  const lines = page.locator(".message-stopped");
  const errors = page.locator(".toast.error");

  // 1. The context runs out while the answer is written: the words that came
  //    stay, and the chat says it is full, in Uzbek, offering a new chat.
  await box.fill("Uzun javob yozing.");
  await box.press("Enter");
  await until("the full-chat line", async () => (await lines.count()) === 1 && (await lines.first().innerText()).includes(SAYS.uz.chatFull), 30000);
  const first = await lines.first().innerText();
  if (!first.includes(SAYS.uz.newChat)) throw new Error(`no new-chat button on a full chat: ${first}`);
  if (/Continue|Davom ettirish/.test(first)) throw new Error(`a full chat offers Continue: ${first}`);
  if ((await page.locator(".prose-chat").last().innerText()).trim() === "") throw new Error("the words that came were not kept");
  step(`ran out of room: "${first}"`);

  // 2. A request too long for the engine at all: refused for want of room,
  //    said as such, never the raw refusal, never an error notice.
  await until("the answer before the next message", async () => (await box.isEnabled()) && (await page.locator(".send.stop").count()) === 0, 30000);
  await box.fill("Iltimos, diqqat bilan oʻqing. ".repeat(260));
  await box.press("Enter");
  await until("the no-room line", async () => (await lines.count()) === 2 && (await lines.nth(1).innerText()).includes(SAYS.uz.noRoom), 30000);
  const body = await page.evaluate(() => document.body.innerText);
  if (RAW.test(body)) throw new Error(`the raw refusal reached the page: ${body.match(RAW)?.[0]}`);
  if ((await errors.count()) !== 0) throw new Error("an error notice was shown");
  step("refused for want of room: said as a full chat, no raw error, no error notice");

  // 3. The same chat in Russian and English: the words are the client's.
  for (const language of ["ru", "en"]) {
    await request({ set_preference: { key: "language", value: language } });
    await page.reload();
    await until(`the lines in ${language}`, async () => {
      const said = (await lines.allInnerTexts()).join(" | ");
      return said.includes(SAYS[language].chatFull) && said.includes(SAYS[language].noRoom);
    }, 30000);
  }
  step("the same lines in Russian and English after a change of language");

  // 4. The new-chat button opens an empty chat.
  await lines.first().getByRole("button", { name: "New chat", exact: true }).click();
  await until("an empty chat", async () => (await page.locator(".message").count()) === 0, 15000);
  step("New chat opens an empty chat");

  // 5. An answer at its own length limit: that is what it says, never a full
  //    chat, and Continue carries it on.
  step("loading `limit`, a test engine that writes to its own limit");
  await load("limit");
  await page.reload();
  await until("the composer", async () => (await box.count()) > 0 && (await box.isEnabled()), 30000);
  await box.fill("Write a long answer.");
  await box.press("Enter");
  await until("the limit line", async () => (await lines.count()) === 1 && (await lines.first().innerText()).includes(SAYS.en.atLimit), 30000);
  const limit = await lines.first().innerText();
  if (/full/i.test(limit)) throw new Error(`an answer at its own limit says the chat is full: ${limit}`);
  await lines.first().getByRole("button", { name: SAYS.en.continue, exact: true }).click();
  await until("the answer carried on", async () => (await page.locator(".prose-chat").last().innerText()).split("The first part").length > 2, 30000);
  step("at its own limit: said so, and Continue carried it on");
  console.log("chat-full walk: ok");
} catch (error) {
  if (page) {
    const shownText = await page.evaluate(() => document.body.innerText).catch(() => "(the page could not be read)");
    console.log(`  the page at ${page.url()} showed: ${JSON.stringify(shownText.slice(0, 800))}`);
  }
  throw error;
} finally {
  await browser.close();
}
