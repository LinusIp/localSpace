// The looks behind the `themes` switch, in a real browser
// (docs/design/0.2.1-THEMES-AND-TRANSLATION.md §2–3): the main screens with
// the switch off, and in each palette in light and dark. With the switch off
// the page carries no palette and keeps Figtree; with it on, <html> carries
// the palette and the mode, the page takes the palette's colours, and the
// text is set in Onest, Uzbek's ʻ included. Saves a screenshot of every
// screen in every look to <out>. Runs against a `localspace serve` that is
// already up, with `LOCALSPACE_LLAMA_SERVER` set to the test engine and a
// model that answers.
//
//   node e2e/looks.mjs http://127.0.0.1:8447 ci-token maths <out>

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "playwright-core";

const [origin = "http://127.0.0.1:8447", token = "ci-token", model = "maths", out = "looks"] = process.argv.slice(2);
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

/** Each palette's page colour in light and dark, as ui.css sets it. */
const PAGE = {
  indigo: { light: "rgb(248, 248, 251)", dark: "rgb(17, 18, 28)" },
  violet: { light: "rgb(250, 248, 251)", dark: "rgb(21, 17, 24)" },
  teal: { light: "rgb(246, 249, 249)", dark: "rgb(14, 21, 23)" },
};
const LOOKS = [
  { name: "off" },
  ...Object.keys(PAGE).flatMap((palette) => ["light", "dark"].map((mode) => ({ name: `${palette}-${mode}`, palette, mode }))),
];
const CHAT = "Looks walk: Oʻzbekcha — oʻqish, gʻoya; Русский — привет; English — hello";

mkdirSync(out, { recursive: true });

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

  /** A fresh window in one look, at the chat: Core's switch and the person's palette and mode, set through the API. */
  const open = async (look) => {
    const switched = await request({ set_feature: { feature: "themes", on: Boolean(look.palette) } });
    if (!switched.environment?.features) throw new Error(`the themes switch was not turned: ${JSON.stringify(switched).slice(0, 200)}`);
    if (look.palette) {
      await request({ set_preference: { key: "theme", value: look.palette } });
      await request({ set_preference: { key: "mode", value: look.mode } });
    }
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const p = await context.newPage();
    p.on("pageerror", (e) => console.log(`  [pageerror] ${e.message.slice(0, 200)}`));
    await p.goto(`${origin}/?token=${token}`);
    const box = p.getByRole("textbox", { name: "Message" });
    const notNow = p.getByRole("button", { name: "Not now", exact: true });
    await until("the chat or the first run", async () => (await box.count()) + (await notNow.count()) > 0, 60000);
    if (await notNow.count()) await notNow.click();
    await until("the message box", async () => (await box.count()) === 1, 15000);
    return p;
  };
  const shoot = async (look, screen) => {
    await page.evaluate(() => document.fonts.ready);
    await sleep(250);
    await page.screenshot({ path: join(out, `${look.name}-${screen}.png`) });
  };

  // One chat with an answer, for every look to show.
  page = await open(LOOKS[0]);
  await page.locator(".rail-new").click();
  await until("an empty chat", async () => (await page.locator(".message").count()) === 0, 15000);
  const box = page.getByRole("textbox", { name: "Message" });
  await box.fill(CHAT);
  await box.press("Enter");
  await until("the answer", async () => (await page.locator(".message").count()) === 2 && (await page.locator(".send.stop").count()) === 0, 30000);
  await until("the chat in the rail", async () => (await page.locator(".rail-chat", { hasText: "Looks walk" }).count()) > 0, 15000);
  await page.context().close();
  step("a chat with an answer, to show in every look");

  for (const look of LOOKS) {
    page = await open(look);
    // The palette comes with the person's preferences, a moment after the page.
    await until(
      `the page to carry ${look.palette ?? "no palette"}`,
      async () => (await page.evaluate(() => document.documentElement.dataset.palette ?? null)) === (look.palette ?? null),
      10000,
    );
    const seen = await page.evaluate(() => ({
      palette: document.documentElement.dataset.palette ?? null,
      theme: document.documentElement.dataset.theme ?? null,
      background: getComputedStyle(document.body).backgroundColor,
      font: getComputedStyle(document.body).fontFamily,
    }));
    if (!look.palette) {
      if (seen.palette || seen.theme) throw new Error(`with themes off the page carries a look: ${JSON.stringify(seen)}`);
      if (!/^"?Figtree/.test(seen.font)) throw new Error(`with themes off the font is ${seen.font}`);
    } else {
      if (seen.palette !== look.palette || seen.theme !== look.mode) throw new Error(`${look.name}: the page carries ${JSON.stringify(seen)}`);
      if (seen.background !== PAGE[look.palette][look.mode]) throw new Error(`${look.name}: the page is ${seen.background}, not ${PAGE[look.palette][look.mode]}`);
      if (!/^"?Onest/.test(seen.font)) throw new Error(`${look.name}: the font is ${seen.font}`);
      const faces = await page.evaluate(async () => (await document.fonts.load('16px "Onest"', "oʻ")).map((f) => f.unicodeRange));
      if (!faces.some((range) => /U\+0*2BB/i.test(range))) throw new Error(`${look.name}: no Onest face covering ʻ loaded (${JSON.stringify(faces)})`);
    }

    await page.locator(".rail-chat", { hasText: "Looks walk" }).first().click();
    await until("the chat", async () => (await page.locator(".message").count()) >= 2, 15000);
    let saved = 0;
    const save = async (screen) => {
      saved++;
      // A default install (this server names no `set_aside`, so the release's
      // applies) shows no whiteboard, planner, Documents or Store anywhere.
      const shown = await page.evaluate(() => document.body.innerText);
      const leak = /whiteboard|planner|\bboards?\b|\bStore\b/i.exec(shown);
      if (leak) throw new Error(`${look.name} ${screen}: a default install shows "${leak[0]}"`);
      for (const name of ["Whiteboard", "Documents", "Store"]) {
        if (await page.getByRole("button", { name, exact: true }).count()) throw new Error(`${look.name} ${screen}: a default install offers ${name}`);
      }
      await shoot(look, `${String(saved).padStart(2, "0")}-${screen}`);
    };
    await save("chat");
    // The dialog that lists every chat: the earlier walks leave more chats
    // than the rail shows.
    await page.locator(".rail-chat.more").click();
    await until("the dialog", async () => (await page.getByRole("dialog").count()) === 1, 5000);
    await save("all-chats-dialog");
    await page.keyboard.press("Escape");
    await until("the dialog to close", async () => (await page.getByRole("dialog").count()) === 0, 5000);
    await page.locator(".rail-new").click();
    await until("an empty chat", async () => (await page.locator(".message").count()) === 0, 15000);
    await save("new-chat");
    await page.getByRole("button", { name: "Settings", exact: true }).first().click();
    // Appearance is offered only while `themes` is on.
    const appearance = await page.locator(".subnav .sub", { hasText: "Appearance" }).count();
    if (appearance !== (look.palette ? 1 : 0)) throw new Error(`${look.name}: Settings offers Appearance ${appearance} times`);
    if (look.palette) {
      const chosen = page.locator(".opt.on", { hasText: { indigo: "Indigo and amber", violet: "Violet and magenta", teal: "Petrol teal" }[look.palette] });
      await page.locator(".subnav .sub", { hasText: "Appearance" }).click();
      await until("Appearance to show the palette in use", async () => (await chosen.count()) === 1, 10000);
    }
    for (const pane of ["General", ...(look.palette ? ["Appearance"] : []), "Assistant", "Network", "Tools", "About", "Advanced"]) {
      await page.locator(".subnav .sub", { hasText: pane }).click();
      await until(`Settings → ${pane}`, async () => (await page.locator(".pane-title", { hasText: pane }).count()) === 1, 15000);
      await save(`settings-${pane.toLowerCase()}`);
    }
    await page.getByRole("button", { name: "Help", exact: true }).click();
    await sleep(300);
    await save("help");
    // An old link to the whiteboard opens the chat and says nothing of boards.
    await page.goto(`${origin}/board/io.localspace.whiteboard`);
    await until("the chat after an old board link", async () => (await page.getByRole("textbox", { name: "Message" }).count()) === 1, 15000);
    await sleep(1500);
    if (await page.locator(".toast").count()) throw new Error(`${look.name}: an old board link shows a message: ${await page.locator(".toast").first().innerText()}`);
    await save("old-board-link");
    await page.context().close();
    step(`${look.name}: ${look.palette ? "the palette, the mode and Onest are on the page" : "no palette, Figtree"}; ${saved} screens saved`);
  }
  await request({ set_feature: { feature: "themes", on: false } });
  console.log(`looks walk: ok (${LOOKS.length} looks, in ${out})`);
} catch (error) {
  if (page) {
    const shownText = await page.evaluate(() => document.body.innerText).catch(() => "(the page could not be read)");
    console.log(`  the page at ${page.url()} showed: ${JSON.stringify(shownText.slice(0, 800))}`);
  }
  throw error;
} finally {
  await browser.close();
}
