// The looks behind the `themes` switch on an organisation's server
// (docs/design/0.2.1-THEMES-AND-TRANSLATION.md §2–4): the screens only a
// server shows, in the same seven looks as looks.mjs. Sign-in, the home page
// after it, the account menu, and Admin → People and Workspaces. Runs after
// org.mjs on the same server, signing in as the administrator it made, and
// saves a screenshot of every screen in every look to <out>.
//
//   node e2e/looks-org.mjs http://127.0.0.1:8446 <out>

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "playwright-core";

const [origin = "http://127.0.0.1:8446", out = "looks-org"] = process.argv.slice(2);
// The administrator org.mjs made.
const ANNA = { email: "anna.karimova@example.test", password: "a long first password for anna" };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const step = (text) => console.log(`· ${text}`);

const LOOKS = [
  { name: "off" },
  ...["indigo", "violet", "teal"].flatMap((palette) => ["light", "dark"].map((mode) => ({ name: `${palette}-${mode}`, palette, mode }))),
];

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
  for (const look of LOOKS) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    await context.addInitScript((l) => {
      if (l.palette) {
        localStorage.setItem("ls.features", "themes");
        localStorage.setItem("ls.palette", l.palette);
        localStorage.setItem("ls.mode", l.mode);
      } else {
        localStorage.removeItem("ls.features");
      }
    }, look);
    page = await context.newPage();
    page.on("pageerror", (e) => console.log(`  [pageerror] ${e.message.slice(0, 200)}`));
    let saved = 0;
    const save = async (screen) => {
      saved++;
      await page.evaluate(() => document.fonts.ready);
      await sleep(250);
      await page.screenshot({ path: join(out, `${look.name}-${String(saved).padStart(2, "0")}-${screen}.png`) });
    };

    await page.goto(origin);
    await page.getByRole("button", { name: "Sign in" }).waitFor({ timeout: 15000 });
    const palette = await page.evaluate(() => document.documentElement.dataset.palette ?? null);
    if ((palette ?? undefined) !== look.palette) throw new Error(`${look.name}: the sign-in page carries palette ${palette}`);
    await save("sign-in");
    await page.getByLabel("Email").fill(ANNA.email);
    await page.getByLabel("Password").fill(ANNA.password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await page.getByRole("button", { name: "Admin", exact: true }).waitFor({ timeout: 15000 });
    await sleep(300);
    await save("home");

    await page.getByRole("button", { name: "Account", exact: true }).click();
    await page.getByRole("menu").waitFor({ timeout: 5000 }).catch(() => undefined);
    await save("account-menu");
    await page.keyboard.press("Escape");
    await sleep(200);

    await page.getByRole("button", { name: "Admin", exact: true }).click();
    for (const pane of ["People", "Workspaces"]) {
      await page.locator(".tabs-quiet .tab-quiet", { hasText: pane }).click();
      await sleep(400);
      await save(`admin-${pane.toLowerCase()}`);
    }
    await context.close();
    step(`${look.name}: ${saved} screens saved`);
  }
  console.log(`looks walk on a server: ok (${LOOKS.length} looks, in ${out})`);
} catch (error) {
  if (page) {
    const shownText = await page.evaluate(() => document.body.innerText).catch(() => "(the page could not be read)");
    console.log(`  the page at ${page.url()} showed: ${JSON.stringify(shownText.slice(0, 800))}`);
  }
  throw error;
} finally {
  await browser.close();
}
