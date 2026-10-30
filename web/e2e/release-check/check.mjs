// The release check (docs/DECISIONS.md, 2026-10-30): the files a release
// will publish, on a fresh Windows machine, used as a person would.
//
//   node web/e2e/release-check/check.mjs --package <dir> --previous <dir> --out <dir>
//
// <package> holds the release's installer and SHA256SUMS.txt (already
// checked by the workflow); <previous> holds the last public release's
// installer. The run's summary and screenshots go to <out>. Each item is
// one line of the summary's table; a failed item is said and the check goes
// on where it can.

import fs from "node:fs";
import path from "node:path";
import { chromium } from "playwright-core";
import { Report } from "./report.mjs";
import { APP_LOG, DATA_DIR, INSTALL_DIR, fileIn, installSilently, logSince, sha256, sleep, startApp, stopApp, uninstallKeepingData, until } from "./machine.mjs";

const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce((pairs, a, i, all) => (a.startsWith("--") ? [...pairs, [a.slice(2), all[i + 1]]] : pairs), []),
);
const out = path.resolve(args.out ?? "results");
const report = new Report(out);
const here = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1"));
const repo = path.resolve(here, "..", "..", "..");
const catalog = JSON.parse(fs.readFileSync(path.join(repo, "models", "catalog.json"), "utf8"));
const packageSha = (process.env.PACKAGE_SHA ?? "").slice(0, 8);
const previousTag = process.env.PREVIOUS ?? "the last public release";

const MINUTE = 60000;

/** The app's API, as the installed app's own page uses it. */
function apiOf(url) {
  const token = () => fs.readFileSync(path.join(DATA_DIR, "token"), "utf8").trim();
  const headers = () => ({ Authorization: `Bearer ${token()}`, "content-type": "application/json" });
  return {
    token,
    request: async (body) => (await fetch(`${url}api/v1/request`, { method: "POST", headers: headers(), body: JSON.stringify(body) })).json(),
    get: async (p) => (await fetch(`${url}api/v1${p}`, { headers: headers() })).json(),
  };
}

const errorOf = (answer) => (answer && typeof answer === "object" && "error" in answer ? (answer.error?.message ?? String(answer.error)) : null);

/** Downloads a model through the app and waits until it's installed and checked. */
async function download(api, id) {
  const asked = await api.request({ download_model: { id } });
  if (errorOf(asked)) throw new Error(`the download was refused: ${errorOf(asked)}`);
  let last = "";
  await until(
    `the download of ${id}`,
    async () => {
      const entry = (await api.request("list_model_catalog")).model_catalog?.entries?.find((e) => e.id === id);
      const stage = entry?.download?.stage ?? "";
      if (stage !== last) console.log(`  download of ${id}: ${stage || "(none)"}`);
      last = stage;
      if (stage.startsWith("failed")) throw new Error(`the download failed: ${stage}`);
      // A finished download keeps its last stage ("done", then "checked").
      return entry?.installed && (!entry.download || ["done", "checked"].includes(stage));
    },
    45 * MINUTE,
    3000,
  );
}

async function loadModel(api, id) {
  const asked = await api.request({ load_model: { id } });
  if (errorOf(asked)) throw new Error(`loading was refused: ${errorOf(asked)}`);
  await until(
    `${id} to load`,
    async () => {
      const engine = (await api.get("/environment")).environment?.engine;
      return engine?.running && engine?.model === id && !engine?.loading;
    },
    10 * MINUTE,
    2000,
  );
}

/** Waits until no answer is being written or waits in any chat. */
async function answersDone(api, minutes = 15) {
  await sleep(1500);
  await until("the answer to finish", async () => ((await api.request("list_turns")).turns?.list ?? []).length === 0, minutes * MINUTE, 2000);
}

/** The engine's prompt size and our own count for every answer since `from`. */
function promptSizes(from) {
  return logSince(from)
    .map((l) => l.match(/answer: step \d+, .*prompt of (\d+) tokens \(we counted (\d+)\), (\d+) of (\d+) tokens written/))
    .filter(Boolean)
    .map((m) => ({ engine: Number(m[1]), counted: Number(m[2]), written: Number(m[3]), allowed: Number(m[4]) }));
}

const refusalsSince = (from) => logSince(from).filter((l) => /refused for want of room|the model could not answer/.test(l));

/** The model's main file (the one that isn't the picture file) and its published SHA-256. */
function mainFileOf(id) {
  const entry = catalog.models.find((m) => m.id === id);
  if (!entry) throw new Error(`${id} is not in the catalog`);
  const file = entry.files.find((f) => f !== entry.vision_file) ?? entry.files[0];
  return { file, sha: entry.verify?.[file]?.sha256 };
}

function findFile(dir, name) {
  if (!fs.existsSync(dir)) return null;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      const found = findFile(p, name);
      if (found) return found;
    } else if (e.name === name) return p;
  }
  return null;
}

let browser;
let app;
const page = { current: null };
const shot = async (name) => {
  if (!page.current) return;
  await page.current.screenshot({ path: path.join(out, "screens", `${name}.png`), fullPage: false }).catch(() => undefined);
};

// A failed item leaves a picture of what the page showed.
report.onFail = (n) => shot(`failed-item-${n}`);

try {
  // ---- 1. The last public release, and a chat made in it ----------------
  const previousSetup = fileIn(path.resolve(args.previous), "-setup.exe");
  const setup = fileIn(path.resolve(args.package), "-setup.exe");
  let chat = null;
  let recommended = null;
  await report.item(`${previousTag} installed, a model downloaded and a chat made in it`, async () => {
    installSilently(previousSetup);
    const started = await startApp();
    app = started.app;
    const api = apiOf(started.url);
    recommended = (await api.request("describe_computer")).computer?.recommended ?? null;
    if (!recommended) throw new Error("the previous release recommended no model on this machine");
    await download(api, recommended);
    await loadModel(api, recommended);
    const made = await api.request("new_conversation");
    const conversation = made.conversations?.current;
    const sent = await api.request({ send_message: { text: "Remember the word apricot. Reply with that one word.", conversation } });
    if (errorOf(sent)) throw new Error(`the message was refused: ${errorOf(sent)}`);
    await answersDone(api);
    chat = { conversation, words: "apricot" };
    await stopApp(app);
    app = null;
    return `${recommended} downloaded and loaded; one chat with an answer`;
  });

  // ---- 2. This release installed over it --------------------------------
  let started = null;
  let api = null;
  const updated = await report.item("Installed over the last public release, silently", async () => {
    installSilently(setup);
    started = await startApp();
    app = started.app;
    api = apiOf(started.url);
    // Both releases say "0.2.0"; the build id on the log's first line names the commit.
    const first = logSince(started.since).find((l) => /localSpace \S+ \(/.test(l)) ?? "";
    const build = first.match(/localSpace (\S+) \((\w+)\)/);
    if (!build) throw new Error("the log does not name the build");
    if (packageSha && build[2] !== packageSha) throw new Error(`the running build is ${build[2]}, not the package's ${packageSha}`);
    const me = await api.get("/me");
    if (me.version !== build[1]) throw new Error(`the app says ${me.version}, the log ${build[1]}`);
    return `the app says ${me.version}, build ${build[2]}`;
  });
  if (!updated) throw new Error("the release could not be installed and started; nothing more can be checked");

  await report.item("The chat made in the last public release is kept", async () => {
    if (!chat) throw new Error("no chat was made before the update");
    const list = (await api.request("list_conversations")).conversations?.list ?? [];
    const kept = list.find((c) => c.id === chat.conversation);
    if (!kept) throw new Error("the chat is gone");
    await api.request({ select_conversation: { id: chat.conversation } });
    const messages = (await api.request("get_transcript")).transcript?.messages ?? [];
    if (!messages.some((m) => m.role === "user" && m.content.includes(chat.words))) throw new Error("the chat's message is gone");
    if (!messages.some((m) => m.role === "assistant" && m.content.trim())) throw new Error("the chat's answer is gone");
    return `"${kept.title}", ${messages.length} messages`;
  });

  // ---- 3. The hardware check, and its model downloaded through the app ---
  let model = null;
  await report.item("The hardware check chooses a model for this machine", async () => {
    const computer = (await api.request("describe_computer")).computer;
    model = computer?.recommended ?? null;
    const found = logSince(started.since).filter((l) => /computer: |the computer as found: /.test(l)).slice(-2).map((l) => l.replace(/^.*?: (computer|the computer)/, "$1"));
    for (const line of found) report.note(`The hardware check: ${line.slice(0, 300)}`);
    if (!model) throw new Error("no model is recommended");
    return `${model} (${computer.sentence})`;
  });
  if (!model) model = recommended;

  await report.item("That model downloaded through the app and checked against the catalog's SHA-256", async () => {
    // Downloaded again by this release: what the last one fetched goes first.
    const removed = await api.request({ delete_model: { id: model } });
    if (errorOf(removed)) throw new Error(`deleting the earlier copy was refused: ${errorOf(removed)}`);
    const from = Date.now() - 1000;
    await download(api, model);
    if (!logSince(from).some((l) => l.includes(`the download of ${model} is done and checked`))) throw new Error("the app did not say the download was checked");
    const { file, sha } = mainFileOf(model);
    const where = findFile(path.join(DATA_DIR, "models"), file);
    if (!where) throw new Error(`${file} is not in the data folder`);
    if (sha && sha256(where) !== sha.toLowerCase()) throw new Error(`${file} does not match the catalog's SHA-256`);
    return sha ? `${file} matches ${sha.slice(0, 12)}…` : `${file} downloaded (the catalog gives no SHA-256 for it)`;
  });

  await report.item("The model loads on the processor", async () => {
    await loadModel(api, model);
    const env = (await api.get("/environment")).environment;
    const fit = logSince(started.since).filter((l) => l.includes("fit: ")).slice(-1)[0] ?? "";
    if (fit) report.note(`The fit: ${fit.replace(/^.*fit: /, "")}`);
    return `context ${env.model?.context_len}, pictures ${env.model?.supports_vision ? "on" : "off"}`;
  });

  // ---- 4. The walk, in the browser the app's window uses -----------------
  browser = await chromium.launch({ channel: "msedge", headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 860 }, locale: "en-US" });
  page.current = await context.newPage();
  const p = page.current;
  await api.request({ set_preference: { key: "language", value: "en" } });
  await p.goto(`${started.url}?token=${api.token()}`);
  const box = p.getByRole("textbox", { name: "Message" });
  const notNow = p.getByRole("button", { name: "Not now", exact: true });
  await until("the chat or the first run", async () => (await box.count()) + (await notNow.count()) > 0, 2 * MINUTE);
  if (await notNow.count()) await notNow.click();
  const inTheBox = () => p.evaluate(() => document.activeElement?.getAttribute("aria-label") === "Message");
  const lines = p.locator(".message-stopped");

  await report.item("The cursor: in the message box on opening, on a new chat, after sending", async () => {
    await until("the cursor when the app opens", inTheBox, 10000);
    await p.locator(".rail-new").click();
    await until("the cursor on a new chat", inTheBox, 10000);
    await box.fill("Say hello in three words.");
    await p.getByRole("button", { name: "Send", exact: true }).click();
    await until("the cursor after sending", inTheBox, 10000);
    await answersDone(api);
    return "all three";
  });

  await report.item("The wording: the top bar's chips are buttons into Settings", async () => {
    const chip = p.getByRole("button", { name: /Online for downloads only|Offline/ });
    await until("the network chip", async () => (await chip.count()) === 1, 10000);
    await chip.click();
    await until("Settings → Network", async () => (await p.locator("h1.pane-title").count()) > 0, 10000);
    const title = await p.locator("h1.pane-title").first().innerText();
    await shot("en-settings-network");
    await p.locator(".rail-chat").first().click();
    await until("the chat again", async () => (await box.count()) === 1, 10000);
    return `the chip opens "${title}"`;
  });

  await report.item("Stop, then Continue", async () => {
    // Nothing else being written: the essay is answered at once, not queued.
    await answersDone(api);
    await p.locator(".rail-new").click();
    await until("an empty chat", async () => (await p.locator(".message").count()) === 0, 10000);
    await box.fill("Write a long essay about the history of tea, at least 600 words.");
    await box.press("Enter");
    const stop = p.getByRole("button", { name: "Stop", exact: true });
    const streamed = async () => (await p.locator(".message .prose-chat").last().innerText().catch(() => "")).length;
    await until("the essay's first words", async () => (await stop.count()) === 1 && (await streamed()) > 40, 10 * MINUTE, 500);
    await stop.click({ timeout: 10000 });
    await until("the stopped line", async () => (await lines.last().innerText().catch(() => "")).includes("The answer stopped here."), MINUTE);
    const before = (await p.locator(".prose-chat").last().innerText()).length;
    await lines.last().getByRole("button", { name: "Continue", exact: true }).click();
    await until("the answer carried on", async () => (await p.locator(".prose-chat").last().innerText()).length > before + 20, 10 * MINUTE, 1000);
    await api.request({ cancel_turn: { conversation: null } });
    await answersDone(api);
    await shot("en-chat-stopped-and-continued");
    return "stopped at once, and carried on";
  });

  await report.item("The language setting: Russian, then Uzbek, kept after a reload", async () => {
    const saidIn = {};
    for (const [language, label, chipText] of [
      ["ru", "Русский", "В сети только для загрузок"],
      ["uz", "Oʻzbekcha", "Faqat yuklab olish uchun onlayn"],
    ]) {
      await p.locator("button.rail-item", { hasText: /Settings|Настройки|Sozlamalar/ }).first().click();
      const select = p.locator("select").first();
      await until("the language choice", async () => (await select.count()) > 0, 10000);
      await select.selectOption({ label });
      await p.reload();
      await until(`the chip in ${language}`, async () => (await p.getByRole("button", { name: chipText }).count()) === 1, 30000);
      saidIn[language] = chipText;
    }
    await api.request({ set_preference: { key: "language", value: "en" } });
    await p.reload();
    await until("English again", async () => (await p.getByRole("button", { name: /Online for downloads only|Offline/ }).count()) === 1, 30000);
    return `"${saidIn.ru}", "${saidIn.uz}"`;
  });

  await report.item("The Help page's link opens the feedback group", async () => {
    await p.locator("button.rail-item", { hasText: "Help" }).first().click();
    const link = p.locator("a.link", { hasText: /Telegram/ }).first();
    await until("the link", async () => (await link.count()) === 1, 10000);
    const href = await link.getAttribute("href");
    const target = await link.getAttribute("target");
    if (href !== "https://t.me/localSpace_discussion") throw new Error(`the link goes to ${href}`);
    if (target !== "_blank") throw new Error("the link would open inside the app");
    await shot("en-help");
    return href;
  });

  // ---- 5. Items 12 and 13 on the processor ------------------------------
  const ask = async (text, file) => {
    await p.locator(".rail-chat").first().waitFor({ timeout: 10000 }).catch(() => undefined);
    if (file) {
      // Sent only once the file's chip is there and its upload is done:
      // sent sooner, the message goes without it.
      const chips = p.locator(".composer-picture, .composer-file");
      const before = await chips.count();
      await p.locator('input[type="file"]').setInputFiles(file);
      await until("the file's chip", async () => (await chips.count()) > before, MINUTE);
      await until("the file to attach", async () => (await p.locator(".composer-pictures .uploading").count()) === 0, 2 * MINUTE);
      if ((await chips.count()) <= before) {
        const said = await p.locator(".composer-said").allInnerTexts();
        throw new Error(`the file was not attached: ${said.join(" ") || "nothing said"}`);
      }
    }
    await box.fill(text);
    await box.press("Enter");
    await answersDone(api);
    return (await p.locator(".prose-chat").last().innerText().catch(() => "")).trim();
  };

  await report.item("Item 12: a full Uzbek chat with a text file: no refusal", async () => {
    const from = Date.now() - 1000;
    await p.locator(".rail-chat").first().click().catch(() => undefined);
    await p.locator(".rail-new").click();
    const text = path.join(here, "uzbek-text.txt");
    const questions = [
      ["Bu matnning qisqacha mazmunini oʻzbek tilida, uch gap bilan ayting.", text],
      ["Bu matnni yana bir bor oʻqing: karvonsaroylar nima uchun kerak boʻlgan? Ikki gap bilan javob bering.", text],
      ["Matnni uchinchi marta oʻqing: mirobning vazifasi nima edi? Ikki gap bilan javob bering.", text],
      ["Bugungi Samarqand qanday vazifalarga duch kelmoqda? Ikki gap bilan javob bering.", null],
    ];
    let divider = false;
    for (const [question, file] of questions) {
      const answer = await ask(question, file);
      report.answer("Uzbek", question, answer || "(no words)");
      divider = (await p.locator(".chat-divider").count()) > 0;
      if (divider) break;
    }
    await shot("en-chat-uzbek-with-file");
    const sizes = promptSizes(from);
    sizes.forEach((s, i) => report.figure(`Item 12, answer ${i + 1}: the engine's prompt / our count / written of allowed`, `${s.engine} / ${s.counted} / ${s.written} of ${s.allowed} (context 8,192: ${8192 - s.engine} left)`));
    const refused = refusalsSince(from);
    if (refused.length) throw new Error(`refused: ${refused[0].replace(/^.*answer: /, "")}`);
    const cut = ((await api.request("get_transcript")).transcript?.messages ?? []).find((m) => ["chat_full", "too_long", "failed"].includes(m.cut_short));
    if (cut) throw new Error(`an answer ended as ${cut.cut_short}`);
    return divider ? "no refusal; the older messages line moved" : "no refusal; the older messages line didn't move within four questions";
  });

  await report.item("Item 13: one picture's real prompt size", async () => {
    const picture = path.join(repo, "docs", "screenshot.png");
    // What the app says of the file when it's sent to it directly.
    const direct = await fetch(`${started.url}api/v1/attachments?name=screenshot.png`, {
      method: "POST",
      headers: { Authorization: `Bearer ${api.token()}`, "content-type": "image/png" },
      body: fs.readFileSync(picture),
    }).then((r) => r.text());
    report.note(`The picture sent to the app directly: ${direct.slice(0, 300)}`);
    const from = Date.now() - 1000;
    await p.locator(".rail-new").click();
    const answer = await ask("What is in this picture? Answer in two sentences.", picture);
    if (!answer) throw new Error("no answer about the picture");
    await shot("en-chat-picture");
    const size = promptSizes(from).slice(-1)[0];
    if (!size) throw new Error("the log does not give the prompt's size");
    // A picture alone is some hundreds of tokens; a prompt this short had none.
    if (size.engine < 250) throw new Error(`the picture did not reach the model (a prompt of ${size.engine} tokens)`);
    report.figure("Item 13: one picture and a short question, the engine's prompt / our count (a picture counted as 400)", `${size.engine} / ${size.counted}`);
    report.answer("English", "What is in this picture?", answer);
    return `the engine read ${size.engine} tokens; we counted ${size.counted}`;
  });

  await report.item("Answers in Russian and Uzbek stay in their language and alphabet", async () => {
    await p.locator(".rail-new").click();
    const ru = await ask("Привет! Расскажи в двух предложениях о Самарканде.");
    report.answer("Russian", "Привет! Расскажи в двух предложениях о Самарканде.", ru || "(no words)");
    await p.locator(".rail-new").click();
    const uz = await ask("Salom! Samarqand haqida ikki gap yozing.");
    report.answer("Uzbek", "Salom! Samarqand haqida ikki gap yozing.", uz || "(no words)");
    const cyrillic = (s) => (s.match(/[Ѐ-ӿ]/g) ?? []).length;
    const latin = (s) => (s.match(/[A-Za-z]/g) ?? []).length;
    if (cyrillic(ru) < latin(ru)) throw new Error("the Russian answer is mostly not in Cyrillic");
    if (latin(uz) < cyrillic(uz)) throw new Error("the Uzbek answer is mostly not in Latin letters");
    return "Russian in Cyrillic, Uzbek in Latin letters";
  });

  // ---- 6. Every screen in English, Russian and Uzbek --------------------
  await report.item("Screenshots of every screen in English, Russian and Uzbek", async () => {
    let taken = 0;
    for (const language of ["en", "ru", "uz"]) {
      await api.request({ set_preference: { key: "language", value: language } });
      await p.reload();
      await until("the page", async () => (await p.locator(".rail-new").count()) === 1, 30000);
      const take = async (name) => {
        await sleep(800);
        await shot(`${language}-${name}`);
        taken++;
      };
      await p.locator(".rail-chat").first().click();
      await take("chat");
      await p.locator(".rail-new").click();
      await take("new-chat");
      const settings = p.locator("button.rail-item").filter({ hasText: /Settings|Настройки|Sozlamalar/ }).first();
      await settings.click();
      const sections = p.locator('nav.subnav[aria-label="Settings"] button.sub');
      const count = await sections.count();
      for (let i = 0; i < count; i++) {
        await sections.nth(i).click();
        await take(`settings-${i + 1}`);
      }
      const advanced = p.locator("button.sub.quiet");
      if (await advanced.count()) {
        await advanced.first().click();
        await take("settings-advanced");
      }
      const help = p.locator("button.rail-item").filter({ hasText: /Help|Помощь|Yordam/ }).first();
      if (await help.count()) {
        await help.click();
        await take("help");
      }
    }
    await api.request({ set_preference: { key: "language", value: "en" } });
    return `${taken} screenshots`;
  });

  // ---- 7. Uninstalling keeps the person's data --------------------------
  await browser.close();
  browser = null;
  await stopApp(app);
  app = null;
  await report.item("Uninstalling removes the program and keeps the data", async () => {
    if (!(await uninstallKeepingData())) throw new Error("the data went with the program");
    return `${INSTALL_DIR} gone, ${DATA_DIR} kept`;
  });
} catch (error) {
  report.row("The check ran to its end", false, String(error?.message ?? error).split("\n")[0]);
} finally {
  if (browser) await browser.close().catch(() => undefined);
  if (app) await stopApp(app).catch(() => undefined);
  if (fs.existsSync(APP_LOG)) fs.copyFileSync(APP_LOG, path.join(out, "app.log"));
  const rotated = `${APP_LOG}.1`;
  if (fs.existsSync(rotated)) fs.copyFileSync(rotated, path.join(out, "app.log.1"));
  report.write();
}
process.exit(report.passed ? 0 : 1);
