// The message box keeps the cursor, in a real browser (docs/DECISIONS.md,
// 2026-10-27): when the app opens, after a message goes, on a new chat, when
// switching chats, after leaving Settings and after closing a dialog; never
// while a dialog is open. Also the wording fixes of the top bar and the
// composer: the chips are buttons into Settings, and the composer's buttons
// say what they do. Runs against a `localspace serve` that is already up,
// with `LOCALSPACE_LLAMA_SERVER` set to the test engine and a model that
// answers.
//
//   node e2e/cursor.mjs http://127.0.0.1:8447 ci-token maths

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
    await sleep(200);
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
  step(`loading ${model}`);
  await request({ load_model: { id: model } });
  await until("the engine", async () => (await api("/environment")).environment.engine.running, 60000);
  await request({ set_preference: { key: "language", value: "en" } });

  page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  page.on("pageerror", (e) => console.log(`  [pageerror] ${e.message.slice(0, 200)}`));
  await page.goto(`${origin}/?token=${token}`);
  const box = page.getByRole("textbox", { name: "Message" });
  const notNow = page.getByRole("button", { name: "Not now", exact: true });
  await until("the chat or the first run", async () => (await box.count()) + (await notNow.count()) > 0, 60000);
  if (await notNow.count()) await notNow.click();

  const inTheBox = () => page.evaluate(() => document.activeElement?.getAttribute("aria-label") === "Message");
  const hasCursor = (when) => until(`the cursor in the message box ${when}`, inTheBox, 5000);
  const answered = async () => (await box.isEnabled()) && (await page.locator(".send.stop").count()) === 0 && (await page.locator(".message").count()) % 2 === 0;
  const ask = async (text, how) => {
    await box.fill(text);
    if (how === "click") await page.getByRole("button", { name: "Send", exact: true }).click();
    else await box.press("Enter");
    await until(`the answer to "${text}"`, async () => (await page.locator(".message").count()) >= 2 && (await answered()), 30000);
  };

  // 1. When the app opens.
  await hasCursor("when the app opens");
  step("the app opens with the cursor in the message box");

  // 2. After a message goes, sent with the button.
  await ask("Cursor walk: the first chat", "click");
  await hasCursor("after sending with the button");
  step("after sending, the cursor is back in the message box");

  // 3. On a new chat.
  await page.locator(".rail-new").click();
  await until("an empty chat", async () => (await page.locator(".message").count()) === 0, 15000);
  await hasCursor("on a new chat");
  step("a new chat has the cursor");
  await ask("Cursor walk: the second chat", "enter");

  // 4. When switching chats.
  await page.locator(".rail-chat", { hasText: "Cursor walk: the first chat" }).click();
  await until("the first chat", async () => (await page.locator(".message-body.user", { hasText: "the first chat" }).count()) === 1, 15000);
  await hasCursor("after switching chats");
  step("switching chats keeps the cursor");

  // 5. After leaving Settings, through the top bar's network chip (now a
  //    button into Settings → Network).
  const chip = page.getByRole("button", { name: /Online for downloads only|Offline/ });
  await chip.click();
  await until("Settings", async () => (await box.count()) === 0, 15000);
  await page.locator(".rail-chat", { hasText: "Cursor walk: the second chat" }).click();
  await until("the chat again", async () => (await box.count()) === 1, 15000);
  await hasCursor("after leaving Settings");
  step("back from Settings, the cursor is in the message box");

  // 6. Never while a dialog is open, and back when it closes: "All chats"
  //    shows once there are more chats than the rail lists.
  const all = page.locator(".rail-chat.more");
  for (let n = 0; (await all.count()) === 0; n++) {
    if (n > 10) throw new Error("no All chats after ten more chats");
    await page.locator(".rail-new").click();
    await until("an empty chat", async () => (await page.locator(".message").count()) === 0, 15000);
    await ask(`Cursor walk: chat ${n + 3}`, "enter");
  }
  await all.click();
  await until("the dialog", async () => (await page.getByRole("dialog").count()) === 1, 5000);
  await sleep(300);
  if (await inTheBox()) throw new Error("the message box took the cursor while a dialog was open");
  await page.keyboard.press("Escape");
  await until("the dialog to close", async () => (await page.getByRole("dialog").count()) === 0, 5000);
  await hasCursor("after closing a dialog");
  step("a dialog keeps the cursor while open; closing it gives it back");

  // The composer's buttons say what they do, and the chip under the box.
  if ((await page.locator('.model-chip[title="Choose a model"]').count()) !== 1) throw new Error("the model chip under the box has no title");
  step("the composer's buttons are named: Send, Choose a model");
  console.log("cursor walk: ok");
} catch (error) {
  if (page) {
    const shownText = await page.evaluate(() => document.body.innerText).catch(() => "(the page could not be read)");
    console.log(`  the page at ${page.url()} showed: ${JSON.stringify(shownText.slice(0, 800))}`);
  }
  throw error;
} finally {
  await browser.close();
}
