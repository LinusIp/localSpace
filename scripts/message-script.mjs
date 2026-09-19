// The message script of the laptop test (docs/DECISIONS.md, 2026-09-19, the
// answers after day 2): the things ten ordinary people type on a Friday
// afternoon, put to a model through Core's own API exactly as the app puts
// them, the same script for every model so that the results compare. Not
// eval cases: nothing is scored here. What came back, how long it took and
// which tools were reached for is written down for a person to judge.
//
//   node scripts/message-script.mjs <origin> <token> <model id> [--out <file.md>]
//
// Needs a running `localspace serve --personal` with the model on disk; use a
// fresh data folder, which is what a tester has. One model at a time.
//
// A model may only be the default if it has been through this script on some
// machine and a person has read what came back: that day is the entry's
// `script_run` in models/catalog.json, and the answers belong in
// docs/test-a/MESSAGE-SCRIPT.md under a heading with the model's title.

import { writeFileSync } from "node:fs";

const args = process.argv.slice(2);
const outAt = args.indexOf("--out");
const out = outAt >= 0 ? args[outAt + 1] : undefined;
const [origin, token, model] = args.filter((a, i) => !a.startsWith("--") && i !== outAt + 1);
if (!origin || !token || !model) {
  console.error("usage: node scripts/message-script.mjs <origin> <token> <model id> [--out <file.md>]");
  process.exit(2);
}

const LONG_TEXT = `The town of Harrowfield sits where two rivers meet, and for most of its history it lived from the water. In 1847 a wooden footbridge was the only crossing, and the ferryman, a man called Tobias Wren, charged a penny a head. The railway arrived in 1869 and with it the first brick warehouses along the east bank. By 1890 the town had three mills, a brewery and a population of eleven thousand. The great flood of March 1912 carried away the footbridge, two of the mills and forty-one houses; nobody died, because the miller's daughter, Ada Pellow, saw the water rising at four in the morning and rang the chapel bell until the street was awake. The stone bridge that replaced the footbridge was opened in 1915 and still carries the main road. After the second war the mills closed one by one, the last in 1971, and the warehouses stood empty until the 1990s, when they were turned into flats and workshops. Today the town has about nineteen thousand people, a weekly market on Thursdays, and a small museum in the old brewery whose most visited exhibit is the chapel bell.`;

const SCRIPT = [
  { name: "a greeting", turns: ["Hi there!"] },
  { name: "what can you do", turns: ["What can you do?"] },
  { name: "a factual question", turns: ["How far is the Moon from the Earth?"] },
  { name: "a short email", turns: ["Write a short, polite email to my landlord asking him to fix the leaking tap in the kitchen."] },
  {
    name: "something to summarise",
    turns: [`Summarise this in two sentences:\n\n${LONG_TEXT}`],
  },
  { name: "a question about a detail of a longer text", turns: [`Read this and then answer: who rang the bell, and in which year?\n\n${LONG_TEXT}`] },
  { name: "arithmetic", turns: ["What is 17 times 24?"] },
  { name: "a word problem", turns: ["I have 3 boxes with 12 eggs in each, and I break 5 eggs. How many whole eggs do I have left?"] },
  { name: "a translation", turns: ["Translate into German: Where is the nearest train station?"] },
  {
    name: "make this shorter",
    turns: ["Make this shorter: I am writing to let you know that, due to circumstances which are unfortunately outside of my control, I will regrettably not be able to attend the meeting that has been scheduled for Thursday afternoon."],
  },
  { name: "a question in Spanish", turns: ["¿Cuál es la capital de Argentina y por qué es conocida?"] },
  { name: "a question in Russian", turns: ["Какая столица Франции и чем она знаменита?"] },
  { name: "something deliberately vague", turns: ["Can you help me with my thing?"] },
  { name: "a packing list", turns: ["Give me a packing list for a three-day hiking trip in autumn."] },
  { name: "a little code", turns: ["Write a Python function that checks whether a number is prime."] },
  { name: "something it cannot know", turns: ["What is the weather like in Lisbon today?"] },
  {
    name: "three turns, each depending on the one before",
    turns: ["Give me three ideas for a weekend trip from Berlin, one line each.", "Tell me more about the second one.", "Roughly what would that cost for two people?"],
  },
];

const headers = { Authorization: `Bearer ${token}`, "content-type": "application/json" };
const request = async (body, ms = 280000) => {
  const abort = new AbortController();
  const timer = setTimeout(() => abort.abort(), ms);
  try {
    const res = await fetch(`${origin}/api/v1/request`, { method: "POST", headers, body: JSON.stringify(body), signal: abort.signal });
    return await res.json();
  } catch (e) {
    return { error: e.name === "AbortError" ? `no answer within ${ms / 1000} s` : String(e) };
  } finally {
    clearTimeout(timer);
  }
};
const environment = async () => (await (await fetch(`${origin}/api/v1/environment`, { headers })).json()).environment;

const began = Date.now();
const loaded = await request({ load_model: { id: model } });
if (loaded && typeof loaded === "object" && "error" in loaded) throw new Error(`the model could not be loaded: ${JSON.stringify(loaded.error)}`);
while (!(await environment()).engine.running) {
  if (Date.now() - began > 600000) throw new Error("the engine did not come up in ten minutes");
  await new Promise((r) => setTimeout(r, 500));
}
const readyAfter = (Date.now() - began) / 1000;
const entry = ((await request("list_model_catalog")).model_catalog?.entries ?? []).find((m) => m.id === model);

const results = [];
for (const item of SCRIPT) {
  await request("new_conversation");
  const turns = [];
  for (const text of item.turns) {
    const asked = Date.now();
    const answer = await request({ send_message: { text } });
    const seconds = (Date.now() - asked) / 1000;
    const messages = answer.transcript?.messages ?? [];
    // What this turn added: everything after the last message of the person.
    const lastUser = messages.map((m) => m.role).lastIndexOf("user");
    const since = messages.slice(lastUser + 1);
    const reply = since.filter((m) => m.role === "assistant").pop()?.content?.trim() ?? "";
    const tools = since.flatMap((m) => (m.tool_calls ?? []).map((c) => c.tool));
    turns.push({ text, seconds, reply, tools, error: answer.error ? JSON.stringify(answer.error) : undefined });
    console.error(`${item.name}: ${seconds.toFixed(1)} s${tools.length ? `, tools: ${tools.join(", ")}` : ""}${reply ? "" : "  — NO REPLY"}`);
  }
  results.push({ name: item.name, turns });
}
await request("unload_model");

const cell = (s) => s.replace(/\|/g, "\\|").replace(/\s*\n\s*/g, " ⏎ ").trim();
const lines = [];
lines.push(`### ${entry?.title ?? model}`);
lines.push("");
lines.push(`\`${model}\`, through Core on a fresh data folder. Ready after ${readyAfter.toFixed(0)} s. The app says of it here: ${entry ? `"${entry.verdict_label}${entry.speed ? ` · ${entry.speed}` : ""}"; ${entry.placement}` : "nothing (not in the catalog)"}`);
lines.push("");
lines.push("| Message | Time | Tools reached for | What came back |");
lines.push("|---|---|---|---|");
for (const item of results) {
  item.turns.forEach((t, i) => {
    const label = item.turns.length > 1 ? `${item.name} (${i + 1}): ${t.text}` : `${item.name}: ${t.text.length > 90 ? `${t.text.slice(0, 90)}…` : t.text}`;
    const back = t.error ? `**${t.error}**` : t.reply ? t.reply : "**no reply**";
    lines.push(`| ${cell(label)} | ${t.seconds.toFixed(1)} s | ${t.tools.length ? t.tools.join(", ") : "none"} | ${cell(back)} |`);
  });
}
lines.push("");
const report = lines.join("\n");
if (out) writeFileSync(out, report);
else console.log(report);
