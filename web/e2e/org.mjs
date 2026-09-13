// Phase A's gate on an organisation server (docs/PILOT-1.md, Phase A): two
// people in two browsers on one server see different personal workspaces
// and the same shared board, with edits crossing between them and each
// seeing the other on it; a viewer's edit is refused server-side, from the
// API and from the board itself; and every action above is in the audit
// log under the right person. Runs against a `localspace serve` on an
// empty data directory whose settings name the organisation; needs Edge or
// Chrome on the machine, nothing downloaded.
//
//   node e2e/org.mjs http://127.0.0.1:8446 --data <the server's storage root>

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "playwright-core";

const args = process.argv.slice(2);
const positional = args.filter((a) => !a.startsWith("--"));
const [origin = "http://127.0.0.1:8446"] = positional;
const dataAt = args.indexOf("--data");
const data = dataAt >= 0 ? args[dataAt + 1] : null;
if (!data) throw new Error("--data <dir> is required: the first administrator's link and the audit log are read from it");

const ORGANISATION = "Meridian Bank";
const WHITEBOARD = "io.localspace.whiteboard";
const people = {
  anna: { name: "Anna Karimova", email: "anna.karimova@example.test", password: "a long first password for anna" },
  bek: { name: "Bek Yusupov", email: "bek.yusupov@example.test", password: "a long password for bek too" },
  vera: { name: "Vera Sultanova", email: "vera.sultanova@example.test", password: "a long password for vera as well" },
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const step = (text) => console.log(`· ${text}`);
const until = async (what, check, ms = 15000) => {
  const start = Date.now();
  for (;;) {
    const value = await check();
    if (value) return value;
    if (Date.now() - start > ms) throw new Error(`timed out waiting for ${what}`);
    await sleep(250);
  }
};
const expect = (ok, what) => {
  if (!ok) throw new Error(what);
};

/** The API as the person a browser context is signed in as: its cookies go with every call. */
function asUser(ctx) {
  const post = async (path, body) => {
    const r = await ctx.request.post(`${origin}${path}`, { data: JSON.stringify(body), headers: { "content-type": "application/json" } });
    return r.json();
  };
  return {
    request: (body) => post("/api/v1/request", body),
    me: async () => (await ctx.request.get(`${origin}/api/v1/me`)).json(),
    setPassword: (body) => post("/api/v1/auth/set-password", body),
    login: (email, password) => post("/api/v1/auth/login", { email, password }),
  };
}

/** The board's frame in a page, with its editor reachable. */
async function openBoard(page, teamId) {
  await page.goto(origin);
  await page.getByRole("button", { name: "Whiteboard", exact: true }).waitFor({ timeout: 15000 });
  await page.getByLabel("Workspace").selectOption(teamId);
  await until("the workspace to switch", async () => (await page.getByLabel("Workspace").inputValue()) === teamId);
  await page.getByRole("button", { name: "Whiteboard", exact: true }).first().click();
  const frameEl = page.locator("iframe[title='Board']");
  await frameEl.waitFor({ state: "visible", timeout: 15000 });
  const frame = await (await frameEl.elementHandle()).contentFrame();
  await frame.waitForFunction(() => Boolean(window.__localspace), null, { timeout: 30000 });
  return frame;
}

const textIn = (frame, id) => frame.evaluate((i) => window.__localspace.editor.scene.get(i)?.text ?? null, id);
const onBoard = async (page) => (await page.getByRole("img", { name: /^On this board/ }).getAttribute("aria-label").catch(() => "")) ?? "";

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

const viewport = { width: 1440, height: 900 };
try {
  // 1. The first administrator, from the link the server wrote at start.
  const linkFile = readFileSync(join(data, "first-admin-link.txt"), "utf8");
  const token = /\/invite\/([0-9a-f]+)/.exec(linkFile)?.[1];
  expect(token, "the first administrator's link is not in first-admin-link.txt");
  const annaCtx = await browser.newContext({ viewport });
  const anna = asUser(annaCtx);
  const made = await anna.setPassword({ token, password: people.anna.password, email: people.anna.email, name: people.anna.name });
  expect(made.user?.email === people.anna.email, `the first administrator was not made: ${JSON.stringify(made).slice(0, 200)}`);
  const annaMe = await anna.me();
  expect(annaMe.roles.includes("admin"), "the first administrator is not an admin");
  step(`the first administrator: ${annaMe.name}, from the link in the data directory`);

  // 2. The whiteboard for everyone, two more people, and a shared workspace.
  const installed = await anna.request({ install_harness: { path: "harnesses/whiteboard" } });
  expect(installed === "ok" || installed.ok !== undefined || installed.environment, `install failed: ${JSON.stringify(installed).slice(0, 200)}`);
  const invite = async (who, role) => {
    const r = await anna.request({ create_user: { email: who.email, name: who.name, roles: [role] } });
    expect(r.invite?.token, `no invite for ${who.name}: ${JSON.stringify(r).slice(0, 200)}`);
    return r.invite;
  };
  const bekInvite = await invite(people.bek, "member");
  const veraInvite = await invite(people.vera, "viewer");
  const bekCtx = await browser.newContext({ viewport });
  const veraCtx = await browser.newContext({ viewport });
  const bek = asUser(bekCtx);
  const vera = asUser(veraCtx);
  await bek.setPassword({ token: bekInvite.token, password: people.bek.password });
  await vera.setPassword({ token: veraInvite.token, password: people.vera.password });
  const bekMe = await bek.me();
  const veraMe = await vera.me();
  expect(bekMe.roles.includes("member") && veraMe.roles.includes("viewer"), "the roles are not as invited");
  const made2 = await anna.request({ create_workspace: { name: "Team" } });
  const team = made2.workspaces?.find((w) => w.name === "Team");
  expect(team, `no Team workspace: ${JSON.stringify(made2).slice(0, 200)}`);
  for (const [who, level] of [
    [bekMe, "edit"],
    [veraMe, "view"],
  ]) {
    const r = await anna.request({ set_member: { workspace: team.id, principal: { user: who.user }, level } });
    expect(r.workspaces, `adding ${who.name} failed: ${JSON.stringify(r).slice(0, 200)}`);
  }
  step(`invited ${people.bek.name} (member) and ${people.vera.name} (viewer); Team holds both`);

  // 3. Each person is in a personal workspace of their own.
  const personalOf = async (user) => {
    const env = (await user.request("get_environment")).environment;
    const list = (await user.request("list_workspaces")).workspaces;
    const current = list.find((w) => w.id === env.workspace_id);
    return { id: env.workspace_id, personal_to: current?.personal_to ?? null };
  };
  const [pa, pb, pv] = await Promise.all([personalOf(anna), personalOf(bek), personalOf(vera)]);
  expect(pa.personal_to === annaMe.user && pb.personal_to === bekMe.user && pv.personal_to === veraMe.user, `not everyone starts in their own personal workspace: ${JSON.stringify([pa, pb, pv])}`);
  expect(new Set([pa.id, pb.id, pv.id]).size === 3, "the personal workspaces are not three different ones");
  step("three people, three different personal workspaces, each their own");

  // 4. Bek signs in through the sign-in page in a fresh browser: the page
  // names the organisation, and so does the tab.
  const bekBrowser = await browser.newContext({ viewport });
  const bekPage = await bekBrowser.newPage();
  await bekPage.goto(origin);
  await bekPage.getByRole("button", { name: "Sign in" }).waitFor({ timeout: 15000 });
  expect((await bekPage.title()) === `localSpace · ${ORGANISATION}`, `the tab is named ${await bekPage.title()}`);
  expect(await bekPage.getByText(ORGANISATION, { exact: true }).isVisible(), "the sign-in page does not name the organisation");
  await bekPage.getByLabel("Email").fill(people.bek.email);
  await bekPage.getByLabel("Password").fill(people.bek.password);
  await bekPage.getByRole("button", { name: "Sign in" }).click();
  await bekPage.getByRole("button", { name: "Whiteboard", exact: true }).waitFor({ timeout: 15000 });
  step(`Bek signed in through the sign-in page, which names ${ORGANISATION}; so does the tab`);

  // 5. The same board in two browsers, each seeing the other on it.
  const annaPage = await annaCtx.newPage();
  const annaFrame = await openBoard(annaPage, team.id);
  const bekFrame = await openBoard(bekPage, team.id);
  await until(
    "both to see each other on the board",
    async () => {
      const a = await onBoard(annaPage);
      const b = await onBoard(bekPage);
      return a.includes(people.anna.name) && a.includes(people.bek.name) && b.includes(people.anna.name) && b.includes(people.bek.name);
    },
    30000,
  );
  step("both browsers show Anna and Bek on the board");

  // 6. Edits cross: a note Anna makes and types into reaches Bek; Bek's change reaches Anna.
  const host = annaFrame.locator(".board-host");
  const box = await host.boundingBox();
  await annaFrame.getByRole("button", { name: /Sticky note/ }).click();
  await host.click({ position: { x: box.width * 0.5, y: box.height * 0.5 } });
  await sleep(300);
  const noteId = await annaFrame.evaluate(() => [...window.__localspace.editor.selection][0] ?? null);
  expect(noteId, "Anna's click did not make a note");
  await annaPage.keyboard.type("from Anna");
  await annaPage.keyboard.press("Escape");
  await until("Bek to see Anna's note", async () => (await textIn(bekFrame, noteId)) === "from Anna", 20000);
  await bekFrame.evaluate((id) => window.__localspace.editor.setText(id, "from Bek"), noteId);
  await until("Anna to see Bek's change", async () => (await textIn(annaFrame, noteId)) === "from Bek", 20000);
  step("a note made and typed by Anna reached Bek; Bek's change to it reached Anna");

  // 7. A read-only account is refused, server-side: from the API, and from the board.
  await vera.request({ select_workspace: { workspace: team.id, reason: null } });
  const denied = await vera.request({ call_tool: { tool: "canvas.add_sticky", params: { text: "from Vera" } } });
  expect(denied.tool_result?.denied, `Vera's tool call was not denied: ${JSON.stringify(denied).slice(0, 200)}`);
  const written = await vera.request({ write_doc: { harness: WHITEBOARD, view: "web", doc: { shapes: [] }, commit: true } });
  expect(written.error, `Vera's write was not refused: ${JSON.stringify(written).slice(0, 200)}`);
  // Her own personal board is no exception: the account is read-only everywhere.
  await vera.request({ select_workspace: { workspace: pv.id, reason: null } });
  const ownWrite = await vera.request({ write_doc: { harness: WHITEBOARD, view: "web", doc: { shapes: [] }, commit: true } });
  expect(ownWrite.error, `Vera's write to her own board was not refused: ${JSON.stringify(ownWrite).slice(0, 200)}`);
  await vera.request({ select_workspace: { workspace: team.id, reason: null } });
  const veraPage = await veraCtx.newPage();
  const veraFrame = await openBoard(veraPage, team.id);
  await until("Vera to see the board", async () => (await textIn(veraFrame, noteId)) === "from Bek", 20000);
  await veraFrame.evaluate((id) => window.__localspace.editor.setText(id, "Vera was here"), noteId);
  await until("the refusal to be shown to Vera", async () => (await veraPage.locator(".toast.error").allInnerTexts()).some((t) => /could not sync/.test(t)), 20000);
  await sleep(1500);
  const bekSees = await textIn(bekFrame, noteId);
  const coreHolds = (await anna.request({ get_doc_json: { harness: WHITEBOARD } })).doc_json.json.shapes.find((s) => s.id === noteId)?.text;
  expect(bekSees === "from Bek" && coreHolds === "from Bek", `the viewer's edit got through: Bek sees ${JSON.stringify(bekSees)}, Core holds ${JSON.stringify(coreHolds)}`);
  step("Vera, whose account can only view: refused from the API and from the board, in Team and on her own board; nothing of hers reached Core");

  // 8. A member given only "can view" on the workspace is refused the same way, by the access level.
  await anna.request({ set_member: { workspace: team.id, principal: { user: bekMe.user }, level: "view" } });
  const bekWrite = await bek.request({ write_doc: { harness: WHITEBOARD, view: "web", doc: { shapes: [] }, commit: true } });
  expect(bekWrite.error, `Bek's write after his access was lowered was not refused: ${JSON.stringify(bekWrite).slice(0, 200)}`);
  await bekFrame.evaluate((id) => window.__localspace.editor.setText(id, "Bek, view only"), noteId);
  await until("the refusal to be shown to Bek", async () => (await bekPage.locator(".toast.error").allInnerTexts()).some((t) => /could not sync/.test(t)), 20000);
  await sleep(1000);
  const stillHeld = (await anna.request({ get_doc_json: { harness: WHITEBOARD } })).doc_json.json.shapes.find((s) => s.id === noteId)?.text;
  expect(stillHeld === "from Bek", `the lowered member's edit got through: Core holds ${JSON.stringify(stillHeld)}`);
  step("Bek, lowered to view on Team: his write is refused by the access level, from the API and from the board");

  // 9. The audit log names the right person for each of the above.
  const records = readdirSync(join(data, "audit"))
    .filter((f) => f.endsWith(".jsonl"))
    .sort()
    .flatMap((f) => readFileSync(join(data, "audit", f), "utf8").split("\n").filter(Boolean).map((line) => JSON.parse(line)));
  const by = (user, event, result) => records.filter((r) => r.actor?.user === user && r.event === event && (result === undefined || r.result === result));
  expect(by(annaMe.user, "user.create").length >= 2, "Anna's two invitations are not in the audit log under her");
  expect(by(bekMe.user, "auth.login").length >= 1, "Bek's sign-in is not in the audit log under him");
  expect(by(veraMe.user, "tool.call", "denied").length >= 1, "Vera's refused tool call is not in the audit log under her as denied");
  expect(by(veraMe.user, "document.write", "denied").length >= 2, "Vera's refused writes are not in the audit log under her as denied");
  const actors = new Set(records.map((r) => r.actor?.user));
  expect(actors.has(annaMe.user) && actors.has(bekMe.user) && actors.has(veraMe.user), "not every person appears as an actor in the audit log");
  const events = {};
  for (const r of records) events[`${r.event}:${r.result}`] = (events[`${r.event}:${r.result}`] ?? 0) + 1;
  step(`${records.length} audit records under ${actors.size} actors: ${Object.entries(events).map(([k, n]) => `${k}×${n}`).join(", ")}`);

  console.log("PASS");
} catch (err) {
  console.log(`FAIL: ${err.message}`);
  process.exitCode = 1;
} finally {
  await browser.close();
}
